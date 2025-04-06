use log::error;
use redis::{PushInfo, PushKind, RedisError};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::{mpsc::UnboundedSender, OnceCell};

mod redis_cli;

type ApprovalResult<T> = Result<T, ApprovalError>;

static AGENT: OnceCell<Agent> = OnceCell::const_new();

// Initialize PubSub agent.
// Parameter `tx` helps send the approval response.
pub fn init(host: &str, port: u16, tx: UnboundedSender<Approval>, me: &User) -> ApprovalResult<()> {
    let client = Agent::new(host, port, tx, me)?;

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
    // publish mr.res.$uid approval
    async fn pub_approval_response(&self, approval: &Approval) -> ApprovalResult<()>;
}

pub struct Agent {
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
    fn new(host: &str, port: u16, tx_approval: UnboundedSender<Approval>, me: &User) -> ApprovalResult<Agent> {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let uid = me.id.clone();

        tauri::async_runtime::spawn(async move {
            while let Some(payload) = rx.recv().await {
                match Approval::try_from(payload) {
                    Ok(approval) => {
                        // todo: requester is me, approver = None, ignore.
                        // todo: requester is me, approver is others, send back to tauri.
                        if approval.requester.id == uid && approval.approver.is_none() {
                            continue;
                        }
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

        let client = redis_cli::Client::new(host, port, tx)?;

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub name: String,
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

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::pubsub::{cli, init, Agent, Approval, PubSub, User};

    #[test]
    fn test_serde() {
        let me = User {
            id: "user001".to_string(),
            name: "foo".to_string(),
        };
        let other = User {
            id: "user002".to_string(),
            name: "bar".to_string(),
        };
        let approval_response = Approval {
            requester: me,
            approver: Some(other),
            pid: "project/001".to_string(),
            mid: "merge/001".to_string(),
        };
        let x = serde_json::to_string(&approval_response).unwrap();
        println!("{}", x);
    }

    #[tokio::test]
    async fn test_all() {
        let (tx_approval, mut rx_approval) = tokio::sync::mpsc::unbounded_channel();
        tokio::spawn(async move {
            while let Some(approval) = rx_approval.recv().await {
                println!("<< recv approval: {:?}", approval);
            }
        });

        // init
        let me = User {
            id: "001".to_string(),
            name: "me".to_string(),
        };

        init("127.0.0.1", 6379, tx_approval, &me).unwrap();

        // init - subscriptions
        cli().sub_approval_request().await.unwrap();
        cli().sub_approval_response(&me).await.unwrap();
        let online_users = cli().oneline_users().await.unwrap();
        assert_eq!(1, online_users);

        // publish approval request
        let approval_request = Approval {
            requester: me.clone(),
            approver: None,
            pid: "project/001".to_string(),
            mid: "merge/001".to_string(),
        };
        cli().pub_approval_request(&approval_request).await.unwrap();

        // foo publish approval response
        let foo = User {
            id: "002".to_string(),
            name: "foo".to_string(),
        };
        let approval_response_of_foo = Approval {
            requester: me,
            approver: Some(foo.clone()),
            pid: "project/001".to_string(),
            mid: "merge/001".to_string(),
        };
        let (tx_approval, mut _rx_approval) = tokio::sync::mpsc::unbounded_channel();
        tokio::spawn(async move {
            while let Some(approval) = _rx_approval.recv().await {
                println!("<< foo recv approval: {:?}", approval);
            }
        });
        let agent_foo = Agent::new("127.0.0.1", 6379, tx_approval, &foo).unwrap();
        agent_foo
            .pub_approval_response(&approval_response_of_foo)
            .await
            .unwrap();

        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}
