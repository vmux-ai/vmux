use base64::Engine;
use reqwest::blocking::{Client, RequestBuilder, Response};
use ring::digest::{SHA256, digest};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use url::Url;
use vmux_ecs::manifest::{AuthFlow as AuthFlowManifest, AuthProvider};
use vmux_profile::oauth::OauthCredentials;
use zeroize::{Zeroize, Zeroizing};

pub enum AuthProgress {
    AwaitingUser {
        verification_uri: String,
        user_code: String,
        expires_at: i64,
    },
}

#[derive(Clone, Default)]
pub(crate) struct Cancellation(Arc<AtomicBool>);

impl Cancellation {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub fn is_canceled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }

    fn check(&self) -> Result<(), String> {
        if self.is_canceled() {
            return Err("OAuth authorization was canceled".to_string());
        }
        Ok(())
    }

    fn wait(&self, duration: Duration) -> Result<(), String> {
        let deadline = Instant::now()
            .checked_add(duration)
            .ok_or_else(|| "OAuth wait duration is invalid".to_string())?;
        loop {
            self.check()?;
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Ok(());
            }
            std::thread::sleep(remaining.min(Duration::from_millis(100)));
        }
    }
}

pub(crate) enum Revocation {
    Revoked,
    Unsupported,
}

pub struct AuthFlow;

impl AuthFlow {
    const MAX_DEVICE_EXPIRY_SECS: u64 = 3600;
    const MAX_DEVICE_INTERVAL_SECS: u64 = 300;
    const MAX_RESPONSE_BYTES: u64 = 256 * 1024;

    pub fn connect(
        provider: &AuthProvider,
        cancellation: &Cancellation,
        progress: impl FnOnce(AuthProgress),
    ) -> Result<OauthCredentials, String> {
        cancellation.check()?;
        match &provider.flow {
            AuthFlowManifest::DeviceCode(flow) => Self::device(flow, cancellation, progress),
            AuthFlowManifest::AuthorizationCodePkce(flow) => {
                Self::pkce(flow, cancellation, progress)
            }
        }
    }

    pub(crate) fn revoke(
        provider: &AuthProvider,
        credentials: &OauthCredentials,
    ) -> Result<Revocation, String> {
        let Some(endpoint) = &provider.revocation_endpoint else {
            return Ok(Revocation::Unsupported);
        };
        let (client_id, headers) = match &provider.flow {
            AuthFlowManifest::DeviceCode(flow) => (&flow.client_id, &flow.headers),
            AuthFlowManifest::AuthorizationCodePkce(flow) => (&flow.client_id, &flow.headers),
        };
        Self::validate_client_id(client_id)?;
        let endpoint = Self::https_url(endpoint, "revocation endpoint")?;
        let client = Self::client()?;
        let mut errors = Vec::new();
        if let Some(refresh_token) = credentials.refresh_token.as_deref()
            && !refresh_token.is_empty()
            && let Err(error) = Self::revoke_token(
                &client,
                endpoint.clone(),
                headers,
                client_id,
                refresh_token,
                "refresh_token",
            )
        {
            errors.push(error);
        }
        if credentials.access_token.is_empty() {
            errors.push("OAuth access token is empty".to_string());
        } else if credentials.refresh_token.as_deref() != Some(credentials.access_token.as_str())
            && let Err(error) = Self::revoke_token(
                &client,
                endpoint,
                headers,
                client_id,
                &credentials.access_token,
                "access_token",
            )
        {
            errors.push(error);
        }
        if !errors.is_empty() {
            return Err(errors.join("; "));
        }
        Ok(Revocation::Revoked)
    }

    pub(crate) fn revoke_retrying(
        provider: &AuthProvider,
        credentials: &OauthCredentials,
        attempts: usize,
    ) -> Result<Revocation, String> {
        let attempts = attempts.max(1);
        let mut last = None;
        for attempt in 0..attempts {
            match Self::revoke(provider, credentials) {
                Ok(result) => return Ok(result),
                Err(error) => last = Some(error),
            }
            if attempt + 1 < attempts {
                std::thread::sleep(Duration::from_millis(200 * (attempt as u64 + 1)));
            }
        }
        Err(last.unwrap_or_else(|| "OAuth revocation failed".to_string()))
    }

