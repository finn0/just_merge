use std::time::Duration;

use crossbeam::channel::{bounded, Receiver, Sender};
use log::{info, warn};
use redis::{
    aio::{ConnectionManager, ConnectionManagerConfig},
    AsyncCommands, PushInfo,
};
use tokio::sync::{mpsc::UnboundedReceiver, OnceCell};

use super::PubSub;

type RedisResult<T> = Result<T, redis::RedisError>;

pub struct Client {
    inner: redis::Client,
    once: OnceCell<ConnectionManager>,
    // tx: UnboundedReceiver<>
    // rx: UnboundedReceiver<PushInfo>,
}

impl PubSub for Client {
    // SUBSCRIBE mr
    async fn sub_approval_request(&self) {
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
        let x = self.get_async_connection().await.unwrap();

        self.subscribe("ha").await;

        todo!()
    }
}

impl Client {
    pub fn new(host: &str, port: u16) -> RedisResult<Client> {
        let inner = redis::Client::open(format!("redis://{}:{}?protocol=resp3", host, port))?;
        let _ = inner.get_connection()?; // connectivity test

        Ok(Client {
            inner,
            once: OnceCell::const_new(),
        })
    }

    // // Error: Cannot set resubscribe_automatically without setting a push sender to receive messages.- ClientError
    // pub async fn new2(host: &str, port: u16) -> RedisResult<Client> {
    //     let inner = redis::Client::open(format!("redis://{}:{}", host, port))?;
    //     let mut conn = inner.get_connection()?; // connectivity test

    //     tokio::spawn(async move {
    //         let mut ps = conn.as_pubsub();
    //         let msg_result = ps.get_message();
    //         match msg_result {
    //             Ok(msg) => {
    //                 let channel = msg.get_channel_name();
    //                 let payload: String = msg.get_payload().unwrap();
    //                 info!("recv from {}: {}", channel, payload);
    //             }
    //             Err(err) => warn!("recv from channel: {}", err),
    //         }
    //     });

    //     Ok(Client {
    //         inner,
    //         once: OnceCell::const_new(),
    //     })
    // }

    // pub async fn new3(host: &str, port: u16) -> RedisResult<Client> {
    //     let inner = redis::Client::open(format!("redis://{}:{}", host, port))?;
    //     let mut conn = inner.get_connection()?; // connectivity test

    //     let (sink, stream) = inner.get_async_pubsub().await?.split();

    //     // let (tx, rx) = bounded(10);
    //     // let mut ps = conn.as_pubsub();
    //     // ps.
    //     todo!()
    // }

    // pub async fn new4(host: &str, port: u16) -> RedisResult<Client> {
    //     let inner = redis::Client::open(format!("redis://{}:{}", host, port))?;
    //     let _ = inner.get_connection()?; // connectivity test

    //     let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    //     let once = OnceCell::const_new();
    //     once.get_or_try_init(|| {
    //         let config = ConnectionManagerConfig::new()
    //             .set_connection_timeout(Duration::from_secs(30))
    //             .set_response_timeout(Duration::from_secs(30))
    //             .set_push_sender(tx)
    //             .set_automatic_resubscription(); // auto re-subscribe after disconnection

    //         inner.get_connection_manager_with_config(config)
    //     })
    //     .await?
    //     .clone();

    //     let client = Client { inner, once, rx };

    //     // tokio::spawn(async {
    //     //     while let Some(payload) =
    //     // });

    //     Ok(client)
    // }

    // AI guides me the way of choosing the type of your parameter.
    // | Consideration                | Choose `&str`  | Choose `impl AsRef<str>`          |
    // |------------------------------|----------------|------------------------------|
    // | Maximum flexibility needed   | ❌             | ✅                            |
    // | Performance is critical      | ✅             | ❌ (minor overhead)           |
    // | Only needs `&str` internally | ✅             | ✅ (requires `.as_ref()`)     |
    // | Public API for libraries     | ❌             | ✅                            |
    // | Simple internal functions    | ✅             | ❌                            |
    async fn subscribe(&self, topic: &str) -> RedisResult<()> {
        let mut conn = self.get_async_connection2().await?;

        conn.subscribe(topic).await
    }

    // RESP3
    // https://docs.rs/redis/latest/redis/#resp3-async-pubsub
    async fn publish(&self, topic: &str, msg: String) -> RedisResult<()> {
        let mut conn = self.get_async_connection2().await?;

        conn.publish(topic, msg).await
    }

    async fn get_async_connection(&self) -> RedisResult<redis::aio::ConnectionManager> {
        Ok(self
            .once
            .get_or_try_init(|| {
                self.inner.get_connection_manager_with_config(
                    ConnectionManagerConfig::new()
                        .set_connection_timeout(Duration::from_secs(30))
                        .set_response_timeout(Duration::from_secs(30))
                        .set_automatic_resubscription(), // auto re-subscribe after disconnection
                )
            })
            .await?
            .clone())
    }

    async fn get_async_connection2(&self) -> RedisResult<redis::aio::ConnectionManager> {
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
