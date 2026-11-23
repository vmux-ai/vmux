use super::auth::CallbackState;
use bevy::prelude::*;
use reqwest::blocking::{Client, Response};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;
use url::Url;

#[derive(Component, Clone)]
pub(super) struct Http(pub(super) Client);

impl Http {
    pub(super) fn new() -> Result<Self, String> {
        Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent("Vmux")
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map(Self)
            .map_err(|error| error.to_string())
    }
    pub(super) fn protected_metadata(&self, resource: &str) -> Result<ProtectedResource, String> {
        let metadata = Self::well_known_url(resource, "MCP resource", "oauth-protected-resource")?;
        Self::json(self.0.get(metadata).send())
    }

    pub(super) fn authorization_metadata(
        &self,
        issuer: &str,
    ) -> Result<AuthorizationServer, String> {
        let metadata = Self::well_known_url(issuer, "OAuth issuer", "oauth-authorization-server")?;
        Self::json(self.0.get(metadata).send())
    }

    pub(super) fn register(
        &self,
        authorization: &AuthorizationServer,
        redirect_uri: &str,
    ) -> Result<ClientRegistration, String> {
        let endpoint = authorization
            .registration_endpoint
            .as_ref()
            .ok_or_else(|| {
                "OAuth server does not support dynamic client registration".to_string()
            })?;
        let endpoint = Self::https_url(endpoint, "OAuth registration endpoint")?;
        Self::json(
            self.0
                .post(endpoint)
                .json(&RegistrationRequest {
                    client_name: "vmux",
                    redirect_uris: [redirect_uri],
                    grant_types: ["authorization_code", "refresh_token"],
                    response_types: ["code"],
                    token_endpoint_auth_method: "none",
                })
                .send(),
        )
    }

    pub(super) fn exchange(
        &self,
        endpoint: &str,
        registration: &ClientRegistration,
        code: &str,
        verifier: &str,
        redirect_uri: &str,
        resource: &str,
    ) -> Result<TokenResponse, String> {
        let mut form = vec![
            ("grant_type", "authorization_code".to_string()),
            ("code", code.to_string()),
            ("client_id", registration.client_id.clone()),
            ("redirect_uri", redirect_uri.to_string()),
            ("code_verifier", verifier.to_string()),
            ("resource", resource.to_string()),
        ];
        if let Some(secret) = &registration.client_secret {
            form.push(("client_secret", secret.clone()));
        }
        let endpoint = Self::https_url(endpoint, "OAuth token endpoint")?;
        Self::json(self.0.post(endpoint).form(&form).send())
    }

    pub(super) fn device_authorize(
        &self,
        endpoint: &str,
        client_id: &str,
        scope: &str,
    ) -> Result<DeviceAuthorization, String> {
        let endpoint = Self::https_url(endpoint, "device authorization endpoint")?;
        Self::json(
            self.0
                .post(endpoint)
                .header("Accept", "application/json")
                .form(&[("client_id", client_id), ("scope", scope)])
                .send(),
        )
    }

    pub(super) fn device_token(
        &self,
        endpoint: &str,
        client_id: &str,
        device_code: &str,
    ) -> Result<DeviceToken, String> {
        let endpoint = Self::https_url(endpoint, "token endpoint")?;
        let response = self
            .0
            .post(endpoint)
            .header("Accept", "application/json")
            .form(&[
                ("client_id", client_id),
                ("device_code", device_code),
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ])
            .send()
            .map_err(|error| error.to_string())?;
        response.json().map_err(|error| error.to_string())
    }

    pub(super) fn account(
        &self,
        endpoint: &str,
        access_token: &str,
    ) -> Result<Option<String>, String> {
        let endpoint = Self::https_url(endpoint, "account endpoint")?;
        let value: serde_json::Value =
            Self::json(self.0.get(endpoint).bearer_auth(access_token).send())?;
        Ok(value
            .get("preferred_username")
            .or_else(|| value.get("login"))
            .or_else(|| value.get("name"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_string))
    }

    fn json<T: for<'de> Deserialize<'de>>(
        response: Result<Response, reqwest::Error>,
    ) -> Result<T, String> {
        let response = response.map_err(|error| error.to_string())?;
        let status = response.status();
        let body = response.text().map_err(|error| error.to_string())?;
        if !status.is_success() {
            return Err(format!("OAuth request failed ({status}): {body}"));
        }
        serde_json::from_str(&body).map_err(|error| format!("invalid OAuth response: {error}"))
    }

    pub(super) fn https_url(value: &str, label: &str) -> Result<Url, String> {
        let url = Url::parse(value).map_err(|error| format!("invalid {label}: {error}"))?;
        if url.scheme() != "https" {
            return Err(format!("{label} must use https"));
        }
        Ok(url)
    }

    pub(super) fn well_known_url(value: &str, label: &str, metadata: &str) -> Result<Url, String> {
        let mut url = Self::https_url(value, label)?;
        let suffix = match url.path() {
            "/" => String::new(),
            path => path.to_string(),
        };
        url.set_path(&format!("/.well-known/{metadata}{suffix}"));
        url.set_query(None);
        url.set_fragment(None);
        Ok(url)
    }
}

#[derive(Component)]
pub(super) struct McpCallback {
    listener: TcpListener,
    pub(super) redirect: String,
}

impl McpCallback {
    pub(super) fn bind() -> Result<Self, String> {
        let listener = TcpListener::bind("127.0.0.1:0")
            .map_err(|error| format!("failed to open OAuth callback: {error}"))?;
        listener
            .set_nonblocking(true)
            .map_err(|error| error.to_string())?;
        let address = listener.local_addr().map_err(|error| error.to_string())?;
        Ok(Self {
            listener,
            redirect: format!("http://127.0.0.1:{}/callback", address.port()),
        })
    }

