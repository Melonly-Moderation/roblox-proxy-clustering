use std::{net::AddrParseError, num::ParseIntError};

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use thiserror::Error;
use tracing::error;

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, Error)]
pub enum AppError {
    #[error(transparent)]
    Config(#[from] ConfigError),

    #[error("redis error: {0}")]
    Redis(#[from] redis::RedisError),

    #[error("http client error: {0}")]
    HttpClient(#[from] reqwest::Error),

    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("url error: {0}")]
    Url(#[from] url::ParseError),

    #[error("http response build error: {0}")]
    HttpBuild(#[from] axum::http::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("bad request: {0}")]
    BadRequest(String),

    #[error("bad gateway: {0}")]
    BadGateway(String),
}

impl AppError {
    fn status_and_message(&self) -> (StatusCode, String) {
        match self {
            Self::BadRequest(message) => (StatusCode::BAD_REQUEST, message.clone()),
            Self::BadGateway(message) => (StatusCode::BAD_GATEWAY, message.clone()),
            Self::Config(_)
            | Self::Redis(_)
            | Self::HttpClient(_)
            | Self::Serialization(_)
            | Self::Url(_)
            | Self::HttpBuild(_)
            | Self::Io(_) => {
                error!(error = %self, "request failed");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal server error".to_owned(),
                )
            }
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = self.status_and_message();
        (status, Json(json!({ "error": message }))).into_response()
    }
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("missing required environment variable {0}")]
    Missing(&'static str),

    #[error("environment variable {0} cannot be empty")]
    Empty(&'static str),

    #[error("invalid PROXY_ROLE {value:?}: must be \"provider\" or \"member\"")]
    InvalidRole { value: String },

    #[error("invalid listen address in {key}: {value}")]
    InvalidListenAddress { key: &'static str, value: String },

    #[error("invalid integer for {key}: {source}")]
    InvalidInteger {
        key: &'static str,
        #[source]
        source: ParseIntError,
    },

    #[error("invalid duration for {key}: {value}")]
    InvalidDuration { key: &'static str, value: String },

    #[error("environment variable {0} must be greater than zero")]
    MustBePositive(&'static str),

    #[error("invalid IP address: {0}")]
    Address(#[from] AddrParseError),

    #[error("invalid HTTP client settings: {0}")]
    HttpClient(#[from] reqwest::Error),
}
