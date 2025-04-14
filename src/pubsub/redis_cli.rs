use std::time::Duration;

use log::debug;
use redis::{
    aio::{ConnectionManager, ConnectionManagerConfig},
    AsyncCommands, PushInfo, RedisError, Script,
};
use tokio::sync::{mpsc::UnboundedSender, OnceCell};

type RedisResult<T> = Result<T, RedisError>;

pub struct Client {
    // manager: ConnectionManager,
    inner: redis::Client,
    once: OnceCell<ConnectionManager>,
    tx: UnboundedSender<PushInfo>,
}

impl Client {
    pub fn new(host: &str, port: u16, tx: UnboundedSender<PushInfo>) -> RedisResult<Client> {
        let client = redis::Client::open(format!("redis://{}:{}?protocol=resp3", host, port))?;
        _ = client.get_connection()?;

        Ok(Client {
            inner: client,
            once: OnceCell::const_new(),
            tx,
        })
    }

    pub async fn subscribe<T>(&self, channel: T) -> RedisResult<()>
    where
        T: AsRef<str>,
    {
        let mut conn = self.get_manager().await?;

        debug!("[redis][subscribe] {}", channel.as_ref());

        conn.subscribe(channel.as_ref()).await
    }

    pub async fn publish<T>(&self, channel: T, msg: String) -> RedisResult<()>
    where
        T: AsRef<str>,
    {
        let mut conn = self.get_manager().await?;

        debug!("[redis][publish] {}", channel.as_ref());

        conn.publish(channel.as_ref(), msg).await
    }

    pub async fn psubscribe<T>(&self, pattern: T) -> RedisResult<()>
    where
        T: AsRef<str>,
    {
        let mut conn = self.get_manager().await?;

        debug!("[redis][psubscribe] {}", pattern.as_ref());

        conn.psubscribe(pattern.as_ref()).await
    }

    pub async fn pubsub_numsub<T>(&self, channel: T) -> RedisResult<u32>
    where
        T: AsRef<str>,
    {
        let mut conn = self.get_manager().await?;

        debug!("[redis][pubsub numsub] {}", channel.as_ref());

        let (_channel, count): (String, u32) = redis::cmd("PUBSUB")
            .arg("NUMSUB")
            .arg(channel.as_ref())
            .query_async(&mut conn)
            .await?;

        Ok(count)
    }

    // the semaphore lock should be applied on each approval request.
    // mr.lock.$pid.$mid
    async fn acquire_semaphore_lock(&self, key: &str) -> RedisResult<bool> {
        let mut conn = self.get_manager().await?;

        debug!("[redis][semaphore script] {}", key);

        let script = Script::new(SEMAPHORE_SCRIPT);
        let expire = 10;
        let concurrency = 3;
        let result: i32 = script
            .key(key)
            .arg(expire)
            .arg(concurrency)
            .invoke_async(&mut conn)
            .await?;

        Ok(result == 1)
    }

    pub async fn get_manager(&self) -> RedisResult<ConnectionManager> {
        let manager = self
            .once
            .get_or_try_init(|| {
                let config = ConnectionManagerConfig::new()
                    .set_connection_timeout(Duration::from_secs(30))
                    .set_response_timeout(Duration::from_secs(30))
                    .set_push_sender(self.tx.clone())
                    .set_automatic_resubscription();

                self.inner.get_connection_manager_with_config(config)
            })
            .await?
            .clone();

        Ok(manager)
    }
}

const SEMAPHORE_SCRIPT: &str = r#"
    local current = redis.call('get', KEYS[1])
    if not current then
        redis.call('set', KEYS[1], 1, 'EX', ARGV[1])
        return 1
    elseif tonumber(current) < tonumber(ARGV[2]) then
        redis.call('incr', KEYS[1])
        redis.call('expire', KEYS[1], ARGV[1])
        return 1
    else
        return 0
    end
"#;

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::pubsub::redis_cli::Client;

    #[tokio::test]
    async fn test_semaphone_lock() {
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let cli = Client::new("127.0.0.1", 6379, tx).unwrap();
        let key = "mr.lock.p1.m1";

        for i in 0..4 {
            let acquired = cli.acquire_semaphore_lock(key).await.unwrap();
            let expected = i != 3;
            assert_eq!(expected, acquired);
        }
        tokio::time::sleep(Duration::from_secs(10)).await;
        assert!(cli.acquire_semaphore_lock(key).await.unwrap());
    }
}
