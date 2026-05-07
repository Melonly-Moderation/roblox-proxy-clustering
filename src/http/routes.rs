use std::net::SocketAddr;

use axum::{
    body::Body,
    extract::{ConnectInfo, State},
    http::{Request, Response},
    response::IntoResponse,
    routing::any,
    Router,
};
use tower_http::{cors::CorsLayer, trace::TraceLayer};

use crate::{
    app::AppState,
    domain::Role,
    services::{member, provider},
};

pub fn router(state: AppState) -> Router {
    Router::new()
        .fallback(any(dispatch))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
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
