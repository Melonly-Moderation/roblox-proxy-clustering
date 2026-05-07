use std::time::Duration;

use reqwest::Client;
use serde_json::json;
use tracing::warn;

#[derive(Clone)]
pub struct DiscordWebhook {
    client: Client,
    url: Option<String>,
}

impl DiscordWebhook {
    pub fn new(url: Option<String>) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(5))
            .use_rustls_tls()
            .build()
            .unwrap_or_else(|_| Client::new());

        Self { client, url }
    }

    pub async fn send(&self, message: impl Into<String>) {
        let Some(url) = self.url.as_deref().filter(|url| !url.is_empty()) else {
            return;
        };

        let response = self
            .client
            .post(url)
            .header("User-Agent", "MelonlyBot/12.0.0")
            .json(&json!({
                "username": "RobloxProxyCluster",
                "content": message.into(),
            }))
            .send()
            .await;

        if let Err(error) = response {
            warn!(%error, "discord webhook delivery failed");
        }
    }

    pub fn spawn_send(&self, message: impl Into<String>) {
        let webhook = self.clone();
        let message = message.into();

        tokio::spawn(async move {
            webhook.send(message).await;
        });
    }
}
