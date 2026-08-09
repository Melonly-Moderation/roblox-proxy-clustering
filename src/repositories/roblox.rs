use axum::http::StatusCode;
use reqwest::header;
use serde::de::DeserializeOwned;
use url::Url;

use crate::{
    error::{AppError, AppResult},
    infrastructure::{DiscordWebhook, ProxyHttpClient},
};

const USER_AGENT: &str = "RobloxProxyCluster/1.0";
const MAX_ERROR_BODY_BYTES: usize = 1 << 20;
const HEALTH_CHECK_URL: &str = "https://users.roblox.com/v1/users/1";

#[derive(Clone)]
pub struct RobloxRepository {
    http: ProxyHttpClient,
    webhook: DiscordWebhook,
}

impl RobloxRepository {
    pub fn new(http: ProxyHttpClient, webhook: DiscordWebhook) -> Self {
        Self { http, webhook }
    }

    pub async fn ping(&self) -> AppResult<()> {
        self.fetch_json::<serde_json::Value>(Url::parse(HEALTH_CHECK_URL)?)
            .await?;

        Ok(())
    }

    pub async fn fetch_json<T>(&self, target: Url) -> AppResult<T>
    where
        T: DeserializeOwned,
    {
        let response = self
            .http
            .inner()
            .get(target.clone())
            .header(header::USER_AGENT, USER_AGENT)
            .header(header::ACCEPT, "application/json")
            .send()
            .await?;

        let status = response.status();
        let bytes = response.bytes().await?;

        if status == StatusCode::TOO_MANY_REQUESTS {
            self.webhook
                .spawn_send(format!("Received 429 from upstream: {target}"));
        }

        if !status.is_success() {
            return Err(AppError::Upstream {
                status: client_status_for_upstream(status),
                message: upstream_error_message(status, &bytes),
            });
        }

        Ok(serde_json::from_slice(&bytes)?)
    }
}

fn client_status_for_upstream(status: StatusCode) -> StatusCode {
    match status {
        StatusCode::TOO_MANY_REQUESTS => StatusCode::TOO_MANY_REQUESTS,
        StatusCode::NOT_FOUND => StatusCode::NOT_FOUND,
        StatusCode::SERVICE_UNAVAILABLE => StatusCode::SERVICE_UNAVAILABLE,
        status if status.is_server_error() => StatusCode::BAD_GATEWAY,
        _ => StatusCode::BAD_GATEWAY,
    }
}

fn upstream_error_message(status: StatusCode, bytes: &[u8]) -> String {
    let message = error_message(bytes);

    if message.is_empty() {
        format!("roblox upstream returned {status}")
    } else {
        format!("roblox upstream returned {status}: {message}")
    }
}

fn error_message(bytes: &[u8]) -> String {
    let bytes = &bytes[..bytes.len().min(MAX_ERROR_BODY_BYTES)];

    serde_json::from_slice::<serde_json::Value>(bytes)
        .ok()
        .and_then(|value| {
            value
                .get("error")
                .and_then(|error| error.as_str())
                .map(str::to_owned)
        })
        .filter(|message| !message.is_empty())
        .unwrap_or_else(|| String::from_utf8_lossy(bytes).trim().to_owned())
}
