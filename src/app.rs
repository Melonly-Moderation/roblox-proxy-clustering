use std::{
    sync::{atomic::AtomicUsize, Arc},
    time::Duration,
};

use axum::Router;
use dashmap::DashMap;
use tokio::{net::TcpListener, task::JoinHandle};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use crate::{
    config::Settings,
    domain::{upstream, MemberTarget, ProviderTarget},
    error::AppError,
    http,
    infrastructure::{DiscordWebhook, ProxyHttpClient, RedisCache},
    repositories::{CacheRepository, RobloxRepository},
};

#[derive(Clone)]
pub struct AppState {
    pub settings: Arc<Settings>,
    pub cache: CacheRepository,
    pub http: ProxyHttpClient,
    pub webhook: DiscordWebhook,
    pub roblox: RobloxRepository,
    pub member_targets: Arc<Vec<MemberTarget>>,
    pub provider_targets: Arc<Vec<ProviderTarget>>,
    pub provider_cursor: Arc<AtomicUsize>,
    pub cache_locks: Arc<DashMap<String, Arc<tokio::sync::Mutex<()>>>>,
}

pub async fn run() -> Result<(), AppError> {
    dotenvy::dotenv().ok();
    init_tracing();

    let settings = Arc::new(Settings::from_env()?);
    let state = build_state(Arc::clone(&settings)).await?;

    state
        .webhook
        .spawn_send("Proxy service started successfully".to_owned());

    let shutdown = CancellationToken::new();
    let app = http::router(state.clone());
    let result = serve(app, settings.listen_addr.clone(), shutdown.clone()).await;

    shutdown.cancel();
    wait_for_background_tasks(Vec::new(), Duration::from_secs(5)).await;

    result
}

async fn build_state(settings: Arc<Settings>) -> Result<AppState, AppError> {
    let redis = RedisCache::connect(&settings.redis_url).await?;
    let cache = CacheRepository::new(redis);
    let http = ProxyHttpClient::new(&settings)?;
    let webhook = DiscordWebhook::new(settings.discord_webhook_url.clone());
    let roblox = RobloxRepository::new(http.clone(), webhook.clone());
    let member_targets =
        upstream::parse_member_targets(&settings.member_clusters).unwrap_or_default();
    let provider_targets =
        upstream::parse_provider_targets(&settings.provider_clusters).unwrap_or_default();

    Ok(AppState {
        settings,
        cache,
        http,
        webhook,
        roblox,
        member_targets: Arc::new(member_targets),
        provider_targets: Arc::new(provider_targets),
        provider_cursor: Arc::new(AtomicUsize::new(0)),
        cache_locks: Arc::new(DashMap::new()),
    })
}

async fn serve(
    app: Router,
    listen_addr: String,
    shutdown: CancellationToken,
) -> Result<(), AppError> {
    let listener = TcpListener::bind(&listen_addr).await?;
    info!(%listen_addr, "roblox proxy cluster listening");

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal(shutdown))
    .await?;

    Ok(())
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("roblox_proxy_clustering=info,tower_http=info"));

    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().compact())
        .try_init();
}

async fn shutdown_signal(shutdown: CancellationToken) {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    info!("received shutdown signal");
    shutdown.cancel();
}

async fn wait_for_background_tasks(handles: Vec<JoinHandle<()>>, timeout: Duration) {
    let joined = async {
        for handle in handles {
            if let Err(error) = handle.await {
                warn!(%error, "background task join failed");
            }
        }
    };

    if tokio::time::timeout(timeout, joined).await.is_err() {
        warn!(?timeout, "background task shutdown timed out");
    }
}
