use std::time::Duration;

use log::debug;
use redis::{
    aio::{ConnectionManager, ConnectionManagerConfig},
    AsyncCommands, PushInfo, RedisError,
};
use tokio::sync::mpsc::UnboundedSender;

type RedisResult<T> = Result<T, RedisError>;

pub struct Client {
    manager: ConnectionManager,
}

impl Client {
    pub async fn new(host: &str, port: u16, tx: UnboundedSender<PushInfo>) -> RedisResult<Client> {
        let client = redis::Client::open(format!("redis://{}:{}?protocol=resp3", host, port))?;
        _ = client.get_connection()?;

        let config = ConnectionManagerConfig::new()
            .set_connection_timeout(Duration::from_secs(30))
            .set_response_timeout(Duration::from_secs(30))
            .set_push_sender(tx)
            .set_automatic_resubscription();

        let manager = client.get_connection_manager_with_config(config).await?;

        Ok(Client { manager })
    }

    pub async fn subscribe<T>(&self, channel: T) -> RedisResult<()>
    where
        T: AsRef<str>,
    {
        let mut conn = self.manager.clone();

        debug!("[redis][subscribe] {}", channel.as_ref());

        conn.subscribe(channel.as_ref()).await
    }

    pub async fn publish<T>(&self, channel: T, msg: String) -> RedisResult<()>
    where
        T: AsRef<str>,
    {
        let mut conn = self.manager.clone();

        debug!("[redis][publish] {}", channel.as_ref());

        conn.publish(channel.as_ref(), msg).await
    }

    pub async fn psubscribe<T>(&self, pattern: T) -> RedisResult<()>
    where
        T: AsRef<str>,
    {
        let mut conn = self.manager.clone();

        debug!("[redis][psubscribe] {}", pattern.as_ref());

        conn.psubscribe(pattern.as_ref()).await
    }

    pub async fn pubsub_numsub<T>(&self, channel: T) -> RedisResult<u32>
    where
        T: AsRef<str>,
    {
        let mut conn = self.manager.clone();

        debug!("[redis][pubsub numsub] {}", channel.as_ref());

        let (_channel, count): (String, u32) = redis::cmd("PUBSUB")
            .arg("NUMSUB")
            .arg(channel.as_ref())
            .query_async(&mut conn)
            .await?;

        Ok(count)
    }
}
