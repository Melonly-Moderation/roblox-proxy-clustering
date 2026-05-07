use std::{net::SocketAddr, sync::Arc};

use axum::{
    body::Body,
    http::{Request, Response, StatusCode},
};
use futures_util::future::join_all;
use serde::de::DeserializeOwned;
use url::{form_urlencoded, Url};

use crate::{
    app::AppState,
    domain::{
        roblox::{
            first_avatar_url, AvatarPayload, RobloxAvatarResponse, RobloxSearchResponse,
            RobloxUserResponse, SearchPayloadItem, UserLookupPayload,
        },
        upstream::{consistent_index, member_target_url, routing_key},
    },
    error::{AppError, AppResult},
    services::{cache, proxy, response},
};

#[derive(Clone)]
struct MemberService {
    state: AppState,
}

pub async fn handle(
    state: AppState,
    remote_addr: SocketAddr,
    request: Request<Body>,
) -> AppResult<Response<Body>> {
    let service = MemberService { state };

    if let Some(user_id) = query_param(request.uri().query(), "userId") {
        return service.handle_user_lookup(user_id).await;
    }

    if let Some(search) = query_param(request.uri().query(), "search") {
        return service.handle_search(search).await;
    }

    service.handle_proxy(remote_addr, request).await
}

impl MemberService {
    async fn handle_proxy(
        &self,
        remote_addr: SocketAddr,
        request: Request<Body>,
    ) -> AppResult<Response<Body>> {
        let target = self.pick_target(request.uri().path(), request.uri().query())?;
        proxy::forward(&self.state, remote_addr, request, target).await
    }

    async fn handle_user_lookup(&self, user_id: String) -> AppResult<Response<Body>> {
        if !is_numeric(&user_id) {
            return Ok(response::json_bytes(
                StatusCode::BAD_REQUEST,
                br#"{"error":"Invalid or missing userId"}"#.to_vec(),
                self.state.settings.role,
                None,
            ));
        }

        let key = format!("roblox:user:{user_id}");
        let fetcher = {
            let service = self.clone();
            let user_id = user_id.clone();
            Arc::new(move || {
                let service = service.clone();
                let user_id = user_id.clone();
                Box::pin(async move { service.fetch_user_payload(&user_id).await }) as _
            })
        };

        let payload = cache::read_through(&self.state, key, fetcher).await?;
        Ok(response::json_bytes(
            StatusCode::OK,
            payload,
            self.state.settings.role,
            Some("max-age=18000"),
        ))
    }

    async fn handle_search(&self, search: String) -> AppResult<Response<Body>> {
        let needle = search.trim().to_owned();

        if needle.len() < 3 {
            return Ok(response::json_bytes(
                StatusCode::BAD_REQUEST,
                b"[]".to_vec(),
                self.state.settings.role,
                None,
            ));
        }

        let key = format!("roblox:search:{}", needle.to_ascii_lowercase());
        let fetcher = {
            let service = self.clone();
            let needle = needle.clone();
            Arc::new(move || {
                let service = service.clone();
                let needle = needle.clone();
                Box::pin(async move { service.fetch_search_payload(&needle).await }) as _
            })
        };

        let payload = cache::read_through(&self.state, key, fetcher).await?;
        Ok(response::json_bytes(
            StatusCode::OK,
            payload,
            self.state.settings.role,
            Some("max-age=18000"),
        ))
    }

    async fn fetch_user_payload(&self, user_id: &str) -> AppResult<Vec<u8>> {
        let user: RobloxUserResponse = self
            .fetch_json(
                "users",
                &format!("/v1/users/{user_id}"),
                Vec::<(&str, String)>::new(),
            )
            .await?;

        let avatar: RobloxAvatarResponse = self
            .fetch_json(
                "thumbnails",
                "/v1/users/avatar-bust",
                vec![
                    ("userIds", user_id.to_owned()),
                    ("size", "48x48".to_owned()),
                    ("format", "Png".to_owned()),
                    ("isCircular", "false".to_owned()),
                ],
            )
            .await?;

        let payload = UserLookupPayload {
            description: user.description,
            created: user.created,
            is_banned: user.is_banned,
            id: user.id,
            name: user.name,
            display_name: user.display_name,
            avatar_url: first_avatar_url(&avatar.data),
        };

        Ok(serde_json::to_vec(&payload)?)
    }