    pub(super) fn try_clone(&self) -> Result<Self, String> {
        let listener = self
            .listener
            .try_clone()
            .map_err(|error| error.to_string())?;
        Ok(Self {
            listener,
            redirect: self.redirect.clone(),
        })
    }

    pub(super) fn receive_once(
        &self,
        expected_state: &CallbackState,
    ) -> Result<Option<String>, String> {
        match self.listener.accept() {
            Ok((mut stream, _)) => Self::read(&mut stream, &expected_state.0).map(Some),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
            Err(error) => Err(format!("OAuth callback failed: {error}")),
        }
    }

    fn read(stream: &mut TcpStream, expected_state: &str) -> Result<String, String> {
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .map_err(|error| error.to_string())?;
        let max_request_line_bytes: u64 = 16 * 1024;
        let mut request = String::new();
        let mut reader = BufReader::new(&mut *stream).take(max_request_line_bytes + 1);
        let length = reader
            .read_line(&mut request)
            .map_err(|error| format!("failed to read OAuth callback: {error}"))?;
        if length == 0 || length as u64 > max_request_line_bytes || !request.ends_with('\n') {
            return Err("invalid OAuth callback".to_string());
        }
        let target = request
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .ok_or_else(|| "invalid OAuth callback".to_string())?;
        let result = Self::code(target, expected_state);
        let response = match result {
            Ok(_) => b"HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n".as_slice(),
            Err(_) => b"HTTP/1.1 400 Bad Request\r\nConnection: close\r\n\r\n".as_slice(),
        };
        stream
            .write_all(response)
            .map_err(|error| format!("failed to finish OAuth callback: {error}"))?;
        result
    }

    pub(super) fn code(target: &str, expected_state: &str) -> Result<String, String> {
        let url = Url::parse(&format!("http://127.0.0.1{target}"))
            .map_err(|error| format!("invalid OAuth callback: {error}"))?;
        let values = url
            .query_pairs()
            .collect::<std::collections::BTreeMap<_, _>>();
        let state = values
            .get("state")
            .ok_or_else(|| "OAuth callback omitted state".to_string())?;
        if state.as_ref() != expected_state {
            return Err("OAuth callback state did not match".to_string());
        }
        if let Some(error) = values.get("error") {
            return Err(values
                .get("error_description")
                .map(|description| description.to_string())
                .unwrap_or_else(|| error.to_string()));
        }
        let code = values
            .get("code")
            .ok_or_else(|| "OAuth callback omitted authorization code".to_string())?
            .to_string();
        Ok(code)
    }
}

#[derive(Component, Clone, Deserialize)]
pub(super) struct ProtectedResource {
    pub(super) authorization_servers: Vec<String>,
}

#[derive(Component, Clone, Deserialize)]
pub(super) struct AuthorizationServer {
    #[serde(default)]
    pub(super) device_authorization_endpoint: Option<String>,
    pub(super) authorization_endpoint: String,
    pub(super) token_endpoint: String,
    pub(super) registration_endpoint: Option<String>,
}

#[derive(Component, Clone, Deserialize)]
pub(super) struct DeviceAuthorization {
    pub(super) device_code: String,
    pub(super) user_code: String,
    pub(super) verification_uri: String,
    pub(super) expires_in: u64,
    pub(super) interval: Option<u64>,
}

#[derive(Component, Clone, Deserialize)]
pub(super) struct DeviceToken {
    pub(super) access_token: Option<String>,
    pub(super) refresh_token: Option<String>,
    pub(super) expires_in: Option<u64>,
    pub(super) scope: Option<String>,
    pub(super) error: Option<String>,
}

impl DeviceToken {
    pub(super) fn poll(self, interval: &mut u64) -> Result<Option<TokenResponse>, String> {
        if let Some(access_token) = self.access_token {
            if access_token.is_empty() {
                return Err("Authentication returned an empty token".to_string());
            }
            return Ok(Some(TokenResponse {
                access_token,
                refresh_token: self.refresh_token,
                expires_in: self.expires_in,
                scope: self.scope,
            }));
        }
        match self.error.as_deref() {
            Some("authorization_pending") => Ok(None),
            Some("slow_down") => {
                *interval = interval.saturating_add(5);
                Ok(None)
            }
            Some("access_denied") => Err("Authentication was denied".to_string()),
            Some("expired_token") => Err("Authentication timed out".to_string()),
            _ => Err("Authentication failed".to_string()),
        }
    }
}

#[derive(Serialize)]
struct RegistrationRequest<'a> {
    client_name: &'static str,
    redirect_uris: [&'a str; 1],
    grant_types: [&'static str; 2],
    response_types: [&'static str; 1],
    token_endpoint_auth_method: &'static str,
}

#[derive(Component, Clone, Deserialize)]
pub(super) struct ClientRegistration {
    pub(super) client_id: String,
    pub(super) client_secret: Option<String>,
}

#[derive(Component, Clone, Deserialize)]
pub(super) struct TokenResponse {
    pub(super) access_token: String,
    pub(super) refresh_token: Option<String>,
    pub(super) expires_in: Option<u64>,
    pub(super) scope: Option<String>,
}
