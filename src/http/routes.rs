use std::net::SocketAddr;

use axum::{
    body::Body,
    extract::{ConnectInfo, State},
    http::{Request, Response, StatusCode},
    response::IntoResponse,
    routing::{any, get},
    Router,
};
use futures_util::future::join_all;
use serde_json::json;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing::warn;

use crate::{
    app::AppState,
    domain::{upstream, MemberTarget, ProviderTarget, Role},
    error::{AppError, AppResult},
    services::{member, provider, response},
};

const ROBLOX_HEALTH_PATH: &str = "/users/v1/users/1";

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
    let checks = async {
        tokio::join!(
            state.cache.ping(),
            state.roblox.ping(),
            proxies_healthy(&state)
        )
    };
    let healthy = match tokio::time::timeout(state.settings.request_timeout, checks).await {
        Ok((redis, roblox, proxies)) => {
            if let Err(error) = &redis {
                warn!(%error, "health check failed for Redis");
            }
            if !health_check_passed(&roblox) {
                let error = roblox.as_ref().unwrap_err();
                warn!(%error, "health check failed for Roblox");
            }

            redis.is_ok() && health_check_passed(&roblox) && proxies
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

async fn proxies_healthy(state: &AppState) -> bool {
    let targets = match proxy_health_targets(
        state.settings.role,
        &state.member_targets,
        &state.provider_targets,
    ) {
        Ok(targets) => targets,
        Err(error) => {
            warn!(%error, "could not build proxy health check targets");
            return false;
        }
    };

    join_all(targets.into_iter().map(|target| async {
        let host = target.host_str().unwrap_or("unknown").to_owned();
        let result = state.roblox.fetch_json::<serde_json::Value>(target).await;

        if !health_check_passed(&result) {
            let error = result.as_ref().unwrap_err();
            warn!(%error, %host, "health check failed for proxy");
        }

        health_check_passed(&result)
    }))
    .await
    .into_iter()
    .all(|healthy| healthy)
}

fn proxy_health_targets(
    role: Role,
    member_targets: &[MemberTarget],
    provider_targets: &[ProviderTarget],
) -> AppResult<Vec<url::Url>> {
    match role {
        Role::Member => member_targets
            .iter()
            .filter(|target| matches!(target, MemberTarget::Static(_)))
            .map(|target| upstream::member_target_url(target, ROBLOX_HEALTH_PATH, None))
            .collect(),
        Role::Provider => provider_targets
            .iter()
            .map(|target| upstream::provider_target_url(target, ROBLOX_HEALTH_PATH, None))
            .collect(),
    }
}

fn health_status(healthy: bool) -> StatusCode {
    if healthy {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    }
}

fn health_check_passed<T>(result: &AppResult<T>) -> bool {
    matches!(
        result,
        Ok(_)
            | Err(AppError::Upstream {
                status: StatusCode::TOO_MANY_REQUESTS,
                ..
            })
    )
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

    #[test]
    fn rate_limits_do_not_fail_health_checks() {
        let rate_limited: AppResult<()> = Err(AppError::Upstream {
            status: StatusCode::TOO_MANY_REQUESTS,
            message: "rate limited".to_owned(),
        });
        let unavailable: AppResult<()> = Err(AppError::Upstream {
            status: StatusCode::SERVICE_UNAVAILABLE,
            message: "unavailable".to_owned(),
        });

        assert!(health_check_passed(&Ok(())));
        assert!(health_check_passed(&rate_limited));
        assert!(!health_check_passed(&unavailable));
    }

    #[test]
    fn health_checks_every_configured_proxy() {
        let members = upstream::parse_member_targets(&[
            "direct://".to_owned(),
            "https://member-one.example.com".to_owned(),
            "https://member-two.example.com".to_owned(),
        ])
        .unwrap();
        let providers = upstream::parse_provider_targets(&[
            "https://provider-one.example.com".to_owned(),
            "https://provider-two.example.com".to_owned(),
        ])
        .unwrap();

        let member_urls = proxy_health_targets(Role::Member, &members, &providers).unwrap();
        let provider_urls = proxy_health_targets(Role::Provider, &members, &providers).unwrap();

        assert_eq!(member_urls.len(), 2);
        assert_eq!(provider_urls.len(), 2);
        assert!(member_urls
            .iter()
            .chain(&provider_urls)
            .all(|url| url.path() == ROBLOX_HEALTH_PATH));
    }
}
