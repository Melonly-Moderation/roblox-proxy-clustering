use std::{future::Future, pin::Pin, sync::Arc};

use tokio::sync::Mutex;
use tracing::{debug, warn};

use crate::{app::AppState, error::AppResult};

pub type CacheFetcher = Arc<
    dyn Fn() -> Pin<Box<dyn Future<Output = AppResult<Vec<u8>>> + Send>> + Send + Sync + 'static,
>;

pub async fn read_through(
    state: &AppState,
    key: String,
    fetcher: CacheFetcher,
) -> AppResult<Vec<u8>> {
    if let Some(entry) = state.cache.get(&key).await? {
        if entry.stored_at + state.settings.background_refresh_after < chrono::Utc::now() {
            launch_refresh(state.clone(), key.clone(), fetcher.clone());
        }

        return Ok(entry.payload);
    }

    let lock = lock_for(state, &key);
    let _guard = lock.lock().await;

    if let Some(entry) = state.cache.get(&key).await? {
        return Ok(entry.payload);
    }

    let payload = fetcher().await?;
    store_payload(state, &key, &payload).await;
    Ok(payload)
}

fn launch_refresh(state: AppState, key: String, fetcher: CacheFetcher) {
    tokio::spawn(async move {
        let lock_key = format!("{key}:refresh");
        let lock = lock_for(&state, &lock_key);
        let _guard = lock.lock().await;

        match fetcher().await {
            Ok(payload) => store_payload(&state, &key, &payload).await,
            Err(error) => debug!(%key, %error, "background cache refresh failed"),
        }
    });
}

async fn store_payload(state: &AppState, key: &str, payload: &[u8]) {
    if let Err(error) = state
        .cache
        .set(key, payload, state.settings.cache_ttl)
        .await
    {
        warn!(%key, %error, "cache store failed");
    }
}

fn lock_for(state: &AppState, key: &str) -> Arc<Mutex<()>> {
    state
        .cache_locks
        .entry(key.to_owned())
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone()
}
