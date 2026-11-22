use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use reqwest::blocking::{Client, Response};
use ring::digest::{SHA256, digest};
use serde::{Deserialize, Serialize};
use url::Url;
use vmux_profile::mcp_credentials::{
    McpCredentialAccess, McpCredentialStorage, McpOauthCredentials,
};
use vmux_profile::safe_storage::SafeStorageName;
use vmux_profile::{Profile, ProfilePaths};
use zeroize::Zeroize;

pub struct McpOauthFlow {
    profile: Profile,
    paths: ProfilePaths,
    storage: McpCredentialStorage,
    access: McpCredentialAccess,
    client: Client,
    #[cfg(test)]
    allow_loopback_http: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum McpOauthStatus {
    Connected,
    Expired,
    Missing,
    ResourceMismatch,
    Unreadable,
}

impl McpOauthStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Connected => "oauth: connected",
            Self::Expired => "oauth: expired (refreshable)",
            Self::Missing => "oauth: not authenticated",
            Self::ResourceMismatch => "oauth: URL mismatch (re-auth required)",
            Self::Unreadable => "oauth: credential unreadable",
        }
    }
}

struct ZeroizedCredentials(McpOauthCredentials);

impl Drop for ZeroizedCredentials {
    fn drop(&mut self) {
        self.0.token_endpoint.zeroize();
        self.0.client_id.zeroize();
        self.0.client_secret.zeroize();
        self.0.access_token.zeroize();
        self.0.refresh_token.zeroize();
        self.0.scope.zeroize();
        self.0.resource.zeroize();
    }
}

#[derive(Deserialize)]
struct ProtectedResource {
    authorization_servers: Vec<String>,
}

#[derive(Deserialize)]
struct AuthorizationServer {
    authorization_endpoint: String,
    token_endpoint: String,
    registration_endpoint: Option<String>,
}

