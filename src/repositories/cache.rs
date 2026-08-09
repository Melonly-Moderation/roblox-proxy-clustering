use std::time::Duration;

use crate::{domain::CacheEntry, error::AppResult, infrastructure::RedisCache};

#[derive(Clone)]
pub struct CacheRepository {
    cache: RedisCache,
}

impl CacheRepository {
    pub fn new(cache: RedisCache) -> Self {
        Self { cache }
    }

    pub async fn ping(&self) -> AppResult<()> {
        self.cache.ping().await
    }

    pub async fn get(&self, key: &str) -> AppResult<Option<CacheEntry>> {
        self.cache.get(key).await
    }

    pub async fn set(&self, key: &str, payload: &[u8], ttl: Duration) -> AppResult<()> {
        self.cache.set(key, payload, ttl).await
    }
}