    fn device(
        flow: &vmux_ecs::manifest::DeviceCodeFlow,
        cancellation: &Cancellation,
        progress: impl FnOnce(AuthProgress),
    ) -> Result<OauthCredentials, String> {
        cancellation.check()?;
        Self::validate_client_id(&flow.client_id)?;
        let device_endpoint = Self::https_url(
            &flow.device_authorization_endpoint,
            "device authorization endpoint",
        )?;
        let token_endpoint = Self::https_url(&flow.token_endpoint, "token endpoint")?;
        let client = Self::client()?;
        let scopes = flow.scopes.join(" ");
        let response = Self::request(
            client.post(device_endpoint).form(&[
                ("client_id", flow.client_id.as_str()),
                ("scope", scopes.as_str()),
            ]),
            &flow.headers,
        )?
        .send()
        .map_err(|error| error.to_string())?;
        let device: DeviceAuthorizationResponse = Self::json(response, "device authorization")?;
        let verification_uri = device
            .verification_uri_complete
            .clone()
            .unwrap_or_else(|| device.verification_uri.clone());
        let verification_uri = Self::https_url(&verification_uri, "verification URI")?;
        if device.expires_in == 0 || device.expires_in > Self::MAX_DEVICE_EXPIRY_SECS {
            return Err("OAuth device authorization expiry is invalid".to_string());
        }
        let interval_seconds = device.interval.unwrap_or(5);
        if interval_seconds == 0 || interval_seconds > Self::MAX_DEVICE_INTERVAL_SECS {
            return Err("OAuth device authorization poll interval is invalid".to_string());
        }
        progress(AuthProgress::AwaitingUser {
            verification_uri: verification_uri.to_string(),
            user_code: device.user_code.clone(),
            expires_at: chrono::Utc::now()
                .timestamp()
                .saturating_add(i64::try_from(device.expires_in).unwrap_or(i64::MAX)),
        });
        cancellation.check()?;
        Self::open(verification_uri.as_str())?;
        let deadline = Instant::now()
            .checked_add(Duration::from_secs(device.expires_in))
            .ok_or_else(|| "OAuth device authorization expiry is invalid".to_string())?;
        let mut interval = Duration::from_secs(interval_seconds);
        loop {
            cancellation.check()?;
            let now = Instant::now();
            if now >= deadline {
                return Err("OAuth device authorization expired".to_string());
            }
            cancellation.wait(interval.min(deadline.saturating_duration_since(now)))?;
            let response = Self::request(
                client.post(token_endpoint.clone()).form(&[
                    ("client_id", flow.client_id.as_str()),
                    ("device_code", device.device_code.as_str()),
                    ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ]),
                &flow.headers,
            )?
            .send()
            .map_err(|error| error.to_string())?;
            cancellation.check()?;
            let (status, body) = Self::body(response, "token")?;
            if status.is_success() {
                let token = serde_json::from_slice::<TokenResponse>(&body)
                    .map_err(|error| format!("invalid OAuth token response: {error}"))?;
                return Self::credentials(
                    &flow.client_id,
                    &flow.token_endpoint,
                    &flow.scopes,
                    token,
                );
            }
            let error = serde_json::from_slice::<OauthError>(&body)
                .map(|error| error.error)
                .unwrap_or_else(|_| format!("HTTP {status}"));
            match error.as_str() {
                "authorization_pending" => {}
                "slow_down" => {
                    let next = interval
                        .checked_add(Duration::from_secs(5))
                        .ok_or_else(|| "OAuth device poll interval is invalid".to_string())?;
                    if next > Duration::from_secs(Self::MAX_DEVICE_INTERVAL_SECS) {
                        return Err("OAuth device poll interval is unsupported".to_string());
                    }
                    interval = next;
                }
                "expired_token" => return Err("OAuth device authorization expired".to_string()),
                "access_denied" => return Err("OAuth device authorization was denied".to_string()),
                _ => return Err(format!("OAuth token request failed: {error}")),
            }
        }
    }