#[derive(Serialize)]
struct RegistrationRequest<'a> {
    client_name: &'static str,
    redirect_uris: [&'a str; 1],
    grant_types: [&'static str; 2],
    response_types: [&'static str; 1],
    token_endpoint_auth_method: &'static str,
}

#[derive(Deserialize)]
struct ClientRegistration {
    client_id: String,
    client_secret: Option<String>,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
    scope: Option<String>,
}

impl McpOauthFlow {
    pub fn for_profile(profile: Profile, paths: ProfilePaths) -> Result<Self, String> {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|error| error.to_string())?;
        Ok(Self {
            storage: McpCredentialStorage::at(profile.clone(), paths.clone()),
            profile,
            paths,
            access: McpCredentialAccess::default(),
            client,
            #[cfg(test)]
            allow_loopback_http: false,
        })
    }

    pub fn status(&self, server_id: &str, resource_url: &str) -> McpOauthStatus {
        match self.load_credentials(server_id) {
            Ok(Some(credentials)) => {
                let credentials = ZeroizedCredentials(credentials);
                if !self.authorizes(&credentials.0, resource_url) {
                    McpOauthStatus::ResourceMismatch
                } else if credentials.0.expires_soon() {
                    McpOauthStatus::Expired
                } else {
                    McpOauthStatus::Connected
                }
            }
            Ok(None) => McpOauthStatus::Missing,
            Err(_) => McpOauthStatus::Unreadable,
        }
    }

    pub fn logout(&self, server_id: &str) -> Result<(), String> {
        self.access.write(|| self.remove_credentials(server_id))
    }

    pub fn authorize_resolved(
        &self,
        resolved: &crate::McpResolveResult,
        server_id: &str,
        open_url: impl FnOnce(&str) -> Result<(), String>,
    ) -> Result<(), String> {
        let server = resolved
            .servers
            .iter()
            .find(|server| server.id == server_id)
            .ok_or_else(|| format!("MCP server not found: {server_id}"))?;
        let Some((url, scopes)) = server.spec.oauth_config() else {
            return Err(format!(
                "MCP server \"{server_id}\" is not configured for OAuth"
            ));
        };
        self.authorize(server_id, url, scopes, open_url)
    }

    pub fn authorize(
        &self,
        server_id: &str,
        resource_url: &str,
        scopes: &[String],
        open_url: impl FnOnce(&str) -> Result<(), String>,
    ) -> Result<(), String> {
        let listener = TcpListener::bind("127.0.0.1:0")
            .map_err(|error| format!("failed to bind OAuth callback: {error}"))?;
        let port = listener
            .local_addr()
            .map_err(|error| error.to_string())?
            .port();
        let redirect_uri = format!("http://127.0.0.1:{port}/callback");
        let protected: ProtectedResource =
            self.discover_json(resource_url, "MCP resource", "oauth-protected-resource")?;
        let issuer = protected
            .authorization_servers
            .first()
            .ok_or_else(|| "MCP server did not advertise an authorization server".to_string())?;
        let authorization: AuthorizationServer =
            self.discover_json(issuer, "OAuth issuer", "oauth-authorization-server")?;
        let registration = self.register(&authorization, &redirect_uri)?;
        let verifier = format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        );
        let challenge = Self::base64_url(digest(&SHA256, verifier.as_bytes()).as_ref());
        let state = uuid::Uuid::new_v4().simple().to_string();
        let mut auth_url = self.endpoint_url(
            &authorization.authorization_endpoint,
            "OAuth authorization endpoint",
        )?;
        auth_url
            .query_pairs_mut()
            .append_pair("response_type", "code")
            .append_pair("client_id", &registration.client_id)
            .append_pair("redirect_uri", &redirect_uri)
            .append_pair("code_challenge", &challenge)
            .append_pair("code_challenge_method", "S256")
            .append_pair("state", &state)
            .append_pair("resource", resource_url);
        if !scopes.is_empty() {
            auth_url
                .query_pairs_mut()
                .append_pair("scope", &scopes.join(" "));
        }
        open_url(auth_url.as_str())?;
        let code = Self::wait_for_callback(listener, &state)?;
        let token = self.exchange(
            &authorization.token_endpoint,
            &registration,
            &code,
            &verifier,
            &redirect_uri,
            resource_url,
        )?;
        let credentials = ZeroizedCredentials(McpOauthCredentials {
            token_endpoint: authorization.token_endpoint,
            client_id: registration.client_id,
            client_secret: registration.client_secret,
            access_token: token.access_token,
            refresh_token: token.refresh_token,
            expires_at: McpOauthCredentials::expires_at(token.expires_in),
            scope: token.scope.unwrap_or_else(|| scopes.join(" ")),
            resource: resource_url.to_string(),
        });
        self.access
            .write(|| self.store_credentials(server_id, &credentials.0))
    }

    pub fn access_token(
        &self,
        server_id: &str,
        resource_url: &str,
        force_refresh: bool,
    ) -> Result<String, String> {
        let current = self
            .access
            .read(|| self.load_for(server_id, resource_url))?;
        let Some(current) = current else {
            return Err(format!(
                "MCP server \"{server_id}\" is not authenticated; run `vmux tools mcp auth {server_id}`"
            ));
        };
        if !force_refresh && !current.0.expires_soon() && !current.0.access_token.is_empty() {
            return Ok(current.0.access_token.clone());
        }
        self.access.refresh(|| {
            let latest = self
                .access
                .read(|| self.load_for(server_id, resource_url))?
                .ok_or_else(|| format!("MCP server \"{server_id}\" credentials removed"))?;
            if !force_refresh && !latest.0.expires_soon() && !latest.0.access_token.is_empty() {
                return Ok(latest.0.access_token.clone());
            }
            let mut refreshed = ZeroizedCredentials(latest.0.clone());
            self.refresh(&mut refreshed.0)?;
            self.access.write(|| {
                let stored = self
                    .load_for(server_id, resource_url)?
                    .ok_or_else(|| format!("MCP server \"{server_id}\" credentials removed"))?;
                if stored.0 != latest.0 && !stored.0.access_token.is_empty() {
                    return Ok(stored.0.access_token.clone());
                }
                self.store_credentials(server_id, &refreshed.0)?;
                Ok(refreshed.0.access_token.clone())
            })
        })
    }

    fn load_for(
        &self,
        server_id: &str,
        resource_url: &str,
    ) -> Result<Option<ZeroizedCredentials>, String> {
        let Some(credentials) = self.load_credentials(server_id)? else {
            return Ok(None);
        };
        let credentials = ZeroizedCredentials(credentials);
        if !self.authorizes(&credentials.0, resource_url) {
            return Err(format!(
                "stored OAuth credentials for \"{server_id}\" do not match {resource_url}"
            ));
        }
        Ok(Some(credentials))
    }

    fn is_desktop_only_error(error: &str) -> bool {
        error.contains("only available to the desktop process")
    }

    fn fallback_path(&self, server_id: &str) -> PathBuf {
        let account = format!("{}:{server_id}", self.profile);
        self.paths
            .profile()
            .join("mcp-credentials")
            .join(format!("{}.json", SafeStorageName::of(&account)))
    }

    fn read_fallback(&self, server_id: &str) -> Result<Option<McpOauthCredentials>, String> {
        match std::fs::read(self.fallback_path(server_id)) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map(Some)
                .map_err(|err| err.to_string()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(err.to_string()),
        }
    }

    pub(crate) fn load_credentials(
        &self,
        server_id: &str,
    ) -> Result<Option<McpOauthCredentials>, String> {
        match self.storage.load(server_id) {
            Ok(Some(value)) => Ok(Some(value)),
            Ok(None) => self.read_fallback(server_id),
            Err(error) if Self::is_desktop_only_error(&error) => self.read_fallback(server_id),
            Err(error) => Err(error),
        }
    }

    pub(crate) fn store_credentials(
        &self,
        server_id: &str,
        credentials: &McpOauthCredentials,
    ) -> Result<(), String> {
        match self.storage.store(server_id, credentials) {
            Ok(()) => Ok(()),
            Err(error) if Self::is_desktop_only_error(&error) => {
                let path = self.fallback_path(server_id);
                let parent = path.parent().ok_or("MCP credential path has no parent")?;
                std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
                let bytes = serde_json::to_vec(credentials).map_err(|err| err.to_string())?;
                let mut options = std::fs::OpenOptions::new();
                options.create(true).truncate(true).write(true);
                #[cfg(unix)]
                {
                    options.mode(0o600);
                }
                let mut file = options.open(&path).map_err(|err| err.to_string())?;
                #[cfg(unix)]
                {
                    file.set_permissions(std::fs::Permissions::from_mode(0o600))
                        .map_err(|err| err.to_string())?;
                }
                file.write_all(&bytes).map_err(|err| err.to_string())
            }
            Err(error) => Err(error),
        }
    }

    fn remove_credentials(&self, server_id: &str) -> Result<(), String> {
        let primary = match self.storage.remove(server_id) {
            Ok(()) => Ok(()),
            Err(error) if Self::is_desktop_only_error(&error) => Ok(()),
            Err(error) => Err(error),
        };
        let fallback = match std::fs::remove_file(self.fallback_path(server_id)) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(err.to_string()),
        };
        primary.and(fallback)
    }

    fn authorizes(&self, credentials: &McpOauthCredentials, resource_url: &str) -> bool {
        if credentials.authorizes(resource_url) {
            return true;
        }
        #[cfg(test)]
        if self.allow_loopback_http {
            return Url::parse(&credentials.resource).ok() == Url::parse(resource_url).ok();
        }
        false
    }

    fn register(
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
        let endpoint = self.endpoint_url(endpoint, "OAuth registration endpoint")?;
        Self::decode_json(
            self.client
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

    fn exchange(
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
        let endpoint = self.endpoint_url(endpoint, "OAuth token endpoint")?;
        Self::decode_json(self.client.post(endpoint).form(&form).send())
    }

    fn refresh(&self, credentials: &mut McpOauthCredentials) -> Result<(), String> {
        let refresh_token = credentials
            .refresh_token
            .as_ref()
            .ok_or_else(|| "OAuth session expired and has no refresh token".to_string())?;
        let mut form = vec![
            ("grant_type", "refresh_token".to_string()),
            ("refresh_token", refresh_token.clone()),
            ("client_id", credentials.client_id.clone()),
        ];
        if let Some(secret) = &credentials.client_secret {
            form.push(("client_secret", secret.clone()));
        }
        if !credentials.resource.is_empty() {
            form.push(("resource", credentials.resource.clone()));
        }
        if !credentials.scope.is_empty() {
            form.push(("scope", credentials.scope.clone()));
        }
        let endpoint = self.endpoint_url(&credentials.token_endpoint, "OAuth token endpoint")?;
        let token: TokenResponse =
            Self::decode_json(self.client.post(endpoint).form(&form).send())?;
        if token.access_token.is_empty() {
            return Err("OAuth refresh response returned an empty access token".to_string());
        }
        credentials.access_token = token.access_token;
        if token.refresh_token.is_some() {
            credentials.refresh_token = token.refresh_token;
        }
        credentials.expires_at = McpOauthCredentials::expires_at(token.expires_in);
        if let Some(scope) = token.scope {
            credentials.scope = scope;
        }
        Ok(())
    }

    fn discover_json<T: for<'de> Deserialize<'de>>(
        &self,
        value: &str,
        label: &str,
        metadata: &str,
    ) -> Result<T, String> {
        let suffixed = self.well_known_url(value, label, metadata, true)?;
        match self.get_json(suffixed) {
            Ok(decoded) => Ok(decoded),
            Err(first_error) => {
                let root = self.well_known_url(value, label, metadata, false)?;
                self.get_json(root).map_err(|_| first_error)
            }
        }
    }

    fn get_json<T: for<'de> Deserialize<'de>>(&self, url: Url) -> Result<T, String> {
        Self::decode_json(self.client.get(url).send())
    }

    fn decode_json<T: for<'de> Deserialize<'de>>(
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

    fn endpoint_url(&self, value: &str, label: &str) -> Result<Url, String> {
        let url = Url::parse(value).map_err(|error| format!("invalid {label}: {error}"))?;
        if url.scheme() == "https" {
            return Ok(url);
        }
        #[cfg(test)]
        if self.allow_loopback_http
            && url.scheme() == "http"
            && matches!(url.host_str(), Some("127.0.0.1" | "localhost"))
        {
            return Ok(url);
        }
        Err(format!("{label} must use https"))
    }

    fn well_known_url(
        &self,
        value: &str,
        label: &str,
        metadata: &str,
        include_path: bool,
    ) -> Result<Url, String> {
        let mut url = self.endpoint_url(value, label)?;
        let suffix = if include_path {
            match url.path() {
                "/" => String::new(),
                path => path.to_string(),
            }
        } else {
            String::new()
        };
        url.set_path(&format!("/.well-known/{metadata}{suffix}"));
        url.set_query(None);
        url.set_fragment(None);
        Ok(url)
    }

    fn wait_for_callback(listener: TcpListener, expected_state: &str) -> Result<String, String> {
        listener
            .set_nonblocking(true)
            .map_err(|error| error.to_string())?;
        let deadline = Instant::now() + Duration::from_secs(300);
        loop {
            match listener.accept() {
                Ok((mut stream, _)) => return Self::read_callback(&mut stream, expected_state),
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    if Instant::now() >= deadline {
                        return Err("OAuth authorization timed out".to_string());
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(error) => return Err(format!("OAuth callback failed: {error}")),
            }
        }
    }

    fn read_callback(stream: &mut TcpStream, expected_state: &str) -> Result<String, String> {
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .map_err(|error| error.to_string())?;
        let max_bytes: u64 = 16 * 1024;
        let mut request = String::new();
        let mut reader = BufReader::new(&mut *stream).take(max_bytes + 1);
        let length = reader
            .read_line(&mut request)
            .map_err(|error| format!("failed to read OAuth callback: {error}"))?;
        if length == 0 || length as u64 > max_bytes || !request.ends_with('\n') {
            return Err("invalid OAuth callback".to_string());
        }
        let target = request
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .ok_or_else(|| "invalid OAuth callback".to_string())?;
        let result = Self::parse_callback_code(target, expected_state);
        let response = match result {
            Ok(_) => b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\nAuthenticated. You may close this tab.".as_slice(),
            Err(_) => b"HTTP/1.1 400 Bad Request\r\nConnection: close\r\n\r\nOAuth failed.".as_slice(),
        };
        let _ = stream.write_all(response);
        result
    }

    fn parse_callback_code(target: &str, expected_state: &str) -> Result<String, String> {
        let url = Url::parse(&format!("http://127.0.0.1{target}"))
            .map_err(|error| format!("invalid OAuth callback: {error}"))?;
        let values: BTreeMap<_, _> = url.query_pairs().collect();
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
        values
            .get("code")
            .map(|code| code.to_string())
            .ok_or_else(|| "OAuth callback omitted authorization code".to_string())
    }

    fn base64_url(bytes: &[u8]) -> String {
        let table = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
        let mut encoded = String::with_capacity(bytes.len().div_ceil(3) * 4);
        for chunk in bytes.chunks(3) {
            let first = chunk[0];
            let second = chunk.get(1).copied().unwrap_or_default();
            let third = chunk.get(2).copied().unwrap_or_default();
            encoded.push(table[(first >> 2) as usize] as char);
            encoded.push(table[(((first & 0b11) << 4) | (second >> 4)) as usize] as char);
            if chunk.len() > 1 {
                encoded.push(table[(((second & 0b1111) << 2) | (third >> 6)) as usize] as char);
            }
            if chunk.len() > 2 {
                encoded.push(table[(third & 0b111111) as usize] as char);
            }
        }
        encoded
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    impl McpOauthFlow {
        pub(crate) fn for_loopback_test(profile_dir: PathBuf, profile_name: String) -> Self {
            let profile = Profile::named(&profile_name);
            let paths = ProfilePaths::at(profile.clone(), profile_dir);
            let mut flow = Self::for_profile(profile, paths).unwrap();
            flow.allow_loopback_http = true;
            flow
        }
    }

    #[test]
    fn two_servers_sharing_one_remote_url_maintain_isolated_tokens_and_refresh() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let base = format!("http://127.0.0.1:{port}");
        let server_thread = std::thread::spawn(move || {
            for expected_refresh in ["refresh-vmx", "refresh-mistral"] {
                let (mut stream, _) = listener.accept().unwrap();
                let mut buf = [0u8; 4096];
                let count = stream.read(&mut buf).unwrap();
                let req = String::from_utf8_lossy(&buf[..count]);
                assert!(req.contains(expected_refresh));
                let next = if expected_refresh == "refresh-vmx" {
                    r#"{"access_token":"tok-vmx-2","refresh_token":"refresh-vmx-2","expires_in":3600}"#
                } else {
                    r#"{"access_token":"tok-mistral-2","refresh_token":"refresh-mistral-2","expires_in":3600}"#
                };
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{next}",
                    next.len()
                );
                stream.write_all(resp.as_bytes()).unwrap();
            }
        });

        let dir = tempfile::tempdir().unwrap();
        let resource = format!("{base}/mcp");
        let flow = McpOauthFlow::for_loopback_test(dir.path().to_path_buf(), "test".into());
        for (id, token, refresh) in [
            ("linear-vmx", "tok-vmx-1", "refresh-vmx"),
            ("linear-mistral", "tok-mistral-1", "refresh-mistral"),
        ] {
            flow.store_credentials(
                id,
                &McpOauthCredentials {
                    token_endpoint: format!("{base}/token"),
                    client_id: format!("cid-{id}"),
                    client_secret: None,
                    access_token: token.into(),
                    refresh_token: Some(refresh.into()),
                    expires_at: 1,
                    scope: "read write".into(),
                    resource: resource.clone(),
                },
            )
            .unwrap();
        }

        assert_eq!(
            flow.status("linear-vmx", &resource),
            McpOauthStatus::Expired
        );
        let tok_vmx = flow.access_token("linear-vmx", &resource, false).unwrap();
        let tok_mistral = flow
            .access_token("linear-mistral", &resource, false)
            .unwrap();
        assert_eq!(tok_vmx, "tok-vmx-2");
        assert_eq!(tok_mistral, "tok-mistral-2");
        assert_eq!(
            flow.status("linear-vmx", &resource),
            McpOauthStatus::Connected
        );

        flow.logout("linear-mistral").unwrap();
        assert_eq!(
            flow.status("linear-mistral", &resource),
            McpOauthStatus::Missing
        );
        assert_eq!(
            flow.access_token("linear-vmx", &resource, false).unwrap(),
            "tok-vmx-2"
        );
        server_thread.join().unwrap();
    }
}
