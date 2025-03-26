use std::time::Duration;

use log::{debug, info};
use redis::{
    aio::{ConnectionManager, ConnectionManagerConfig},
    AsyncCommands, PushInfo,
};
use tokio::sync::{mpsc::UnboundedSender, OnceCell};

use super::PubSub;

pub type RedisResult<T> = Result<T, redis::RedisError>;

pub struct Client2 {
    inner: redis::Client,
    manager: ConnectionManager,
}
impl Client2 {
    pub async fn new(host: &str, port: u16, tx: UnboundedSender<PushInfo>) -> RedisResult<Client2> {
        let inner = redis::Client::open(format!("redis://{}:{}?protocol=resp3", host, port))?;
        let _ = inner.get_connection()?; // connectivity test

        let config = ConnectionManagerConfig::new()
            .set_connection_timeout(Duration::from_secs(30))
            .set_response_timeout(Duration::from_secs(30))
            .set_push_sender(tx)
            .set_automatic_resubscription(); // auto re-subscribe after disconnection

        let manager = inner.get_connection_manager_with_config(config).await?;

        Ok(Client2 { inner, manager })
    }
    async fn subscribe(&self, channel: &str) -> RedisResult<()> {
        let mut conn = self.manager.clone();

        debug!("subscribe channel={}", channel);

        conn.subscribe(channel).await
    }
}

pub struct Client {
    inner: redis::Client,
    once: OnceCell<ConnectionManager>,
    tx: UnboundedSender<PushInfo>,
}

impl PubSub for Client {
    // SUBSCRIBE mr
    async fn sub_approval_request(&self) {
        todo!()
    }

    async fn online_users(&self) {
        // PUBSUB NUMSUB mr
        todo!()
    }

    async fn pub_approval_request(&self) {
        // PUBLISH mr $uid:$pid:mid
        todo!()
    }

    async fn sub_approval_result(&self) {
        // PSUBSCRIBE mr.res.$uid*
        todo!()
    }
}

impl Client {
    pub fn new(host: &str, port: u16, tx: UnboundedSender<PushInfo>) -> RedisResult<Client> {
        let inner = redis::Client::open(format!("redis://{}:{}?protocol=resp3", host, port))?;
        let _ = inner.get_connection()?; // connectivity test

        Ok(Client {
            inner,
            once: OnceCell::const_new(),
            tx,
        })
    }

    // AI guides me the way of choosing the type of your parameter.
    // | Consideration                | Choose `&str`  | Choose `impl AsRef<str>`     |
    // |------------------------------|----------------|------------------------------|
    // | Maximum flexibility needed   | ❌             | ✅                            |
    // | Performance is critical      | ✅             | ❌ (minor overhead)           |
    // | Only needs `&str` internally | ✅             | ✅ (requires `.as_ref()`)     |
    // | Public API for libraries     | ❌             | ✅                            |
    // | Simple internal functions    | ✅             | ❌                            |

    async fn subscribe(&self, channel: &str) -> RedisResult<()> {
        let mut conn = self.get_async_connection().await?;

        debug!("subscribe channel={}", channel);

        conn.subscribe(channel).await
    }

    async fn publish(&self, channel: &str, msg: String) -> RedisResult<()> {
        let mut conn = self.get_async_connection().await?;

        debug!("publish channel={}, msg={}", channel, msg);

        conn.publish(channel, msg).await
    }

    async fn get_async_connection(&self) -> RedisResult<redis::aio::ConnectionManager> {
        Ok(self
            .once
            .get_or_try_init(|| {
                let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

                tokio::spawn(async move {
                    while let Some(payload) = rx.recv().await {
                        info!("recv: {:?}", payload);
                    }
                });

                let config = ConnectionManagerConfig::new()
                    .set_connection_timeout(Duration::from_secs(30))
                    .set_response_timeout(Duration::from_secs(30))
                    .set_push_sender(tx)
                    .set_automatic_resubscription(); // auto re-subscribe after disconnection

                self.inner.get_connection_manager_with_config(config)
            })
            .await?
            .clone())
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use redis::{AsyncCommands, RedisResult};

    use crate::pubsub::cli;

    use super::Client;

    // #[tokio::test]
    // async fn test_connectivity() {
    //     let client = Client::new("127.0.0.1", 6379).unwrap();
    //     let mut conn = client.get_async_connection().await.unwrap();
    //     let _: () = conn.set_ex("foo", "bar", 1).await.unwrap();
    //     let bar: String = conn.get("foo").await.unwrap();
    //     assert_eq!(bar, "bar");
    // }

    #[tokio::test]
    async fn test_pubsub() -> RedisResult<()> {
        let client = Client::new("192.168.50.183", 6379)?;

        let topic = "foo";

        client.subscribe(topic).await?;

        tokio::time::sleep(Duration::from_secs(1)).await;

        client.publish(topic, "msg 1".to_string()).await?;

        tokio::time::sleep(Duration::from_secs(1)).await;

        client.publish(topic, "msg 2".to_string()).await?;

        tokio::time::sleep(Duration::from_secs(1)).await;

        Ok(())
    }
}