    fn pkce(
        flow: &vmux_ecs::manifest::AuthorizationCodePkceFlow,
        cancellation: &Cancellation,
        _progress: impl FnOnce(AuthProgress),
    ) -> Result<OauthCredentials, String> {
        cancellation.check()?;
        Self::validate_client_id(&flow.client_id)?;
        let listener = TcpListener::bind("127.0.0.1:0")
            .map_err(|error| format!("failed to open OAuth callback: {error}"))?;
        let address = listener
            .local_addr()
            .map_err(|error| format!("failed to read OAuth callback address: {error}"))?;
        let redirect_uri = format!("http://127.0.0.1:{}/callback", address.port());
        let verifier = Zeroizing::new(format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        ));
        let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(digest(&SHA256, verifier.as_bytes()).as_ref());
        let state = uuid::Uuid::new_v4().simple().to_string();
        let mut authorization =
            Self::https_url(&flow.authorization_endpoint, "authorization endpoint")?;
        authorization
            .query_pairs_mut()
            .append_pair("response_type", "code")
            .append_pair("client_id", &flow.client_id)
            .append_pair("redirect_uri", &redirect_uri)
            .append_pair("code_challenge", &challenge)
            .append_pair("code_challenge_method", "S256")
            .append_pair("state", &state)
            .append_pair("scope", &flow.scopes.join(" "));
        cancellation.check()?;
        Self::open(authorization.as_str())?;
        let code = Zeroizing::new(Callback::receive(listener, &state, cancellation)?);
        cancellation.check()?;
        let client = Self::client()?;
        let endpoint = Self::https_url(&flow.token_endpoint, "token endpoint")?;
        let response = Self::request(
            client.post(endpoint).form(&[
                ("grant_type", "authorization_code"),
                ("code", code.as_str()),
                ("client_id", flow.client_id.as_str()),
                ("redirect_uri", redirect_uri.as_str()),
                ("code_verifier", verifier.as_str()),
            ]),
            &flow.headers,
        )?
        .send()
        .map_err(|error| error.to_string())?;
        let token = Self::json(response, "token")?;
        cancellation.check()?;
        Self::credentials(&flow.client_id, &flow.token_endpoint, &flow.scopes, token)
    }

    pub(crate) fn request(
        mut request: RequestBuilder,
        headers: &BTreeMap<String, String>,
    ) -> Result<RequestBuilder, String> {
        for (name, value) in headers {
            let name = reqwest::header::HeaderName::from_bytes(name.as_bytes())
                .map_err(|error| format!("invalid OAuth header name: {error}"))?;
            let value = reqwest::header::HeaderValue::from_str(value)
                .map_err(|error| format!("invalid OAuth header value: {error}"))?;
            request = request.header(name, value);
        }
        Ok(request)
    }

    pub(crate) fn client() -> Result<Client, String> {
        Client::builder()
            .timeout(Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|error| error.to_string())
    }

    fn credentials(
        client_id: &str,
        token_endpoint: &str,
        configured_scopes: &[String],
        mut token: TokenResponse,
    ) -> Result<OauthCredentials, String> {
        if token.access_token.trim().is_empty() {
            return Err("OAuth token response contains an empty access token".to_string());
        }
        Ok(OauthCredentials {
            client_id: client_id.to_string(),
            token_endpoint: token_endpoint.to_string(),
            access_token: std::mem::take(&mut token.access_token),
            refresh_token: token.refresh_token.take(),
            expires_at: OauthCredentials::expires_at(token.expires_in),
            scopes: token
                .scope
                .take()
                .map(|scope| scope.split_whitespace().map(str::to_string).collect())
                .unwrap_or_else(|| configured_scopes.to_vec()),
            token_type: token
                .token_type
                .take()
                .unwrap_or_else(|| "Bearer".to_string()),
        })
    }

    pub(crate) fn json<T: for<'de> Deserialize<'de>>(
        response: Response,
        label: &str,
    ) -> Result<T, String> {
        let (status, body) = Self::body(response, label)?;
        if !status.is_success() {
            return Err(format!("OAuth {label} request failed ({status})"));
        }
        serde_json::from_slice(&body)
            .map_err(|error| format!("invalid OAuth {label} response: {error}"))
    }

    fn body(
        response: Response,
        label: &str,
    ) -> Result<(reqwest::StatusCode, Zeroizing<Vec<u8>>), String> {
        let status = response.status();
        let mut body = Zeroizing::new(Vec::new());
        response
            .take(Self::MAX_RESPONSE_BYTES + 1)
            .read_to_end(&mut body)
            .map_err(|error| format!("failed to read OAuth {label} response: {error}"))?;
        if body.len() as u64 > Self::MAX_RESPONSE_BYTES {
            return Err(format!("OAuth {label} response is too large"));
        }
        Ok((status, body))
    }

    fn revoke_token(
        client: &Client,
        endpoint: Url,
        headers: &BTreeMap<String, String>,
        client_id: &str,
        token: &str,
        token_type: &str,
    ) -> Result<(), String> {
        let response = Self::request(
            client.post(endpoint).form(&[
                ("token", token),
                ("token_type_hint", token_type),
                ("client_id", client_id),
            ]),
            headers,
        )?
        .send()
        .map_err(|error| error.to_string())?;
        if !response.status().is_success() {
            return Err(format!(
                "OAuth {token_type} revocation failed ({})",
                response.status()
            ));
        }
        Ok(())
    }

    fn validate_client_id(client_id: &str) -> Result<(), String> {
        if client_id.trim().is_empty() {
            return Err("OAuth provider client ID is not configured".to_string());
        }
        Ok(())
    }

    pub(crate) fn https_url(value: &str, label: &str) -> Result<Url, String> {
        let url = Url::parse(value).map_err(|error| format!("invalid OAuth {label}: {error}"))?;
        if url.scheme() != "https" {
            return Err(format!("OAuth {label} must use https"));
        }
        Ok(url)
    }

    fn open(url: &str) -> Result<(), String> {
        AuthorizationOpener::open(url)
    }
}

