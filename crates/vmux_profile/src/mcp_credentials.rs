mod access;
mod storage;

use std::fmt::{Debug, Formatter};

use serde::{Deserialize, Serialize};

pub use access::McpCredentialAccess;
pub use storage::McpCredentialStorage;

const DEFAULT_TOKEN_LIFETIME_SECS: u64 = 3600;

#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpOauthCredentials {
    pub token_endpoint: String,
    pub client_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<String>,
    pub access_token: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub expires_at: i64,
    #[serde(default)]
    pub scope: String,
    #[serde(default)]
    pub resource: String,
}

impl Debug for McpOauthCredentials {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("McpOauthCredentials")
            .field("token_endpoint", &self.token_endpoint)
            .field("client_id", &self.client_id)
            .field(
                "client_secret",
                &self.client_secret.as_ref().map(|_| "[REDACTED]"),
            )
            .field("access_token", &"[REDACTED]")
            .field(
                "refresh_token",
                &self.refresh_token.as_ref().map(|_| "[REDACTED]"),
            )
            .field("expires_at", &self.expires_at)
            .field("scope", &self.scope)
            .field("resource", &self.resource)
            .finish()
    }
}

impl McpOauthCredentials {
    pub fn expires_soon(&self) -> bool {
        self.expires_at != 0 && self.expires_at <= chrono::Utc::now().timestamp() + 60
    }

    pub fn expires_at(expires_in: Option<u64>) -> i64 {
        let lifetime = expires_in.unwrap_or(DEFAULT_TOKEN_LIFETIME_SECS);
        let lifetime = i64::try_from(lifetime).unwrap_or(i64::MAX);
        chrono::Utc::now().timestamp().saturating_add(lifetime)
    }

    pub fn authorizes(&self, resource: &str) -> bool {
        let Ok(stored) = url::Url::parse(&self.resource) else {
            return false;
        };
        let Ok(requested) = url::Url::parse(resource) else {
            return false;
        };
        stored.scheme() == "https" && requested.scheme() == "https" && stored == requested
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expiry_keeps_tokens_with_more_than_a_minute_left() {
        let credentials = McpOauthCredentials {
            expires_at: chrono::Utc::now().timestamp() + 61,
            ..McpOauthCredentials::default()
        };

        assert!(!credentials.expires_soon());
    }

    #[test]
    fn expiry_refreshes_tokens_with_a_minute_left() {
        let credentials = McpOauthCredentials {
            expires_at: chrono::Utc::now().timestamp() + 60,
            ..McpOauthCredentials::default()
        };

        assert!(credentials.expires_soon());
    }

    #[test]
    fn missing_expiry_uses_a_bounded_lifetime() {
        let before = chrono::Utc::now().timestamp() + DEFAULT_TOKEN_LIFETIME_SECS as i64;
        let expires_at = McpOauthCredentials::expires_at(None);
        let after = chrono::Utc::now().timestamp() + DEFAULT_TOKEN_LIFETIME_SECS as i64;

        assert!((before..=after).contains(&expires_at));
    }

    #[test]
    fn oauth_credentials_only_authorize_the_registered_resource() {
        let credentials = McpOauthCredentials {
            resource: "https://mcp.linear.app/mcp".to_string(),
            ..McpOauthCredentials::default()
        };

        assert!(credentials.authorizes("https://mcp.linear.app/mcp"));
        assert!(credentials.authorizes("https://MCP.LINEAR.APP:443/mcp"));
        assert!(!credentials.authorizes("https://example.com/mcp"));
        assert!(!credentials.authorizes("http://mcp.linear.app/mcp"));
    }

    #[test]
    fn debug_output_redacts_secrets() {
        let credentials = McpOauthCredentials {
            client_secret: Some("client-secret".to_string()),
            access_token: "access-secret".to_string(),
            refresh_token: Some("refresh-secret".to_string()),
            ..McpOauthCredentials::default()
        };
        let debug = format!("{credentials:?}");

        assert!(!debug.contains("client-secret"));
        assert!(!debug.contains("access-secret"));
        assert!(!debug.contains("refresh-secret"));
    }
}
