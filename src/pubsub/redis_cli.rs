use std::time::Duration;

use redis::aio::{ConnectionManager, ConnectionManagerConfig};
use tokio::sync::OnceCell;

use super::PubSub;

type RedisResult<T> = Result<T, redis::RedisError>;

pub struct Client {
    inner: redis::Client,
    once: OnceCell<ConnectionManager>,
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
        let inner = redis::Client::open(format!("redis://{}:{}", host, port))?;
        let _ = inner.get_connection()?; // connectivity test

        Ok(Client {
            inner,
            once: OnceCell::const_new(),
        })
    }

    async fn subscribe(&self, topic: impl AsRef<str>) {}

    async fn get_async_connection(&self) -> RedisResult<redis::aio::ConnectionManager> {
        Ok(self
            .once
            .get_or_try_init(|| {
                self.inner.get_connection_manager_with_config(
                    ConnectionManagerConfig::new()
                        .set_connection_timeout(Duration::from_secs(30))
                        .set_response_timeout(Duration::from_secs(30)),
                )
            })
            .await?
            .clone())
    }
}

#[cfg(test)]
mod tests {
    use redis::AsyncCommands;

    use super::Client;

    #[tokio::test]
    async fn test_connectivity() {
        let client = Client::new("127.0.0.1", 6379).unwrap();
        let mut conn = client.get_async_connection().await.unwrap();
        let _: () = conn.set_ex("foo", "bar", 1).await.unwrap();
        let bar: String = conn.get("foo").await.unwrap();
        assert_eq!(bar, "bar");
    }
}
