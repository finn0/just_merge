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

pub struct RedisAgent {}

impl RedisAgent {
    async fn init(&self) {
        self.sub_approval_request().await;
        self.sub_approval_result().await;
    }
}

impl PubSub for RedisAgent {
    async fn sub_approval_request(&self) {
        // SUBSCRIBE mr
        todo!()
    }

    async fn online_users(&self) {
        // PUBSUB NUMSUM mr
        todo!()
    }

    async fn pub_approval_request(&self) {
        // PUBLISH mr $uid:$pid:mid
        todo!()
    }

    async fn sub_approval_result(&self) {
        // PSUBSCRIBE mr.res.$uid.*
        todo!()
    }
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
