use std::time::Duration;

use redis::{aio::ConnectionManager, AsyncCommands};

use crate::{
    domain::{cache, CacheEntry},
    error::AppResult,
};

#[derive(Clone)]
pub struct RedisCache {
    connection: ConnectionManager,
}

impl RedisCache {
    pub async fn connect(redis_url: &str) -> AppResult<Self> {
        let client = redis::Client::open(redis_url)?;
        let mut connection = ConnectionManager::new(client).await?;
        let _: String = redis::cmd("PING").query_async(&mut connection).await?;

        Ok(Self { connection })
    }

    pub async fn get(&self, key: &str) -> AppResult<Option<CacheEntry>> {
        let mut connection = self.connection.clone();
        let data: Option<Vec<u8>> = connection.get(key).await?;

        data.map(|bytes| cache::decode_entry(&bytes).map_err(Into::into))
            .transpose()
    }

    pub async fn set(&self, key: &str, payload: &[u8], ttl: Duration) -> AppResult<()> {
        let mut connection = self.connection.clone();
        let data = cache::encode_entry(payload)?;
        let seconds = ttl.as_secs().max(1);
        let _: () = connection.set_ex(key, data, seconds).await?;

        Ok(())
    }
}
