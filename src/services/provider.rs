use std::{net::SocketAddr, sync::atomic::Ordering};

use axum::{
    body::Body,
    http::{Request, Response, StatusCode},
};

use crate::{
    app::AppState,
    domain::upstream::{provider_target_url, routing_key},
    error::{AppError, AppResult},
    services::{proxy, response},
};

pub async fn handle(
    state: AppState,
    remote_addr: SocketAddr,
    request: Request<Body>,
) -> AppResult<Response<Body>> {
    if state.provider_targets.is_empty() {
        return Ok(response::json_error(
            StatusCode::BAD_GATEWAY,
            "no provider upstreams configured",
            state.settings.role,
        ));
    }

    let path = request.uri().path().to_owned();
    let query = request.uri().query().map(str::to_owned);
    let _key = routing_key(&path, query.as_deref());
    let index =
        state.provider_cursor.fetch_add(1, Ordering::Relaxed) % state.provider_targets.len();
    let target = provider_target_url(&state.provider_targets[index], &path, query.as_deref())
        .map_err(|error| AppError::BadGateway(error.to_string()))?;

    proxy::forward(&state, remote_addr, request, target).await
}
