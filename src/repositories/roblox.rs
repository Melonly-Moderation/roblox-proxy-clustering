use reqwest::{header, StatusCode};
use serde::de::DeserializeOwned;
use url::Url;

use crate::{
    error::{AppError, AppResult},
    infrastructure::{DiscordWebhook, ProxyHttpClient},
};

const USER_AGENT: &str = "RobloxProxyCluster/1.0";
const MAX_ERROR_BODY_BYTES: usize = 1 << 20;

#[derive(Clone)]
pub struct RobloxRepository {
    http: ProxyHttpClient,
    webhook: DiscordWebhook,
}

impl RobloxRepository {
    pub fn new(http: ProxyHttpClient, webhook: DiscordWebhook) -> Self {
        Self { http, webhook }
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
            return Err(AppError::BadGateway(format!(
                "roblox request failed: {status} {}",
                error_message(&bytes)
            )));
        }

        Ok(serde_json::from_slice(&bytes)?)
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