    async fn fetch_search_payload(&self, query: &str) -> AppResult<Vec<u8>> {
        let search: RobloxSearchResponse = self
            .fetch_json(
                "apis",
                "/search-api/omni-search",
                vec![
                    ("verticalType", "user".to_owned()),
                    ("searchQuery", query.to_owned()),
                    ("globalSessionId", "TridentBot".to_owned()),
                    ("sessionId", "TridentBot".to_owned()),
                ],
            )
            .await?;

        let Some(contents) = search
            .search_results
            .first()
            .map(|bucket| bucket.contents.clone())
            .filter(|contents| !contents.is_empty())
        else {
            return Ok(serde_json::to_vec(&Vec::<SearchPayloadItem>::new())?);
        };

        let avatars = join_all(contents.iter().map(|entry| {
            let service = self.clone();
            let user_id = entry.content_id.to_string();
            async move {
                service
                    .lookup_avatar_url(&user_id)
                    .await
                    .unwrap_or_default()
            }
        }))
        .await;

        let payload = contents
            .into_iter()
            .zip(avatars.into_iter())
            .map(|(entry, avatar_url)| SearchPayloadItem {
                player_id: entry.content_id.to_string(),
                name: entry.username,
                avatar_url,
            })
            .collect::<Vec<_>>();

        Ok(serde_json::to_vec(&payload)?)
    }

    async fn lookup_avatar_url(&self, user_id: &str) -> AppResult<String> {
        let key = format!("roblox:avatar:{user_id}");
        let fetcher = {
            let service = self.clone();
            let user_id = user_id.to_owned();
            Arc::new(move || {
                let service = service.clone();
                let user_id = user_id.clone();
                Box::pin(async move { service.fetch_avatar_payload(&user_id).await }) as _
            })
        };

        let payload = cache::read_through(&self.state, key, fetcher).await?;
        let avatar = serde_json::from_slice::<AvatarPayload>(&payload)?;
        Ok(avatar.url)
    }

    async fn fetch_avatar_payload(&self, user_id: &str) -> AppResult<Vec<u8>> {
        let avatar: RobloxAvatarResponse = self
            .fetch_json(
                "thumbnails",
                "/v1/users/avatar-bust",
                vec![
                    ("userIds", user_id.to_owned()),
                    ("size", "420x420".to_owned()),
                    ("format", "Png".to_owned()),
                    ("isCircular", "false".to_owned()),
                ],
            )
            .await?;

        Ok(serde_json::to_vec(&AvatarPayload {
            url: first_avatar_url(&avatar.data),
        })?)
    }

    async fn fetch_json<T>(
        &self,
        service: &str,
        path: &str,
        params: Vec<(&str, String)>,
    ) -> AppResult<T>
    where
        T: DeserializeOwned,
    {
        let base_path = format!(
            "/{}/{}",
            service.trim_matches('/'),
            path.trim_start_matches('/')
        );
        let query = encode_query(params);
        let target = self.pick_target(&base_path, query.as_deref())?;

        tracing::info!(service, path = %base_path, query = ?query, target = %target, "fetching Roblox JSON");
        self.state.roblox.fetch_json(target).await
    }

    fn pick_target(&self, path: &str, query: Option<&str>) -> AppResult<Url> {
        if self.state.member_targets.is_empty() {
            return Err(AppError::BadGateway(
                "no upstream target available".to_owned(),
            ));
        }

        let key = routing_key(path, query);
        let index = consistent_index(&key, self.state.member_targets.len());
        member_target_url(&self.state.member_targets[index], path, query)
    }
}

fn encode_query(params: Vec<(&str, String)>) -> Option<String> {
    if params.is_empty() {
        return None;
    }

    let mut serializer = form_urlencoded::Serializer::new(String::new());
    for (key, value) in params {
        serializer.append_pair(key, &value);
    }

    Some(serializer.finish())
}

fn query_param(query: Option<&str>, key: &str) -> Option<String> {
    form_urlencoded::parse(query.unwrap_or_default().as_bytes())
        .find_map(|(name, value)| (name == key).then(|| value.trim().to_owned()))
        .filter(|value| !value.is_empty())
}

fn is_numeric(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_query_parameters() {
        assert_eq!(
            query_param(Some("search=noah&x=1"), "search"),
            Some("noah".to_owned())
        );
        assert_eq!(query_param(Some("search=%20%20"), "search"), None);
    }

    #[test]
    fn validates_numeric_ids() {
        assert!(is_numeric("123"));
        assert!(!is_numeric("12x"));
        assert!(!is_numeric(""));
    }
}
