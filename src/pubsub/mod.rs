use log::error;
use redis::{PushInfo, PushKind, RedisError};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::{mpsc::UnboundedSender, OnceCell};

mod redis_cli;

type ApprovalResult<T> = Result<T, ApprovalError>;

static AGENT: OnceCell<Agent> = OnceCell::const_new();

pub async fn init(host: &str, port: u16, tx: UnboundedSender<Approval>) -> ApprovalResult<()> {
    let client = Agent::new(host, port, tx).await?;

    AGENT
        .set(client)
        .map_err(|err| format!("set redis client: {err}"))
        .expect("init pubsub agent failed");

    Ok(())
}

pub fn cli() -> &'static Agent {
    AGENT.get().expect("pubsub agent not initialized")
}

pub trait PubSub {
    // Subscribe approval request from others.
    //
    // subscribe mr
    async fn sub_approval_request(&self) -> ApprovalResult<()>;

    // Get online users count.
    //
    // pubsub numsub mr
    async fn oneline_users(&self) -> ApprovalResult<u32>;

    // Subscribe to approval results from others.
    //
    // psubscribe mr.res.$uid*
    async fn sub_approval_response(&self, user: &User) -> ApprovalResult<()>;

    // Publish your approval request.
    //
    // publish mr approval
    async fn pub_approval_request(&self, approval: &Approval) -> ApprovalResult<()>;

    // Publish your approval response for others.
    //
    // publish mr approval
    async fn pub_approval_response(&self, approval: &Approval) -> ApprovalResult<()>;
}

struct Agent {
    client: redis_cli::Client,
}

impl PubSub for Agent {
    async fn sub_approval_request(&self) -> ApprovalResult<()> {
        self.client.subscribe("mr").await.map_err(ApprovalError::Redis)
    }

    async fn oneline_users(&self) -> ApprovalResult<u32> {
        self.client.pubsub_numsub("mr").await.map_err(ApprovalError::Redis)
    }

    async fn sub_approval_response(&self, user: &User) -> ApprovalResult<()> {
        self.client
            .psubscribe(format!("mr.res.{}*", user.id))
            .await
            .map_err(ApprovalError::Redis)
    }

    async fn pub_approval_request(&self, approval: &Approval) -> ApprovalResult<()> {
        let payload = serde_json::to_string(approval)?;

        self.client.publish("mr", payload).await.map_err(ApprovalError::Redis)
    }

    async fn pub_approval_response(&self, approval: &Approval) -> ApprovalResult<()> {
        let uid = &approval.requester.id;
        let payload = serde_json::to_string(approval)?;

        self.client
            .publish(format!("mr.res.{}", uid), payload)
            .await
            .map_err(ApprovalError::Redis)
    }
}

impl Agent {
    async fn new(host: &str, port: u16, tx_approval: UnboundedSender<Approval>) -> ApprovalResult<Agent> {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

        tokio::spawn(async move {
            while let Some(payload) = rx.recv().await {
                match Approval::try_from(payload) {
                    Ok(approval) => {
                        if let Err(err) = tx_approval.send(approval) {
                            error!("failed to send approval: {}", err);
                        }
                    }
                    Err(err) => {
                        println!("err or push type that could be ignored: {}", err);
                    }
                }
            }
        });

        let client = redis_cli::Client::new(host, port, tx).await?;

        Ok(Agent { client })
    }
}

// ==== Errors and Data Structures ====

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
pub struct User {
    id: String,
    name: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Approval {
    requester: User,
    approver: Option<User>,
    pid: String,
    mid: String,
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
                    // subscribe mr
                    (PushKind::Message, [channel, payload]) if channel == "mr" => {
                        serde_json::from_str::<Self>(payload).map_err(ApprovalError::JsonDecode)
                    }

                    // psubscribe mr.res.$uid*
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
