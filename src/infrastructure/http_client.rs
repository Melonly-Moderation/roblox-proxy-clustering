use reqwest::Client;
use std::time::Duration;

use crate::{config::Settings, error::ConfigError};

#[derive(Clone)]
pub struct ProxyHttpClient {
    client: Client,
}

impl ProxyHttpClient {
    pub fn new(settings: &Settings) -> Result<Self, ConfigError> {
        let client = Client::builder()
            .connect_timeout(settings.dial_timeout)
            .pool_idle_timeout(settings.idle_conn_timeout)
            .pool_max_idle_per_host(settings.max_idle_conns_per_host)
            .tcp_keepalive(Duration::from_secs(60))
            .timeout(settings.transport_timeout)
            .http2_adaptive_window(true)
            .use_rustls_tls()
            .build()?;

        Ok(Self { client })
    }

    pub fn inner(&self) -> &Client {
        &self.client
    }
}
