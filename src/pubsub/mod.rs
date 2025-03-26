use log::{error, info};
use redis::RedisResult;
use serde::{Deserialize, Serialize};
use tokio::sync::OnceCell;

mod redis_cli;

pub async fn foo() {
    info!(r#"PUBLISH mr.req {{uid:"A",mid:"group_1/project_1",pid:"1102"}}"#);
}

pub async fn on_sub_result() -> String {
    info!("get result from PSUBSCRIBE mr.res.userA");

    "userB approved group1/project_1/1102".into()
}

pub trait PubSub {
    // Subscribe approval request from others
    async fn sub_approval_request(&self);

    // Get online user list
    async fn online_users(&self);

    // Publish your approval request
    async fn pub_approval_request(&self);

    // Subscribe the result of your approval request
    async fn sub_approval_result(&self);
}

// > pubsub.init(host, port, callback)
// > pubsub.cli().online_users()...

// The pubsub agent.
struct Agent {
    client: redis_cli::Client2,
}

impl Agent {
    pub async fn new<F>(host: &str, port: u16, callback: F) -> Self
    where
        F: Fn(String) + Send + 'static,
    {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

        tokio::spawn(async move {
            while let Some(payload) = rx.recv().await {
                let payload = "payload".to_string();
                callback(payload);
            }
        });

        // let redis_client = redis_cli::Client::new(host, port, tx)
        //     .inspect_err(|err| error!("init redis client: {}", err))
        //     .expect("init pubsub agent failed");

        let redis_client = redis_cli::Client2::new(host, port, tx).await.unwrap();

        Agent { client: redis_client }
    }
}

static CLIENT: OnceCell<Agent> = OnceCell::const_new();

pub fn init<F>(host: &str, port: u16, callback: F) -> RedisResult<()>
where
    F: Fn(String) + Send + 'static,
{
    // while let Some(msg) = rx.recv().await {
    //    callback(msg);
    // }

    todo!()
}

pub fn cli() -> &'static Agent {
    CLIENT.get().expect("pubsub agent not initialized")
}

// static CLIENT: OnceCell<redis_cli::Client> = OnceCell::const_new();

// pub fn init(host: &str, port: u16, f: Fn) -> RedisResult<()> {
//     CLIENT
//         .set(redis_cli::Client::new(host, port)?)
//         .map_err(|e| format!("set redis client: {e}"))
//         .expect("init redis failed");

//     Ok(())
// }

// pub fn cli() -> &'static redis_cli::Client {
//     CLIENT.get().expect("redis client not initialized")
// }

#[derive(Debug, Serialize, Deserialize)]
struct User {
    name: String,
    uid: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct Approval {
    requester: User,
    approver: Option<User>,
    pid: String,
    mid: String,
}

// ===== Resis =====

// Events
// mr - merge request, every user should subscribe at initialization.

// Patterns
// mr.res.$uid.* - your merge request approval result, subscribe it at initialization.

// Tips
// > http://doc.redisfans.com/pub_sub/index.html
// > the distributed lock for limiting concurrent requests.

// * Initialization of a user
// 1. Subscribe mr
//      > to get approval request from others
//      > to show online member count
// 2. PSubscribe mr.res.$uid
//      > to get the details of an approval request

// Action
// User A
// 1. if online users > 0, Publish mr.req {uid:"A",mid:"group_1/project_1",pid:"1102"}
// 2.

// User B
// 1. Read message {uid:"A",pid:"group_1/project_1",mid:"1102"} from `mr`
// 2. Approve request, Publish mr.res.A.B - {uid:"A",mid:"group_1/project_1",pid:"1102",approver:"B"}

#[cfg(test)]
mod tests {}
