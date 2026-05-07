use axum::{
    body::Body,
    http::{header, HeaderValue, Response, StatusCode},
};
use serde_json::json;

use crate::domain::Role;

pub fn json_bytes(
    status: StatusCode,
    payload: Vec<u8>,
    role: Role,
    cache_control: Option<&'static str>,
) -> Response<Body> {
    let mut response = Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .header("x-proxy-role", role.as_str())
        .body(Body::from(payload))
        .expect("static response builder is valid");

    if let Some(value) = cache_control {
        response
            .headers_mut()
            .insert(header::CACHE_CONTROL, HeaderValue::from_static(value));
    }

    response
}

pub fn json_error(status: StatusCode, message: impl AsRef<str>, role: Role) -> Response<Body> {
    let payload = serde_json::to_vec(&json!({ "error": message.as_ref() }))
        .unwrap_or_else(|_| b"{\"error\":\"internal server error\"}".to_vec());

    json_bytes(status, payload, role, None)
}
