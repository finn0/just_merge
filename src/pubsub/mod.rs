use std::{ops::Deref, sync::Arc};

use gitlab::User;
use log::{error, info, warn};
use redis::{PushInfo, PushKind, RedisError};
use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;
use thiserror::Error;
use tokio::sync::{mpsc::UnboundedSender, OnceCell};

pub mod gitlab;
mod redis_cli;

type ApprovalResult<T> = Result<T, ApprovalError>;

static REDIS_AGENT: OnceCell<Agent> = OnceCell::const_new();

// Initialize PubSub agent.
// Parameter 'tx' helps send the approval response.
pub fn init(endpoint: &str, tx: UnboundedSender<Approval>, me: User) -> ApprovalResult<()> {
    let client = Agent::new(endpoint, tx, Arc::new(me))?;

    REDIS_AGENT
        .set(client)
        .map_err(|err| format!("set redis client: {err}"))
        .expect("init pubsub agent failed");

    Ok(())
}

pub fn cli() -> &'static Agent {
    REDIS_AGENT.get().expect("pubsub agent not initialized")
}

pub trait PubSub {
    // Subscribe approval request from others.
    //
    // subscribe mr
    async fn sub_approval_request(&self) -> ApprovalResult<()>;

    // Get online users count.
    //
    // pubsub number mr
    async fn online_users(&self) -> ApprovalResult<u32>;

    // Subscribe to approval results from others.
    //
    // psubscribe mr.res.$uid*
    async fn sub_approval_response(&self, user: &User) -> ApprovalResult<()>;

    // Publish your approval request.
    //
    // publish mr approval
    async fn pub_approval_request(&self, approval: &Approval) -> ApprovalResult<()>;

    // Publish your approval response to the requester.
    //
    // publish mr.res.$uid approval
    async fn pub_approval_response(&self, approval: &Approval) -> ApprovalResult<()>;

    async fn acquire_semaphore_lock(&self, approval: &Approval) -> ApprovalResult<bool>;
}

pub struct Agent {
    client: redis_cli::Client,
}

impl PubSub for Agent {
    async fn sub_approval_request(&self) -> ApprovalResult<()> {
        self.client.subscribe("mr").await.map_err(Into::into)
    }

    async fn online_users(&self) -> ApprovalResult<u32> {
        self.client.pubsub_numsub("mr").await.map_err(Into::into)
    }

    async fn sub_approval_response(&self, user: &User) -> ApprovalResult<()> {
        self.client
            .psubscribe(format!("mr.res.{}*", user.id))
            .await
            .map_err(Into::into)
    }

    async fn pub_approval_request(&self, approval: &Approval) -> ApprovalResult<()> {
        let payload = serde_json::to_string(approval)?;

        self.client.publish("mr", payload).await.map_err(Into::into)
    }

    async fn pub_approval_response(&self, approval: &Approval) -> ApprovalResult<()> {
        let uid = &approval.requester.id;
        let payload = serde_json::to_string(approval)?;

        self.client
            .publish(format!("mr.res.{}", uid), payload)
            .await
            .map_err(Into::into)
    }

    async fn acquire_semaphore_lock(&self, approval: &Approval) -> ApprovalResult<bool> {
        let key = format!("mr.lock.{}.{}", &approval.pid, approval.mid);

        self.client.acquire_semaphore_lock(&key).await.map_err(Into::into)
    }
}

