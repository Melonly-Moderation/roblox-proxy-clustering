use std::{env, time::Duration};

use crate::{domain::Role, error::ConfigError};

const DEFAULT_LISTEN_ADDR: &str = ":8080";
const DEFAULT_REQUEST_TIMEOUT: &str = "6s";
const DEFAULT_TRANSPORT_TIMEOUT: &str = "15s";
const DEFAULT_DIAL_TIMEOUT: &str = "750ms";
const DEFAULT_IDLE_CONN_TIMEOUT: &str = "90s";
const DEFAULT_MAX_IDLE_CONNS: usize = 512;
const DEFAULT_MAX_IDLE_CONNS_PER_HOST: usize = 256;
const DEFAULT_BACKGROUND_REFRESH_AFTER: &str = "5h";
const DEFAULT_CACHE_TTL: &str = "720h";

#[derive(Debug, Clone)]
pub struct Settings {
    pub role: Role,
    pub listen_addr: String,
    pub provider_clusters: Vec<String>,
    pub member_clusters: Vec<String>,
    pub redis_url: String,
    pub request_timeout: Duration,
    pub transport_timeout: Duration,
    pub dial_timeout: Duration,
    pub idle_conn_timeout: Duration,
    pub max_idle_conns: usize,
    pub max_idle_conns_per_host: usize,
    pub background_refresh_after: Duration,
    pub cache_ttl: Duration,
    pub discord_webhook_url: Option<String>,
}

impl Settings {
    pub fn from_env() -> Result<Self, ConfigError> {
        let role = Role::parse(&read_required("PROXY_ROLE")?)?;
        let redis_url = read_required("PROXY_REDIS_URL")?;

        let provider_clusters =
            split_csv(&read_optional("PROXY_PROVIDER_CLUSTERS").unwrap_or_default());
        let member_clusters =
            split_csv(&read_optional("PROXY_MEMBER_CLUSTERS").unwrap_or_default());

        match role {
            Role::Provider if provider_clusters.is_empty() => {
                return Err(ConfigError::Missing("PROXY_PROVIDER_CLUSTERS"));
            }
            Role::Member if member_clusters.is_empty() => {
                return Err(ConfigError::Missing("PROXY_MEMBER_CLUSTERS"));
            }
            _ => {}
        }

        Ok(Self {
            role,
            listen_addr: normalize_listen_addr(&read_env_or(
                "PROXY_LISTEN_ADDR",
                DEFAULT_LISTEN_ADDR,
            )?)?,
            provider_clusters,
            member_clusters,
            redis_url,
            request_timeout: read_duration_or("PROXY_REQUEST_TIMEOUT", DEFAULT_REQUEST_TIMEOUT)?,
            transport_timeout: read_duration_or(
                "PROXY_TRANSPORT_TIMEOUT",
                DEFAULT_TRANSPORT_TIMEOUT,
            )?,
            dial_timeout: read_duration_or("PROXY_DIAL_TIMEOUT", DEFAULT_DIAL_TIMEOUT)?,
            idle_conn_timeout: read_duration_or(
                "PROXY_IDLE_CONN_TIMEOUT",
                DEFAULT_IDLE_CONN_TIMEOUT,
            )?,
            max_idle_conns: read_usize_or("PROXY_MAX_IDLE_CONNS", DEFAULT_MAX_IDLE_CONNS)?.max(1),
            max_idle_conns_per_host: read_usize_or(
                "PROXY_MAX_IDLE_CONNS_PER_HOST",
                DEFAULT_MAX_IDLE_CONNS_PER_HOST,
            )?
            .max(1),
            background_refresh_after: read_positive_duration_or(
                "PROXY_BACKGROUND_REFRESH_AFTER",
                DEFAULT_BACKGROUND_REFRESH_AFTER,
            )?,
            cache_ttl: read_positive_duration_or("PROXY_CACHE_TTL", DEFAULT_CACHE_TTL)?,
            discord_webhook_url: read_optional("PROXY_DISCORD_WEBHOOK_URL"),
        })
    }
}

fn read_required(key: &'static str) -> Result<String, ConfigError> {
    let value = env::var(key).map_err(|_| ConfigError::Missing(key))?;
    let value = value.trim();

    if value.is_empty() {
        return Err(ConfigError::Empty(key));
    }

    Ok(value.to_owned())
}

fn read_optional(key: &'static str) -> Option<String> {
    env::var(key)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn read_env_or(key: &'static str, fallback: &'static str) -> Result<String, ConfigError> {
    Ok(read_optional(key).unwrap_or_else(|| fallback.to_owned()))
}

fn read_usize_or(key: &'static str, fallback: usize) -> Result<usize, ConfigError> {
    read_optional(key)
        .map(|value| {
            value
                .parse()
                .map_err(|source| ConfigError::InvalidInteger { key, source })
        })
        .transpose()
        .map(|value| value.unwrap_or(fallback))
}

fn read_duration_or(key: &'static str, fallback: &'static str) -> Result<Duration, ConfigError> {
    let raw = read_optional(key).unwrap_or_else(|| fallback.to_owned());
    humantime::parse_duration(&raw).map_err(|_| ConfigError::InvalidDuration { key, value: raw })
}

fn read_positive_duration_or(
    key: &'static str,
    fallback: &'static str,
) -> Result<Duration, ConfigError> {
    let duration = read_duration_or(key, fallback)?;

    if duration.is_zero() {
        return Err(ConfigError::MustBePositive(key));
    }

    Ok(duration)
}

fn split_csv(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn normalize_listen_addr(raw: &str) -> Result<String, ConfigError> {
    let raw = raw.trim();
    let normalized = if let Some(port) = raw.strip_prefix(':') {
        format!("0.0.0.0:{port}")
    } else if raw.chars().all(|ch| ch.is_ascii_digit()) {
        format!("0.0.0.0:{raw}")
    } else {
        raw.to_owned()
    };

    if !normalized.contains(':') {
        return Err(ConfigError::InvalidListenAddress {
            key: "PROXY_LISTEN_ADDR",
            value: raw.to_owned(),
        });
    }

    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_legacy_listen_addresses() {
        assert_eq!(normalize_listen_addr(":8080").unwrap(), "0.0.0.0:8080");
        assert_eq!(normalize_listen_addr("9090").unwrap(), "0.0.0.0:9090");
        assert_eq!(
            normalize_listen_addr("127.0.0.1:3000").unwrap(),
            "127.0.0.1:3000"
        );
    }

    #[test]
    fn splits_cluster_lists() {
        assert_eq!(
            split_csv(" direct://, https://member.example.com ,,"),
            vec!["direct://", "https://member.example.com"]
        );
    }
}
