use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use reqwest::blocking::{Client, Response};
use serde_json::{Value, json};
use url::Url;
use vmux_profile::{Profile, ProfilePaths};

use crate::mcp_config::{McpAuthSpec, McpResolvedServer, McpServerSpec};
use crate::mcp_oauth::McpOauthFlow;

pub struct McpProxy {
    server: McpResolvedServer,
    oauth: Option<McpOauthFlow>,
    client: Client,
    workspace_root: Option<PathBuf>,
    session_id: Option<String>,
}

impl McpProxy {
    pub fn new(
        server: McpResolvedServer,
        profile: Profile,
        paths: ProfilePaths,
        workspace_root: Option<PathBuf>,
    ) -> Result<Self, String> {
        let client = Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .map_err(|error| error.to_string())?;
        let oauth = if server.spec.oauth_config().is_some() {
            Some(McpOauthFlow::for_profile(profile, paths)?)
        } else {
            None
        };
        Ok(Self {
            server,
            oauth,
            client,
            workspace_root,
            session_id: None,
        })
    }

    pub fn run_stdio(mut self) -> Result<(), String> {
        if !self.server.enabled {
            return Err(format!("MCP server \"{}\" is disabled", self.server.id));
        }
        if !self.server.trusted {
            return Err(format!(
                "MCP server \"{}\" is untrusted; run `vmux tools mcp trust {}`",
                self.server.id, self.server.id
            ));
        }
        match self.server.spec.clone() {
            McpServerSpec::Stdio {
                command,
                args,
                env,
                cwd,
                ..
            } => self.relay_stdio(&command, &args, &env, cwd.as_deref()),
            McpServerSpec::Http {
                url,
                headers,
                header_env,
                bearer_token_env_var,
                auth,
                ..
            } => {
                let stdin = std::io::stdin();
                let stdout = std::io::stdout();
                let mut reader = stdin.lock();
                let mut writer = stdout.lock();
                self.relay_http(
                    &url,
                    &headers,
                    &header_env,
                    bearer_token_env_var.as_deref(),
                    &auth,
                    &mut reader,
                    &mut writer,
                )
            }
            McpServerSpec::Sse {
                url,
                headers,
                header_env,
                bearer_token_env_var,
                auth,
                ..
            } => {
                let stdin = std::io::stdin();
                let mut reader = stdin.lock();
                let writer = Arc::new(Mutex::new(std::io::stdout()));
                self.relay_sse(
                    &url,
                    &headers,
                    &header_env,
                    bearer_token_env_var.as_deref(),
                    &auth,
                    &mut reader,
                    writer,
                )
            }
        }
    }

    fn resolve_cwd(&self, cwd: Option<&str>) -> Option<PathBuf> {
        let raw = cwd?.trim();
        if raw.is_empty() {
            return None;
        }
        let path = PathBuf::from(raw);
        if path.is_absolute() {
            Some(path)
        } else if let Some(root) = &self.workspace_root {
            Some(root.join(path))
        } else {
            Some(path)
        }
    }