impl Agent {
    fn new(endpoint: &str, tx_approval: UnboundedSender<Approval>, me: Arc<User>) -> ApprovalResult<Self> {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let me = me.clone();

        // block reading approval request/response from others.
        tauri::async_runtime::spawn(async move {
            while let Some(payload) = rx.recv().await {
                match Approval::try_from(payload) {
                    // 1. ignore your own approval request
                    Ok(approval) if approval.is_requested_by_yourself(me.id) => {}

                    // 2. send approval response to tauri frontend(psub mr.res.$uid*)
                    Ok(approval) if approval.is_response() => {
                        if let Err(err) = tx_approval.send(approval) {
                            error!("failed to send approval to tauri frontend: {}", err);
                        }
                    }

                    // 3. process approval request
                    Ok(mut approval) => {
                        // 1. try to get semaphore lock
                        match cli().acquire_semaphore_lock(&approval).await {
                            Ok(true) => {}
                            Ok(false) => {
                                warn!("failed to acquire semaphore lock, good luck next time.");
                                continue;
                            }
                            Err(err) => {
                                error!("failed to acquire semaphore lock: {}", err);
                                continue;
                            }
                        };

                        // 2. approval request
                        if approval.approver.is_none() {
                            info!("{}", approval.request_details);
                            if let Err(err) = gitlab::cli().approve_merge_request(&approval).await {
                                error!("failed to approve merge request: {}", err);
                                approval.fail_reason = Some(err.to_string());
                            }

                            approval.approver = Some(me.clone().deref().to_owned());
                        }

                        // 3. publish approval response
                        if let Err(err) = cli().pub_approval_response(&approval).await {
                            error!("failed to publish approval response: {}", err);
                        }
                    }
                    Err(err) => {
                        println!("err or push type that could be ignored: {}", err);
                    }
                }
            }
        });

        let client = redis_cli::Client::new(endpoint, tx)?;

        Ok(Agent { client })
    }
}

#[derive(Debug, Error)]
pub enum ApprovalError {
    #[error("redis error: {0}")]
    Redis(#[from] RedisError),

    #[error("invalid push_info kind: {0}")]
    InvalidPushInfo(String),

    #[error("decode json error: {0}")]
    JsonDecode(#[from] serde_json::Error),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Approval {
    requester: User,
    approver: Option<User>,
    pid: String,
    mid: u64,
    request_details: String,
    fail_reason: Option<String>,
}

impl Approval {
    pub fn new_request(me: User, pid: String, mid: u64, mr_details: String) -> Self {
        Self {
            requester: me,
            approver: None,
            pid,
            mid,
            request_details: mr_details,
            fail_reason: None,
        }
    }

    pub fn show_response_details(&self, app: &AppHandle) {
        let approver = match &self.approver {
            Some(approver) => &approver.name,
            None => {
                error!("failed to get approver from {:?}", self);
                return;
            }
        };

        match &self.fail_reason {
            Some(reason) => {
                error!(
                    "[{}] approved {}/{} but failed: {}",
                    approver, self.pid, self.mid, reason
                );
            }
            None => {
                info!("[{}] approved {}/{}", self.requester.name, self.pid, self.mid);
            }
        }

        app.notification()
            .builder()
            .title(format!("{}{}", self.pid, self.mid))
            .body(format!("{} approved your request!", approver))
            .auto_cancel()
            .show()
            .unwrap();
    }

    fn is_requested_by_yourself(&self, uid: u64) -> bool {
        self.requester.id == uid && self.approver.is_none()
    }

    fn is_response(&self) -> bool {
        self.approver.is_some()
    }
}

impl TryFrom<PushInfo> for Approval {
    type Error = ApprovalError;

    fn try_from(value: PushInfo) -> Result<Self, Self::Error> {
        match value {
            PushInfo {
                kind: PushKind::Message | PushKind::PMessage,
                data,
            } => {
                let data = data
                    .into_iter()
                    .map(redis::from_owned_redis_value)
                    .collect::<Result<Vec<String>, RedisError>>()
                    .map_err(ApprovalError::Redis)?;

                match (value.kind, data.as_slice()) {
                    // approval request: subscribe mr
                    (PushKind::Message, [channel, payload]) if channel == "mr" => {
                        serde_json::from_str::<Self>(payload).map_err(ApprovalError::JsonDecode)
                    }

                    // approval response: psubscribe mr.res.$uid*
                    (PushKind::PMessage, [wildcard, pattern, payload])
                        if wildcard.starts_with("mr.res.") && pattern.starts_with("mr.res.") =>
                    {
                        serde_json::from_str::<Self>(payload).map_err(ApprovalError::JsonDecode)
                    }

                    invalid_data => {
                        error!("invalid push info: {:?}", invalid_data);
                        Err(ApprovalError::InvalidPushInfo(format!("{:?}", invalid_data)))
                    }
                }
            }
            _ => Err(ApprovalError::InvalidPushInfo(format!("{:?}", value))),
        }
    }
}
