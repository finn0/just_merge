use log::info;
use redis::RedisResult;
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

static CLIENT: OnceCell<redis_cli::Client> = OnceCell::const_new();

pub fn init(host: &str, port: u16) -> RedisResult<()> {
    CLIENT
        .set(redis_cli::Client::new(host, port)?)
        .map_err(|e| format!("set redis client: {e}"))
        .expect("init redis failed");

    Ok(())
}

pub fn cli() -> &'static redis_cli::Client {
    CLIENT.get().expect("redis client not initialized")
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