struct AuthorizationOpener;

impl AuthorizationOpener {
    fn open(url: &str) -> Result<(), String> {
        #[cfg(target_os = "macos")]
        let program = "/usr/bin/open";
        #[cfg(not(target_os = "macos"))]
        let program = "/usr/bin/xdg-open";
        let mut child = std::process::Command::new(program)
            .arg(url)
            .spawn()
            .map_err(|error| format!("failed to open OAuth authorization page: {error}"))?;
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            match child.try_wait() {
                Ok(Some(status)) if status.success() => return Ok(()),
                Ok(Some(status)) => {
                    return Err(format!("OAuth authorization opener exited with {status}"));
                }
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(25));
                }
                Ok(None) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Ok(());
                }
                Err(error) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!("OAuth authorization opener failed: {error}"));
                }
            }
        }
    }
}

struct Callback;

impl Callback {
    fn receive(
        listener: TcpListener,
        expected_state: &str,
        cancellation: &Cancellation,
    ) -> Result<String, String> {
        listener
            .set_nonblocking(true)
            .map_err(|error| error.to_string())?;
        let deadline = Instant::now() + Duration::from_secs(300);
        loop {
            cancellation.check()?;
            if Instant::now() >= deadline {
                return Err("OAuth authorization timed out".to_string());
            }
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let connection_deadline =
                        std::cmp::min(deadline, Instant::now() + Duration::from_secs(2));
                    match Self::read(
                        &mut stream,
                        expected_state,
                        connection_deadline,
                        cancellation,
                    ) {
                        CallbackAttempt::Code(code) => return Ok(code),
                        CallbackAttempt::Denied(error) => return Err(error),
                        CallbackAttempt::Ignore => {}
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    cancellation.wait(Duration::from_millis(50))?;
                }
                Err(error) => return Err(format!("OAuth callback failed: {error}")),
            }
        }
    }

    fn read(
        stream: &mut TcpStream,
        expected_state: &str,
        deadline: Instant,
        cancellation: &Cancellation,
    ) -> CallbackAttempt {
        if stream.set_nonblocking(true).is_err() {
            return CallbackAttempt::Ignore;
        }
        let mut request = Vec::new();
        let mut buffer = [0_u8; 1024];
        loop {
            if cancellation.is_canceled() || Instant::now() >= deadline {
                return Self::reject(stream);
            }
            match stream.read(&mut buffer) {
                Ok(0) => return Self::reject(stream),
                Ok(length) => {
                    request.extend_from_slice(&buffer[..length]);
                    if request.len() > 16 * 1024 {
                        return Self::reject(stream);
                    }
                    if request.contains(&b'\n') {
                        break;
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(25));
                }
                Err(_) => return Self::reject(stream),
            }
        }
        let Ok(request) = std::str::from_utf8(&request) else {
            return Self::reject(stream);
        };
        let Some(target) = request
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
        else {
            let _ = stream.write_all(Self::response(false).as_bytes());
            return CallbackAttempt::Ignore;
        };
        let Ok(url) = Url::parse(&format!("http://127.0.0.1{target}")) else {
            let _ = stream.write_all(Self::response(false).as_bytes());
            return CallbackAttempt::Ignore;
        };
        let mut code = None;
        let mut state = None;
        let mut error = None;
        for (key, value) in url.query_pairs() {
            match key.as_ref() {
                "code" => code = Some(value.into_owned()),
                "state" => state = Some(value.into_owned()),
                "error" => error = Some(value.into_owned()),
                _ => {}
            }
        }
        let accepted = state.as_deref() == Some(expected_state)
            && error.is_none()
            && code.as_deref().is_some_and(|code| !code.trim().is_empty());
        let response = Self::response(accepted);
        let _ = stream.write_all(response.as_bytes());
        if let Some(error) = error {
            if state.as_deref() == Some(expected_state) {
                return CallbackAttempt::Denied(format!("OAuth authorization failed: {error}"));
            }
            return CallbackAttempt::Ignore;
        }
        if state.as_deref() != Some(expected_state) {
            return CallbackAttempt::Ignore;
        }
        match code {
            Some(code) if !code.trim().is_empty() => CallbackAttempt::Code(code),
            _ => CallbackAttempt::Ignore,
        }
    }

    fn reject(stream: &mut TcpStream) -> CallbackAttempt {
        let _ = stream.write_all(Self::response(false).as_bytes());
        CallbackAttempt::Ignore
    }

    fn response(accepted: bool) -> String {
        let (status, body) = if accepted {
            ("200 OK", "Authorization complete. Return to Vmux.")
        } else {
            ("400 Bad Request", "Authorization could not complete.")
        };
        format!(
            "HTTP/1.1 {status}\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    }
}

enum CallbackAttempt {
    Code(String),
    Denied(String),
    Ignore,
}

#[derive(Deserialize)]
struct DeviceAuthorizationResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    verification_uri_complete: Option<String>,
    expires_in: u64,
    interval: Option<u64>,
}

