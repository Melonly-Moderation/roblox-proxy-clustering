use url::Url;

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone)]
pub enum MemberTarget {
    Direct,
    Static(Url),
}

#[derive(Debug, Clone)]
pub struct ProviderTarget {
    pub base: Url,
}

pub fn parse_member_targets(raw: &[String]) -> AppResult<Vec<MemberTarget>> {
    if raw.is_empty() {
        return Err(AppError::BadGateway(
            "no member targets provided".to_owned(),
        ));
    }

    raw.iter()
        .map(|value| {
            if value.eq_ignore_ascii_case("direct://") {
                return Ok(MemberTarget::Direct);
            }

            let mut url = Url::parse(value)?;
            validate_http_url("member target", value, &url)?;
            let path = url.path().trim_end_matches('/').to_owned();
            url.set_path(&path);

            Ok(MemberTarget::Static(url))
        })
        .collect()
}

pub fn parse_provider_targets(raw: &[String]) -> AppResult<Vec<ProviderTarget>> {
    if raw.is_empty() {
        return Err(AppError::BadGateway(
            "no provider targets provided".to_owned(),
        ));
    }

    raw.iter()
        .map(|value| {
            let url = Url::parse(value)?;
            validate_http_url("provider target", value, &url)?;
            Ok(ProviderTarget { base: url })
        })
        .collect()
}

pub fn member_target_url(target: &MemberTarget, path: &str, query: Option<&str>) -> AppResult<Url> {
    match target {
        MemberTarget::Direct => resolve_roblox_target(path, query),
        MemberTarget::Static(base) => absolute_target_url(base, path, query),
    }
}

pub fn provider_target_url(
    target: &ProviderTarget,
    path: &str,
    query: Option<&str>,
) -> AppResult<Url> {
    absolute_target_url(&target.base, path, query)
}

pub fn resolve_roblox_target(path: &str, query: Option<&str>) -> AppResult<Url> {
    let mut segments = path.trim_start_matches('/').split('/');
    let domain = segments
        .next()
        .filter(|segment| !segment.is_empty())
        .ok_or_else(|| {
            AppError::BadGateway("unable to determine Roblox upstream from path".to_owned())
        })?;

    let remaining = segments.collect::<Vec<_>>().join("/");
    let rewritten = if remaining.is_empty() {
        "/".to_owned()
    } else {
        format!("/{remaining}")
    };

    let mut url = Url::parse(&format!("https://{domain}.roblox.com{rewritten}"))?;
    url.set_query(query.filter(|value| !value.is_empty()));
    Ok(url)
}

pub fn consistent_index(key: &str, buckets: usize) -> usize {
    if buckets == 0 {
        return 0;
    }

    let hash = key.as_bytes().iter().fold(0x811c9dc5u32, |hash, byte| {
        (hash ^ u32::from(*byte)).wrapping_mul(0x01000193)
    });

    hash as usize % buckets
}

pub fn routing_key(path: &str, query: Option<&str>) -> String {
    match query.filter(|value| !value.is_empty()) {
        Some(query) => format!("{path}?{query}"),
        None => path.to_owned(),
    }
}

fn absolute_target_url(base: &Url, path: &str, query: Option<&str>) -> AppResult<Url> {
    let mut url = base.clone();
    url.set_path(if path.is_empty() { "/" } else { path });
    url.set_query(query.filter(|value| !value.is_empty()));
    Ok(url)
}

fn validate_http_url(kind: &str, raw: &str, url: &Url) -> AppResult<()> {
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err(AppError::BadGateway(format!(
            "{kind} {raw:?} must use http or https scheme with a host"
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_direct_roblox_targets_like_legacy_paths() {
        let url = resolve_roblox_target("/users/v1/users/1", Some("debug=true")).unwrap();
        assert_eq!(
            url.as_str(),
            "https://users.roblox.com/v1/users/1?debug=true"
        );
    }

    #[test]
    fn computes_legacy_fnv_indices() {
        assert_eq!(consistent_index("/users/v1/users/1", 8), 5);
    }

    #[test]
    fn static_targets_preserve_host_and_replace_path() {
        let base = Url::parse("https://member.example.com/base").unwrap();
        let url = absolute_target_url(&base, "/thumbnails/v1", Some("x=1")).unwrap();
        assert_eq!(url.as_str(), "https://member.example.com/thumbnails/v1?x=1");
    }
}
