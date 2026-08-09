use std::net::SocketAddr;

use axum::{
    body::Body,
    extract::{ConnectInfo, State},
    http::{Request, Response, StatusCode},
    response::IntoResponse,
    routing::{any, get},
    Router,
};
use serde_json::json;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing::warn;

use crate::{
    app::AppState,
    domain::Role,
    services::{member, provider, response},
};

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(root))
        .route("/health", get(health))
        .route("/healthz", get(health))
        .fallback(any(dispatch))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn root(State(state): State<AppState>) -> Response<Body> {
    json_status(
        StatusCode::OK,
        json!({
            "service": "roblox-proxy-clustering",
            "status": "ok",
            "role": state.settings.role,
        }),
        &state,
    )
}

async fn health(State(state): State<AppState>) -> Response<Body> {
    let checks = async { tokio::join!(state.cache.ping(), state.roblox.ping()) };
    let healthy = match tokio::time::timeout(state.settings.request_timeout, checks).await {
        Ok((redis, roblox)) => {
            if let Err(error) = &redis {
                warn!(%error, "health check failed for Redis");
            }
            if let Err(error) = &roblox {
                warn!(%error, "health check failed for Roblox");
            }

            redis.is_ok() && roblox.is_ok()
        }
        Err(error) => {
            warn!(%error, "health check timed out");
            false
        }
    };

    (
        health_status(healthy),
        if healthy { "ok" } else { "unhealthy" },
    )
        .into_response()
}

fn health_status(healthy: bool) -> StatusCode {
    if healthy {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    }
}

async fn dispatch(
    State(state): State<AppState>,
    ConnectInfo(remote_addr): ConnectInfo<SocketAddr>,
    request: Request<Body>,
) -> Response<Body> {
    let role = state.settings.role;
    let result = match role {
        Role::Member => member::handle(state, remote_addr, request).await,
        Role::Provider => provider::handle(state, remote_addr, request).await,
    };

    match result {
        Ok(response) => response,
        Err(error) => {
            let mut response = error.into_response();
            response.headers_mut().insert(
                "x-proxy-role",
                role.as_str().parse().expect("static role header"),
            );
            response
        }
    }
}

fn json_status(status: StatusCode, payload: serde_json::Value, state: &AppState) -> Response<Body> {
    let payload = serde_json::to_vec(&payload)
        .unwrap_or_else(|_| b"{\"error\":\"internal server error\"}".to_vec());

    response::json_bytes(status, payload, state.settings.role, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_requires_all_dependencies() {
        assert_eq!(health_status(true), StatusCode::OK);
        assert_eq!(health_status(false), StatusCode::SERVICE_UNAVAILABLE);
    }
}