impl Drop for DeviceAuthorizationResponse {
    fn drop(&mut self) {
        self.device_code.zeroize();
        self.user_code.zeroize();
    }
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
    scope: Option<String>,
    token_type: Option<String>,
}

impl Drop for TokenResponse {
    fn drop(&mut self) {
        self.access_token.zeroize();
        if let Some(refresh_token) = &mut self.refresh_token {
            refresh_token.zeroize();
        }
    }
}

#[derive(Deserialize)]
struct OauthError {
    error: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn callback_requires_matching_state() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let sender = std::thread::spawn(move || {
            let mut stream = TcpStream::connect(address).unwrap();
            stream
                .write_all(b"GET /callback?code=secret&state=wrong HTTP/1.1\r\n\r\n")
                .unwrap();
            drop(stream);
            let mut stream = TcpStream::connect(address).unwrap();
            stream
                .write_all(b"GET /callback?code=accepted&state=expected HTTP/1.1\r\n\r\n")
                .unwrap();
        });

        let result = Callback::receive(listener, "expected", &Cancellation::default());
        sender.join().unwrap();

        assert_eq!(result.unwrap(), "accepted");
    }

    #[test]
    fn endpoints_require_https() {
        assert_eq!(
            AuthFlow::https_url("http://example.com/token", "token endpoint").unwrap_err(),
            "OAuth token endpoint must use https"
        );
    }

    #[test]
    fn client_id_is_required() {
        assert_eq!(
            AuthFlow::validate_client_id(" ").unwrap_err(),
            "OAuth provider client ID is not configured"
        );
    }

    #[test]
    fn callback_response_declares_its_complete_body() {
        let response = Callback::response(true);
        let (headers, body) = response.split_once("\r\n\r\n").unwrap();

        assert!(headers.contains(&format!("Content-Length: {}", body.len())));
        assert_eq!(body, "Authorization complete. Return to Vmux.");
    }
}