    fn relay_stdio(
        &self,
        command: &str,
        args: &[String],
        env: &BTreeMap<String, String>,
        cwd: Option<&str>,
    ) -> Result<(), String> {
        let mut cmd = Command::new(command);
        cmd.args(args)
            .envs(env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        if let Some(dir) = self.resolve_cwd(cwd) {
            cmd.current_dir(dir);
        }
        let mut child = cmd
            .spawn()
            .map_err(|error| format!("failed to spawn MCP server {command}: {error}"))?;
        let mut child_stdin = child.stdin.take().ok_or("child stdin unavailable")?;
        let mut child_stdout = child.stdout.take().ok_or("child stdout unavailable")?;
        let input_thread = std::thread::spawn(move || {
            let mut stdin = std::io::stdin().lock();
            let _ = std::io::copy(&mut stdin, &mut child_stdin);
            let _ = child_stdin.flush();
        });
        let copy_result = {
            let mut stdout = std::io::stdout().lock();
            std::io::copy(&mut child_stdout, &mut stdout).and_then(|_| stdout.flush())
        };
        let _ = child.kill();
        let _ = child.wait();
        drop(input_thread);
        copy_result.map_err(|error| error.to_string())
    }

    #[allow(clippy::too_many_arguments)]
    fn relay_http(
        &mut self,
        url: &str,
        headers: &BTreeMap<String, String>,
        header_env: &BTreeMap<String, String>,
        bearer_env: Option<&str>,
        auth: &McpAuthSpec,
        reader: &mut dyn BufRead,
        writer: &mut dyn Write,
    ) -> Result<(), String> {
        let mut line = String::new();
        loop {
            line.clear();
            let count = reader
                .read_line(&mut line)
                .map_err(|error| error.to_string())?;
            if count == 0 {
                return Ok(());
            }
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let message: Value = match serde_json::from_str(trimmed) {
                Ok(value) => value,
                Err(error) => {
                    Self::write_error(writer, None, format!("invalid JSON-RPC input: {error}"))?;
                    continue;
                }
            };
            let request_id = message.get("id").cloned();
            match self.post_http(url, url, headers, header_env, bearer_env, auth, trimmed) {
                Ok(frames) => {
                    for frame in frames {
                        Self::write_frame(writer, &frame)?;
                    }
                }
                Err(error) => {
                    if let Some(id) = request_id {
                        Self::write_error(writer, Some(id), error)?;
                    }
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn post_http(
        &mut self,
        post_url: &str,
        resource_url: &str,
        headers: &BTreeMap<String, String>,
        header_env: &BTreeMap<String, String>,
        bearer_env: Option<&str>,
        auth: &McpAuthSpec,
        body: &str,
    ) -> Result<Vec<Value>, String> {
        let is_oauth = matches!(auth, McpAuthSpec::OAuth { .. });
        let response = self.send_http_once(
            post_url,
            resource_url,
            headers,
            header_env,
            bearer_env,
            is_oauth,
            false,
            body,
        )?;
        let response = if response.status() == reqwest::StatusCode::UNAUTHORIZED && is_oauth {
            self.send_http_once(
                post_url,
                resource_url,
                headers,
                header_env,
                bearer_env,
                true,
                true,
                body,
            )?
        } else {
            response
        };
        if let Some(session) = response
            .headers()
            .get("mcp-session-id")
            .and_then(|value| value.to_str().ok())
        {
            self.session_id = Some(session.to_string());
        }
        let status = response.status();
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_ascii_lowercase();
        let text = response.text().map_err(|error| error.to_string())?;
        if !status.is_success() {
            return Err(format!("MCP HTTP error ({status}): {text}"));
        }
        if status == reqwest::StatusCode::ACCEPTED || text.trim().is_empty() {
            return Ok(Vec::new());
        }
        if content_type.contains("text/event-stream") {
            Self::parse_sse_frames(&text)
        } else {
            let value: Value = serde_json::from_str(&text)
                .map_err(|error| format!("invalid MCP JSON response: {error}"))?;
            Ok(vec![value])
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn send_http_once(
        &self,
        post_url: &str,
        resource_url: &str,
        headers: &BTreeMap<String, String>,
        header_env: &BTreeMap<String, String>,
        bearer_env: Option<&str>,
        is_oauth: bool,
        force_refresh: bool,
        body: &str,
    ) -> Result<Response, String> {
        let mut request = self
            .client
            .post(post_url)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .header(
                reqwest::header::ACCEPT,
                "application/json, text/event-stream",
            );
        if let Some(session_id) = &self.session_id {
            request = request.header("Mcp-Session-Id", session_id);
        }
        for (name, value) in self.build_headers(
            resource_url,
            headers,
            header_env,
            bearer_env,
            is_oauth,
            force_refresh,
        )? {
            request = request.header(name, value);
        }
        request
            .body(body.to_string())
            .send()
            .map_err(|error| error.to_string())
    }

    #[allow(clippy::too_many_arguments)]
    fn open_sse_stream(
        &self,
        stream_client: &Client,
        url: &str,
        headers: &BTreeMap<String, String>,
        header_env: &BTreeMap<String, String>,
        bearer_env: Option<&str>,
        is_oauth: bool,
        force_refresh: bool,
    ) -> Result<Response, String> {
        let mut get = stream_client
            .get(url)
            .header(reqwest::header::ACCEPT, "text/event-stream");
        for (name, value) in self.build_headers(
            url,
            headers,
            header_env,
            bearer_env,
            is_oauth,
            force_refresh,
        )? {
            get = get.header(name, value);
        }
        get.send().map_err(|error| error.to_string())
    }

    #[allow(clippy::too_many_arguments)]
    fn relay_sse<W: Write + Send + 'static>(
        &mut self,
        url: &str,
        headers: &BTreeMap<String, String>,
        header_env: &BTreeMap<String, String>,
        bearer_env: Option<&str>,
        auth: &McpAuthSpec,
        reader: &mut dyn BufRead,
        writer: Arc<Mutex<W>>,
    ) -> Result<(), String> {
        let is_oauth = matches!(auth, McpAuthSpec::OAuth { .. });
        let stream_client = Client::builder()
            .build()
            .map_err(|error| error.to_string())?;
        let response = self.open_sse_stream(
            &stream_client,
            url,
            headers,
            header_env,
            bearer_env,
            is_oauth,
            false,
        )?;
        let response = if response.status() == reqwest::StatusCode::UNAUTHORIZED && is_oauth {
            self.open_sse_stream(
                &stream_client,
                url,
                headers,
                header_env,
                bearer_env,
                true,
                true,
            )?
        } else {
            response
        };
        if !response.status().is_success() {
            return Err(format!("MCP SSE stream failed ({})", response.status()));
        }
        let base_url = Url::parse(url).map_err(|error| error.to_string())?;
        let mut sse_reader = BufReader::new(response);
        let endpoint = Self::read_sse_endpoint(&mut sse_reader, &base_url)?;
        if endpoint.origin() != base_url.origin() {
            return Err("MCP SSE endpoint origin does not match server URL".to_string());
        }
        let sink = Arc::clone(&writer);
        let sse_thread = std::thread::spawn(move || {
            Self::pump_sse_messages(&mut sse_reader, &sink);
        });
        let mut line = String::new();
        loop {
            line.clear();
            let count = reader
                .read_line(&mut line)
                .map_err(|error| error.to_string())?;
            if count == 0 {
                break;
            }
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let message: Value = match serde_json::from_str(trimmed) {
                Ok(value) => value,
                Err(error) => {
                    let mut out = writer.lock().unwrap();
                    Self::write_error(&mut *out, None, format!("invalid JSON-RPC input: {error}"))?;
                    continue;
                }
            };
            let request_id = message.get("id").cloned();
            match self.post_http(
                endpoint.as_str(),
                url,
                headers,
                header_env,
                bearer_env,
                auth,
                trimmed,
            ) {
                Ok(frames) => {
                    if !frames.is_empty() {
                        let mut out = writer.lock().unwrap();
                        for frame in frames {
                            Self::write_frame(&mut *out, &frame)?;
                        }
                    }
                }
                Err(error) => {
                    if let Some(id) = request_id {
                        let mut out = writer.lock().unwrap();
                        Self::write_error(&mut *out, Some(id), error)?;
                    }
                }
            }
        }
        drop(sse_thread);
        Ok(())
    }

    fn read_sse_endpoint(reader: &mut impl BufRead, base_url: &Url) -> Result<Url, String> {
        let mut event = String::new();
        let mut data = String::new();
        let mut line = String::new();
        loop {
            line.clear();
            if reader
                .read_line(&mut line)
                .map_err(|error| error.to_string())?
                == 0
            {
                return Err("MCP SSE stream closed before sending endpoint event".to_string());
            }
            let trimmed = line.trim_end_matches(['\r', '\n']);
            if trimmed.is_empty() {
                if event == "endpoint" && !data.trim().is_empty() {
                    return base_url
                        .join(data.trim())
                        .map_err(|error| format!("invalid SSE endpoint: {error}"));
                }
                event.clear();
                data.clear();
                continue;
            }
            if let Some(value) = trimmed.strip_prefix("event:") {
                event = value.trim().to_string();
            } else if let Some(value) = trimmed.strip_prefix("data:") {
                if !data.is_empty() {
                    data.push('\n');
                }
                data.push_str(value.trim());
            }
        }
    }

    fn pump_sse_messages<W: Write>(reader: &mut impl BufRead, sink: &Arc<Mutex<W>>) {
        let mut data = String::new();
        let mut line = String::new();
        loop {
            line.clear();
            let Ok(count) = reader.read_line(&mut line) else {
                return;
            };
            if count == 0 {
                return;
            }
            let trimmed = line.trim_end_matches(['\r', '\n']);
            if trimmed.is_empty() {
                if let Ok(value) = serde_json::from_str::<Value>(data.trim()) {
                    let Ok(mut out) = sink.lock() else {
                        return;
                    };
                    if Self::write_frame(&mut *out, &value).is_err() {
                        return;
                    }
                }
                data.clear();
                continue;
            }
            if let Some(value) = trimmed.strip_prefix("data:") {
                if !data.is_empty() {
                    data.push('\n');
                }
                data.push_str(value.trim());
            }
        }
    }

    fn build_headers(
        &self,
        url: &str,
        headers: &BTreeMap<String, String>,
        header_env: &BTreeMap<String, String>,
        bearer_env: Option<&str>,
        is_oauth: bool,
        force_refresh: bool,
    ) -> Result<Vec<(String, String)>, String> {
        let mut resolved = headers.clone();
        for (name, variable) in header_env {
            if let Ok(value) = std::env::var(variable) {
                resolved.insert(name.clone(), value);
            }
        }
        if let Some(variable) = bearer_env
            && let Ok(value) = std::env::var(variable)
        {
            resolved.insert("Authorization".to_string(), format!("Bearer {value}"));
        }
        if is_oauth {
            let oauth = self
                .oauth
                .as_ref()
                .ok_or_else(|| "OAuth flow is unavailable".to_string())?;
            let token = oauth.access_token(&self.server.id, url, force_refresh)?;
            resolved.insert("Authorization".to_string(), format!("Bearer {token}"));
        }
        Ok(resolved.into_iter().collect())
    }

    fn parse_sse_frames(body: &str) -> Result<Vec<Value>, String> {
        let mut frames = Vec::new();
        let mut data = String::new();
        for raw_line in body.lines() {
            let line = raw_line.trim_end_matches('\r');
            if line.is_empty() {
                if !data.trim().is_empty() {
                    let value: Value = serde_json::from_str(data.trim())
                        .map_err(|error| format!("invalid SSE JSON frame: {error}"))?;
                    frames.push(value);
                    data.clear();
                }
                continue;
            }
            if let Some(payload) = line.strip_prefix("data:") {
                if !data.is_empty() {
                    data.push('\n');
                }
                data.push_str(payload.trim());
            }
        }
        if !data.trim().is_empty() {
            let value: Value = serde_json::from_str(data.trim())
                .map_err(|error| format!("invalid SSE JSON frame: {error}"))?;
            frames.push(value);
        }
        Ok(frames)
    }

    fn write_frame(writer: &mut dyn Write, value: &Value) -> Result<(), String> {
        serde_json::to_writer(&mut *writer, value).map_err(|error| error.to_string())?;
        writer.write_all(b"\n").map_err(|error| error.to_string())?;
        writer.flush().map_err(|error| error.to_string())
    }

    fn write_error(
        writer: &mut dyn Write,
        id: Option<Value>,
        message: String,
    ) -> Result<(), String> {
        let payload = json!({
            "jsonrpc": "2.0",
            "id": id.unwrap_or(Value::Null),
            "error": {
                "code": -32000,
                "message": message,
            }
        });
        Self::write_frame(writer, &payload)
    }

    pub fn resolve_server(
        config_dir: &Path,
        profile_dir: &Path,
        cwd: Option<&Path>,
        server_id: &str,
    ) -> Result<(McpResolvedServer, Option<PathBuf>), String> {
        let workspace = cwd.and_then(crate::McpWorkspaceRoot::resolve);
        let mut input = crate::McpLayerInput::default();
        if let Ok(text) =
            crate::io::fs::LocalFs::read_to_string(&config_dir.join("tools").join("tools.toml"))
            && let Ok(manifest) = toml::from_str::<crate::ToolsManifest>(&text)
        {
            input.legacy = crate::McpConfigFile::from_legacy(&manifest.mcp);
        }
        if let Ok(text) = crate::io::fs::LocalFs::read_to_string(&config_dir.join("mcp.ron")) {
            input.global = crate::McpConfigFile::parse(&text)?;
        }
        if profile_dir != config_dir
            && let Ok(text) = crate::io::fs::LocalFs::read_to_string(&profile_dir.join("mcp.ron"))
        {
            input.profile = crate::McpConfigFile::parse(&text)?;
        }
        if let Ok(text) = crate::io::fs::LocalFs::read_to_string(&profile_dir.join("mcp-trust.ron"))
        {
            input.trust = crate::McpTrustStore::parse(&text)?;
        }
        let checkout_root = if let Some(workspace) = &workspace {
            input.repo_root = Some(workspace.primary_root.to_string_lossy().into_owned());
            if let Ok(text) = crate::io::fs::LocalFs::read_to_string(
                &workspace.checkout_root.join(".vmux/mcp.ron"),
            ) {
                input.project = crate::McpConfigFile::parse(&text)?;
            }
            if let Ok(text) = crate::io::fs::LocalFs::read_to_string(
                &workspace.primary_root.join(".vmux/mcp.local.ron"),
            ) {
                input.primary_local = crate::McpConfigFile::parse(&text)?;
            }
            if workspace.checkout_root != workspace.primary_root
                && let Ok(text) = crate::io::fs::LocalFs::read_to_string(
                    &workspace.checkout_root.join(".vmux/mcp.local.ron"),
                )
            {
                input.worktree_local = crate::McpConfigFile::parse(&text)?;
            }
            Some(workspace.checkout_root.clone())
        } else {
            None
        };
        let server = input
            .resolve()
            .into_iter()
            .find(|candidate| candidate.id == server_id)
            .ok_or_else(|| format!("MCP server not found: {server_id}"))?;
        Ok((server, checkout_root))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp_config::McpConfigScope;
    use std::io::{Cursor, Read};
    use std::net::TcpListener;
    use vmux_profile::mcp_credentials::McpOauthCredentials;

    #[test]
    fn http_proxy_refreshes_oauth_token_on_401_and_decodes_sse_response() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let base = format!("http://127.0.0.1:{port}");
        let server_thread = std::thread::spawn(move || {
            for step in 0..3 {
                let (mut stream, _) = listener.accept().unwrap();
                let mut buf = [0u8; 4096];
                let count = stream.read(&mut buf).unwrap();
                let request = String::from_utf8_lossy(&buf[..count]);
                let response = match step {
                    0 => {
                        assert!(request.contains("Bearer expired-token"));
                        "HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                            .to_string()
                    }
                    1 => {
                        assert!(request.contains("refresh-tok-1"));
                        let body = r#"{"access_token":"fresh-token","refresh_token":"refresh-tok-2","expires_in":3600}"#;
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        )
                    }
                    _ => {
                        assert!(request.contains("Bearer fresh-token"));
                        let body = "event: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"tools\":[]}}\n\n";
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nMcp-Session-Id: sess-42\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        )
                    }
                };
                stream.write_all(response.as_bytes()).unwrap();
            }
        });

        let dir = tempfile::tempdir().unwrap();
        let url = format!("{base}/mcp");
        let oauth = McpOauthFlow::for_loopback_test(dir.path().to_path_buf(), "test".into());
        oauth
            .store_credentials(
                "linear-vmx",
                &McpOauthCredentials {
                    token_endpoint: format!("{base}/token"),
                    client_id: "cid".into(),
                    client_secret: None,
                    access_token: "expired-token".into(),
                    refresh_token: Some("refresh-tok-1".into()),
                    expires_at: McpOauthCredentials::expires_at(Some(3600)),
                    scope: "read write".into(),
                    resource: url.clone(),
                },
            )
            .unwrap();

        let server = McpResolvedServer {
            id: "linear-vmx".into(),
            spec: McpServerSpec::Http {
                url: url.clone(),
                headers: BTreeMap::new(),
                header_env: BTreeMap::new(),
                bearer_token_env_var: None,
                auth: McpAuthSpec::OAuth {
                    scopes: vec!["read".into()],
                },
                enabled: true,
                agents: Vec::new(),
            },
            source: McpConfigScope::Global,
            enabled: true,
            trusted: true,
            repo_root: None,
            digest: None,
        };
        let mut proxy = McpProxy {
            server,
            oauth: Some(oauth),
            client: Client::builder().build().unwrap(),
            workspace_root: None,
            session_id: None,
        };
        let mut input = Cursor::new(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/list\"}\n");
        let mut output = Vec::new();
        proxy
            .relay_http(
                &url,
                &BTreeMap::new(),
                &BTreeMap::new(),
                None,
                &McpAuthSpec::OAuth {
                    scopes: vec!["read".into()],
                },
                &mut input,
                &mut output,
            )
            .unwrap();
        let line = String::from_utf8(output).unwrap();
        let decoded: Value = serde_json::from_str(line.trim()).unwrap();
        assert_eq!(decoded["id"], 1);
        assert_eq!(decoded["result"]["tools"], json!([]));
        assert_eq!(proxy.session_id.as_deref(), Some("sess-42"));
        assert_eq!(
            proxy
                .oauth
                .as_ref()
                .unwrap()
                .load_credentials("linear-vmx")
                .unwrap()
                .unwrap()
                .access_token,
            "fresh-token"
        );
        server_thread.join().unwrap();
    }

    #[test]
    fn sse_proxy_streams_async_response_with_oauth_resource_url_and_rejects_cross_origin() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let base = format!("http://127.0.0.1:{port}");
        let (done_tx, done_rx) = std::sync::mpsc::channel::<()>();
        let server_thread = std::thread::spawn(move || {
            let (mut sse_stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 4096];
            let count = sse_stream.read(&mut buf).unwrap();
            let req = String::from_utf8_lossy(&buf[..count]);
            assert!(req.contains("GET /sse"));
            assert!(req.contains("Bearer sse-tok"));
            sse_stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\n\r\n",
                )
                .unwrap();
            let ep = "event: endpoint\ndata: /messages?session=1\n\n";
            let chunk = format!("{:x}\r\n{ep}\r\n", ep.len());
            sse_stream.write_all(chunk.as_bytes()).unwrap();
            sse_stream.flush().unwrap();

            let (mut post_stream, _) = listener.accept().unwrap();
            let mut post_buf = [0u8; 4096];
            let post_count = post_stream.read(&mut post_buf).unwrap();
            let post_req = String::from_utf8_lossy(&post_buf[..post_count]);
            assert!(post_req.contains("POST /messages?session=1"));
            assert!(post_req.contains("Bearer sse-tok"));
            post_stream
                .write_all(
                    b"HTTP/1.1 202 Accepted\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .unwrap();

            let msg =
                "event: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":7,\"result\":{\"ok\":true}}\n\n";
            let msg_chunk = format!("{:x}\r\n{msg}\r\n0\r\n\r\n", msg.len());
            sse_stream.write_all(msg_chunk.as_bytes()).unwrap();
            sse_stream.flush().unwrap();
            let _ = done_rx.recv();
        });

        let dir = tempfile::tempdir().unwrap();
        let sse_url = format!("{base}/sse");
        let oauth = McpOauthFlow::for_loopback_test(dir.path().to_path_buf(), "test".into());
        oauth
            .store_credentials(
                "sse-srv",
                &McpOauthCredentials {
                    token_endpoint: format!("{base}/token"),
                    client_id: "cid".into(),
                    client_secret: None,
                    access_token: "sse-tok".into(),
                    refresh_token: Some("rt".into()),
                    expires_at: McpOauthCredentials::expires_at(Some(3600)),
                    scope: String::new(),
                    resource: sse_url.clone(),
                },
            )
            .unwrap();

        let server = McpResolvedServer {
            id: "sse-srv".into(),
            spec: McpServerSpec::Sse {
                url: sse_url.clone(),
                headers: BTreeMap::new(),
                header_env: BTreeMap::new(),
                bearer_token_env_var: None,
                auth: McpAuthSpec::OAuth { scopes: Vec::new() },
                enabled: true,
                agents: Vec::new(),
            },
            source: McpConfigScope::Global,
            enabled: true,
            trusted: true,
            repo_root: None,
            digest: None,
        };
        let mut proxy = McpProxy {
            server,
            oauth: Some(oauth),
            client: Client::builder().build().unwrap(),
            workspace_root: None,
            session_id: None,
        };
        let mut input = Cursor::new(b"{\"jsonrpc\":\"2.0\",\"id\":7,\"method\":\"ping\"}\n");
        let output = Arc::new(Mutex::new(Vec::<u8>::new()));
        proxy
            .relay_sse(
                &sse_url,
                &BTreeMap::new(),
                &BTreeMap::new(),
                None,
                &McpAuthSpec::OAuth { scopes: Vec::new() },
                &mut input,
                Arc::clone(&output),
            )
            .unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while output.lock().unwrap().is_empty() && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        let _ = done_tx.send(());
        let text = String::from_utf8(output.lock().unwrap().clone()).unwrap();
        let decoded: Value = serde_json::from_str(text.trim()).unwrap();
        assert_eq!(decoded["id"], 7);
        assert_eq!(decoded["result"]["ok"], true);
        server_thread.join().unwrap();

        let bad_listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let bad_port = bad_listener.local_addr().unwrap().port();
        let bad_url = format!("http://127.0.0.1:{bad_port}/sse");
        let bad_thread = std::thread::spawn(move || {
            let (mut stream, _) = bad_listener.accept().unwrap();
            let mut buf = [0u8; 4096];
            let _ = stream.read(&mut buf);
            let body = "event: endpoint\ndata: https://evil.example/steal\n\n";
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(resp.as_bytes());
        });
        let mut empty_in = Cursor::new(b"");
        let out2 = Arc::new(Mutex::new(Vec::<u8>::new()));
        let err = proxy
            .relay_sse(
                &bad_url,
                &BTreeMap::new(),
                &BTreeMap::new(),
                None,
                &McpAuthSpec::None,
                &mut empty_in,
                out2,
            )
            .unwrap_err();
        assert!(err.contains("origin"));
        bad_thread.join().unwrap();
    }
}
