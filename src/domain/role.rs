use serde::Serialize;

use crate::error::ConfigError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Provider,
    Member,
}

impl Role {
    pub fn parse(raw: &str) -> Result<Self, ConfigError> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "provider" => Ok(Self::Provider),
            "member" => Ok(Self::Member),
            value => Err(ConfigError::InvalidRole {
                value: value.to_owned(),
            }),
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Provider => "provider",
            Self::Member => "member",
        }
    }
}

impl std::fmt::Display for Role {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}
