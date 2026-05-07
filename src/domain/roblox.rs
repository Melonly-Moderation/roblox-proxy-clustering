use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct RobloxUserResponse {
    pub description: String,
    pub created: String,

    #[serde(rename = "isBanned")]
    pub is_banned: bool,

    pub id: i64,
    pub name: String,

    #[serde(rename = "displayName")]
    pub display_name: String,
}

#[derive(Debug, Deserialize)]
pub struct RobloxAvatarResponse {
    #[serde(default)]
    pub data: Vec<RobloxAvatarItem>,
}

#[derive(Debug, Deserialize)]
pub struct RobloxAvatarItem {
    #[serde(default, rename = "imageUrl")]
    pub image_url: String,
}

#[derive(Debug, Deserialize)]
pub struct RobloxSearchResponse {
    #[serde(default, rename = "searchResults")]
    pub search_results: Vec<RobloxSearchBucket>,
}

#[derive(Debug, Deserialize)]
pub struct RobloxSearchBucket {
    #[serde(default)]
    pub contents: Vec<RobloxSearchContent>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RobloxSearchContent {
    #[serde(rename = "contentId")]
    pub content_id: i64,

    #[serde(default)]
    pub username: String,
}

#[derive(Debug, Serialize)]
pub struct UserLookupPayload {
    pub description: String,
    pub created: String,

    #[serde(rename = "isBanned")]
    pub is_banned: bool,

    pub id: i64,
    pub name: String,

    #[serde(rename = "displayName")]
    pub display_name: String,

    #[serde(rename = "avatarUrl")]
    pub avatar_url: String,
}

#[derive(Debug, Serialize)]
pub struct SearchPayloadItem {
    #[serde(rename = "playerId")]
    pub player_id: String,

    pub name: String,

    #[serde(rename = "avatarUrl")]
    pub avatar_url: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AvatarPayload {
    pub url: String,
}

pub fn first_avatar_url(items: &[RobloxAvatarItem]) -> String {
    items
        .first()
        .map(|item| item.image_url.clone())
        .unwrap_or_default()
}
