use std::net::SocketAddr;

use axum::{
    body::Body,
    http::{header, HeaderMap, HeaderName, HeaderValue, Request, Response},
};
use reqwest::StatusCode;
use url::Url;

use crate::{app::AppState, error::AppResult};

const HOP_HEADERS: &[&str] = &[
    "connection",
    "proxy-connection",
    "keep-alive",
    "proxy-authenticate",
    "proxy-authorization",
    "te",
    "trailer",
    "transfer-encoding",
    "upgrade",
];

pub async fn forward(
    state: &AppState,
    remote_addr: SocketAddr,
    request: Request<Body>,
    target: Url,
) -> AppResult<Response<Body>> {
    let (parts, body) = request.into_parts();
    tracing::info!(method = %parts.method, uri = %parts.uri, target = %target, "forwarding request");

    let mut headers = outbound_headers(&parts.headers, remote_addr);
    set_forwarded_headers(&mut headers, &parts.headers, remote_addr);

    let response = state
        .http
        .inner()
        .request(parts.method, target.clone())
        .headers(headers)
        .body(reqwest::Body::wrap_stream(body.into_data_stream()))
        .send()
        .await?;

    if response.status() == StatusCode::TOO_MANY_REQUESTS {
        state
            .webhook
            .spawn_send(format!("Received 429 from upstream: {target}"));
    }

    stream_response(state, response).await
}

async fn stream_response(
    state: &AppState,
    response: reqwest::Response,
) -> AppResult<Response<Body>> {
    let mut builder = Response::builder().status(response.status());

    if let Some(headers) = builder.headers_mut() {
        for (name, value) in response.headers() {
            if !is_hop_header(name) {
                headers.append(name, value.clone());
            }
        }

        headers.insert(
            "x-proxy-role",
            HeaderValue::from_static(state.settings.role.as_str()),
        );
    }

    Ok(builder.body(Body::from_stream(response.bytes_stream()))?)
}

fn outbound_headers(inbound: &HeaderMap, remote_addr: SocketAddr) -> HeaderMap {
    let mut headers = HeaderMap::with_capacity(inbound.len() + 3);

    for (name, value) in inbound {
        if !is_hop_header(name) && *name != header::HOST {
            headers.append(name, value.clone());
        }
    }

    if !headers.contains_key(header::USER_AGENT) {
        headers.insert(
            header::USER_AGENT,
            HeaderValue::from_static("RobloxProxyCluster/1.0"),
        );
    }

    if !headers.contains_key("x-real-ip") {
        if let Ok(value) = HeaderValue::from_str(&remote_addr.ip().to_string()) {
            headers.insert("x-real-ip", value);
        }
    }

    headers
}

fn set_forwarded_headers(headers: &mut HeaderMap, inbound: &HeaderMap, remote_addr: SocketAddr) {
    let remote_ip = remote_addr.ip().to_string();
    let forwarded_for = inbound
        .get("x-forwarded-for")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .map(|value| format!("{value}, {remote_ip}"))
        .unwrap_or(remote_ip);

    if let Ok(value) = HeaderValue::from_str(&forwarded_for) {
        headers.insert("x-forwarded-for", value);
    }

    if !headers.contains_key("x-forwarded-proto") {
        headers.insert("x-forwarded-proto", HeaderValue::from_static("http"));
    }

    if let Some(host) = inbound.get(header::HOST) {
        headers.insert("x-forwarded-host", host.clone());
    }
}

fn is_hop_header(name: &HeaderName) -> bool {
    HOP_HEADERS
        .iter()
        .any(|candidate| name.as_str().eq_ignore_ascii_case(candidate))
}
