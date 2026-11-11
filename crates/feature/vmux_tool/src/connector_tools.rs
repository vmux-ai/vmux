use std::io::{BufRead, BufReader, Read};
use std::time::Duration;

use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use reqwest::blocking::Client;
use serde::Deserialize;
use serde_json::{Value, json};
use vmux_api::protocol::{
    AgentQueryResult, AgentRequest, AgentRequestId, ClientMessage, JsonValue,
};
use vmux_ecs::manifest::FeaturePlugin;
use vmux_ecs::profile::McpCredentials;
use vmux_ecs::service::ServiceRequest;
use vmux_ecs::{ProcessAnchor, ProcessId};

use crate::{
    AddedTool, ToolDispatchSet, ToolQuery, ToolQueryAppExt, ToolQueryMessage, ToolQueryRouteSet,
    ToolStore,
};

#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct ConnectorAccess(pub Vec<String>);

impl ConnectorAccess {
    fn allows(&self, id: &str) -> Result<(), String> {
        if self.0.iter().any(|selected| selected == id) {
            return Ok(());
        }
        Err(format!(
            "Select {id} with /connectors in this conversation first"
        ))
    }
}

pub struct ConnectorToolPlugin;

impl Plugin for ConnectorToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FeaturePlugin::<crate::Feature>::default())
            .add_tool_query::<AgentConnectorTools>()
            .add_tool_query::<AgentConnectorCall>()
            .add_systems(Update, (tools, call).in_set(ToolDispatchSet))
            .add_systems(Update, (list, invoke).after(ToolQueryRouteSet))
            .add_systems(Update, finish);
    }
}

#[vmux_api::agent]
struct AgentConnectorTools {
    anchor: ProcessId,
    connector: Option<String>,
}

#[vmux_api::agent]
struct AgentConnectorCall {
    anchor: ProcessId,
    connector: String,
    name: String,
    arguments: JsonValue,
}

#[crate::input]
#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConnectorToolsTool {
    #[serde(default)]
    connector: Option<String>,
}

#[crate::input]
#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConnectorCallTool {
    connector: String,
    name: String,
    arguments: Value,
}

fn tools(
    requests: Query<
        (Entity, Option<&ProcessAnchor>, &ConnectorToolsTool),
        AddedTool<ConnectorToolsTool>,
    >,
    mut commands: Commands,
) {
    for (entity, anchor, input) in &requests {
        let request = ProcessAnchor::required(anchor, "connector_tools").and_then(|anchor| {
            AgentRequest::encode(&AgentConnectorTools {
                anchor,
                connector: input.connector.clone(),
            })
        });
        commands.entity(entity).insert(ToolQuery(request));
    }
}

fn call(
    requests: Query<
        (Entity, Option<&ProcessAnchor>, &ConnectorCallTool),
        AddedTool<ConnectorCallTool>,
    >,
    mut commands: Commands,
) {
    for (entity, anchor, input) in &requests {
        let request = ProcessAnchor::required(anchor, "connector_call").and_then(|anchor| {
            if !input.arguments.is_object() || input.name.trim().is_empty() {
                return Err("connector_call requires a tool name and object arguments".to_string());
            }
            AgentRequest::encode(&AgentConnectorCall {
                anchor,
                connector: input.connector.clone(),
                name: input.name.clone(),
                arguments: JsonValue::from(input.arguments.clone()),
            })
        });
        commands.entity(entity).insert(ToolQuery(request));
    }
}

#[derive(Component)]
struct RequestTask {
    request_id: AgentRequestId,
    task: Task<Result<String, String>>,
}

fn list(
    mut requests: MessageReader<ToolQueryMessage<AgentConnectorTools>>,
    contexts: Query<(&ProcessId, &ConnectorAccess)>,
    credentials: Query<&McpCredentials>,
    proxy: Option<Res<bevy::winit::EventLoopProxyWrapper>>,
    mut replies: MessageWriter<ServiceRequest>,
    mut commands: Commands,
) {
    for request in requests.read() {
        let access = contexts
            .iter()
            .find(|(anchor, _)| **anchor == request.payload.anchor)
            .map(|(_, access)| access.clone());
        let Some(access) = access else {
            replies.write(ServiceRequest(ClientMessage::AgentQueryResult(
                AgentQueryResult::text(
                    request.request_id,
                    Err("Conversation connector context is unavailable".to_string()),
                ),
            )));
            continue;
        };
        let Some(id) = &request.payload.connector else {
            replies.write(ServiceRequest(ClientMessage::AgentQueryResult(
                AgentQueryResult::text(
                    request.request_id,
                    Ok(json!({"connectors": access.0}).to_string()),
                ),
            )));
            continue;
        };
        let result = access.allows(id).and_then(|()| {
            credentials
                .single()
                .cloned()
                .map_err(|_| "Profile credentials are unavailable".to_string())
        });
        let credentials = match result {
            Ok(credentials) => credentials,
            Err(error) => {
                replies.write(ServiceRequest(ClientMessage::AgentQueryResult(
                    AgentQueryResult::text(request.request_id, Err(error)),
                )));
                continue;
            }
        };
        let id = id.clone();
        let wake = proxy.as_deref().map(|proxy| (**proxy).clone());
        commands.spawn(RequestTask {
            request_id: request.request_id,
            task: IoTaskPool::get().spawn(async move {
                let result = (|| {
                    let mut remote = Remote::connect(&id, &credentials)?;
                    remote.list().map(|value| value.to_string())
                })();
                if let Some(wake) = wake {
                    let _ = wake.send_event(bevy::winit::WinitUserEvent::WakeUp);
                }
                result
            }),
        });
    }
}

fn invoke(
    mut requests: MessageReader<ToolQueryMessage<AgentConnectorCall>>,
    contexts: Query<(&ProcessId, &ConnectorAccess)>,
    credentials: Query<&McpCredentials>,
    proxy: Option<Res<bevy::winit::EventLoopProxyWrapper>>,
    mut replies: MessageWriter<ServiceRequest>,
    mut commands: Commands,
) {
    for request in requests.read() {
        let result = contexts
            .iter()
            .find(|(anchor, _)| **anchor == request.payload.anchor)
            .ok_or_else(|| "Conversation connector context is unavailable".to_string())
            .and_then(|(_, access)| access.allows(&request.payload.connector))
            .and_then(|()| {
                credentials
                    .single()
                    .cloned()
                    .map_err(|_| "Profile credentials are unavailable".to_string())
            });
        let credentials = match result {
            Ok(credentials) => credentials,
            Err(error) => {
                replies.write(ServiceRequest(ClientMessage::AgentQueryResult(
                    AgentQueryResult::text(request.request_id, Err(error)),
                )));
                continue;
            }
        };
        let id = request.payload.connector.clone();
        let name = request.payload.name.clone();
        let arguments =
            Value::try_from(&request.payload.arguments).map_err(|error| error.to_string());
        let wake = proxy.as_deref().map(|proxy| (**proxy).clone());
        commands.spawn(RequestTask {
            request_id: request.request_id,
            task: IoTaskPool::get().spawn(async move {
                let result = (|| {
                    let arguments = arguments?;
                    if !arguments.is_object() || name.trim().is_empty() {
                        return Err(
                            "connector_call requires a tool name and object arguments".to_string()
                        );
                    }
                    let mut remote = Remote::connect(&id, &credentials)?;
                    remote
                        .request("tools/call", json!({"name": name, "arguments": arguments}))
                        .map(|value| value.to_string())
                })();
                if let Some(wake) = wake {
                    let _ = wake.send_event(bevy::winit::WinitUserEvent::WakeUp);
                }
                result
            }),
        });
    }
}

fn finish(
    mut tasks: Query<(Entity, &mut RequestTask)>,
    mut replies: MessageWriter<ServiceRequest>,
    mut commands: Commands,
) {
    for (entity, mut task) in &mut tasks {
        let Some(result) = future::block_on(future::poll_once(&mut task.task)) else {
            continue;
        };
        replies.write(ServiceRequest(ClientMessage::AgentQueryResult(
            AgentQueryResult::text(task.request_id, result),
        )));
        commands.entity(entity).despawn();
    }
}

struct Remote {
    client: Client,
    url: String,
    token: String,
    session: Option<String>,
    sequence: u64,
}

impl Remote {
    fn connect(id: &str, credentials: &McpCredentials) -> Result<Self, String> {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::none())
            .user_agent("Vmux")
            .build()
            .map_err(|error| error.to_string())?;
        let (url, token) = credentials.access.write(|| {
            let manifest = ToolStore::current().load()?;
            let server = manifest
                .mcp
                .servers
                .get(id)
                .ok_or_else(|| format!("Connector {id} is disconnected"))?;
            let url = server
                .url
                .clone()
                .ok_or_else(|| "Connector requires an HTTP endpoint".to_string())?;
            let mut oauth = credentials
                .storage
                .load(id)?
                .ok_or_else(|| format!("Sign in to {id} in Connectors"))?;
            if !oauth.authorizes(&url) {
                return Err("Connector credentials do not authorize this endpoint".to_string());
            }
            if oauth.expires_soon() {
                let refresh = oauth
                    .refresh_token
                    .as_ref()
                    .ok_or_else(|| format!("Sign in to {id} again in Connectors"))?;
                let endpoint =
                    url::Url::parse(&oauth.token_endpoint).map_err(|error| error.to_string())?;
                if endpoint.scheme() != "https" {
                    return Err("OAuth token endpoint must use https".to_string());
                }
                let mut form = vec![
                    ("grant_type", "refresh_token"),
                    ("refresh_token", refresh),
                    ("client_id", &oauth.client_id),
                ];
                if let Some(secret) = &oauth.client_secret {
                    form.push(("client_secret", secret));
                }
                let response = client
                    .post(endpoint)
                    .header("Accept", "application/json")
                    .form(&form)
                    .send()
                    .map_err(|error| error.to_string())?;
                if !response.status().is_success() {
                    return Err(format!("Sign in to {id} again in Connectors"));
                }
                let token: RefreshToken = response.json().map_err(|error| error.to_string())?;
                if token.access_token.is_empty() {
                    return Err("OAuth server returned an empty token".to_string());
                }
                oauth.access_token = token.access_token;
                oauth.expires_at =
                    vmux_ecs::profile::mcp_credentials::McpOauthCredentials::expires_at(
                        token.expires_in,
                    );
                if let Some(refresh) = token.refresh_token {
                    oauth.refresh_token = Some(refresh);
                }
                credentials.storage.store(id, &oauth)?;
            }
            Ok((url, oauth.access_token))
        })?;
        let mut remote = Self {
            client,
            url,
            token,
            session: None,
            sequence: 0,
        };
        let initialized = remote.request("initialize", json!({"protocolVersion": "2025-03-26", "capabilities": {}, "clientInfo": {"name": "vmux", "version": env!("CARGO_PKG_VERSION")}}))?;
        if initialized.get("protocolVersion").and_then(Value::as_str) != Some("2025-03-26") {
            return Err("Connector negotiated an unsupported MCP version".to_string());
        }
        let response =
            remote.post(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}))?;
        if !response.status().is_success() {
            return Err(format!(
                "Connector initialization failed ({})",
                response.status()
            ));
        }
        Ok(remote)
    }

    fn post(&self, body: &Value) -> Result<reqwest::blocking::Response, String> {
        let mut request = self
            .client
            .post(&self.url)
            .bearer_auth(&self.token)
            .header("Accept", "application/json, text/event-stream")
            .header("MCP-Protocol-Version", "2025-03-26")
            .json(body);
        if let Some(session) = &self.session {
            request = request.header("Mcp-Session-Id", session);
        }
        request.send().map_err(|error| error.to_string())
    }

    fn request(&mut self, method: &str, params: Value) -> Result<Value, String> {
        self.sequence += 1;
        let id = self.sequence;
        let response =
            self.post(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))?;
        if !response.status().is_success() {
            return Err(format!("Connector request failed ({})", response.status()));
        }
        if let Some(session) = response.headers().get("Mcp-Session-Id") {
            self.session = Some(
                session
                    .to_str()
                    .map_err(|error| error.to_string())?
                    .to_string(),
            );
        }
        let streaming = response
            .headers()
            .get("Content-Type")
            .and_then(|header| header.to_str().ok())
            .is_some_and(|content| content.starts_with("text/event-stream"));
        Self::decode(response, streaming, id)
    }

    fn decode(reader: impl Read, streaming: bool, id: u64) -> Result<Value, String> {
        let mut reader = BufReader::new(reader.take(8 * 1024 * 1024 + 1));
        if !streaming {
            let mut body = String::new();
            reader
                .read_to_string(&mut body)
                .map_err(|error| error.to_string())?;
            if body.len() > 8 * 1024 * 1024 {
                return Err("Connector response is too large".to_string());
            }
            let response: Value = serde_json::from_str(&body).map_err(|error| error.to_string())?;
            return Self::result(response, id);
        }
        let mut data = String::new();
        let mut length = 0;
        loop {
            let mut line = String::new();
            let read = reader
                .read_line(&mut line)
                .map_err(|error| error.to_string())?;
            length += read;
            if length > 8 * 1024 * 1024 {
                return Err("Connector response is too large".to_string());
            }
            if line.trim_end_matches(['\r', '\n']).is_empty() {
                if !data.is_empty() {
                    let response: Value =
                        serde_json::from_str(&data).map_err(|error| error.to_string())?;
                    if response.get("id") == Some(&json!(id)) {
                        return Self::result(response, id);
                    }
                    data.clear();
                }
            } else if let Some(line) = line.strip_prefix("data:") {
                if !data.is_empty() {
                    data.push('\n');
                }
                data.push_str(
                    line.strip_prefix(' ')
                        .unwrap_or(line)
                        .trim_end_matches(['\r', '\n']),
                );
            }
            if read == 0 {
                break;
            }
        }
        Err("Connector returned no matching response".to_string())
    }

    fn result(response: Value, id: u64) -> Result<Value, String> {
        if response.get("jsonrpc").and_then(Value::as_str) != Some("2.0")
            || response.get("id") != Some(&json!(id))
        {
            return Err("Connector returned a mismatched response".to_string());
        }
        if let Some(error) = response.get("error") {
            return Err(error.to_string());
        }
        response
            .get("result")
            .cloned()
            .ok_or_else(|| "Connector response has no result".to_string())
    }

    fn list(&mut self) -> Result<Value, String> {
        let mut tools = Vec::new();
        let mut cursor = None;
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..64 {
            let params = match &cursor {
                Some(cursor) => json!({"cursor": cursor}),
                None => json!({}),
            };
            let result = self.request("tools/list", params)?;
            let entries = result
                .get("tools")
                .and_then(Value::as_array)
                .ok_or_else(|| "Connector returned an invalid tool list".to_string())?;
            tools.extend(entries.iter().cloned());
            let Some(next) = result.get("nextCursor").and_then(Value::as_str) else {
                return Ok(json!({"tools": tools}));
            };
            if !seen.insert(next.to_string()) {
                return Err("Connector repeated a tool cursor".to_string());
            }
            cursor = Some(next.to_string());
        }
        Err("Connector tool list exceeded the page limit".to_string())
    }
}

#[derive(Deserialize)]
struct RefreshToken {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::net::TcpListener;
    use std::sync::mpsc;

    #[test]
    fn connector_queries_reject_accounts_not_selected_by_the_calling_conversation() {
        let mut app = App::new();
        app.add_plugins(ConnectorToolPlugin);
        let anchor = ProcessId::new();
        app.world_mut()
            .spawn((anchor, ConnectorAccess(vec!["linear".into()])));
        for id in [None, Some("github".to_string())] {
            app.world_mut().write_message(crate::ToolQueryRequest {
                request_id: AgentRequestId::new(),
                query: AgentRequest::encode(&AgentConnectorTools {
                    anchor,
                    connector: id,
                })
                .unwrap(),
            });
        }
        app.update();
        let replies: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<ServiceRequest>>()
            .drain()
            .collect();
        assert_eq!(replies.len(), 2);
        let mut success = 0;
        let mut refused = 0;
        for reply in replies {
            let ClientMessage::AgentQueryResult(result) = reply.0 else {
                panic!("unexpected response");
            };
            if result.is_error {
                assert!(result.content.contains("Select github"));
                refused += 1;
            } else {
                assert_eq!(
                    serde_json::from_str::<Value>(&result.content).unwrap(),
                    json!({"connectors": ["linear"]})
                );
                success += 1;
            }
        }
        assert_eq!((success, refused), (1, 1));
    }

    #[test]
    fn remote_returns_the_matching_sse_result_without_waiting_for_the_stream_to_close() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/mcp", listener.local_addr().unwrap());
        let (release, wait) = mpsc::channel();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut reader = BufReader::new(&mut stream);
            let mut content_length = 0;
            let mut authenticated = false;
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                let lower = line.to_ascii_lowercase();
                if let Some(value) = lower.strip_prefix("content-length:") {
                    content_length = value.trim().parse::<usize>().unwrap();
                }
                if lower.starts_with("authorization:") {
                    authenticated = line.contains("fixture-token");
                }
                if line == "\r\n" {
                    break;
                }
            }
            let mut body = vec![0; content_length];
            reader.read_exact(&mut body).unwrap();
            assert!(authenticated);
            let request: Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(request["method"], "tools/call");
            assert_eq!(
                request["params"],
                json!({"name": "get_me", "arguments": {}})
            );
            stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nMcp-Session-Id: fixture-session\r\nConnection: close\r\n\r\n").unwrap();
            stream
                .write_all(
                    b"data: {\"jsonrpc\":\"2.0\",\"method\":\"notifications/progress\"}\r\n\r\n",
                )
                .unwrap();
            let result = json!({"jsonrpc": "2.0", "id": request["id"], "result": {"content": [{"type": "text", "text": "fixture-account"}]}});
            write!(stream, "event: message\r\ndata: {result}\r\n\r\n").unwrap();
            stream.flush().unwrap();
            wait.recv_timeout(Duration::from_secs(2)).unwrap();
        });
        let mut remote = Remote {
            client: Client::builder()
                .timeout(Duration::from_secs(3))
                .build()
                .unwrap(),
            url,
            token: "fixture-token".into(),
            session: None,
            sequence: 0,
        };
        let result = remote
            .request("tools/call", json!({"name": "get_me", "arguments": {}}))
            .unwrap();
        assert_eq!(result["content"][0]["text"], "fixture-account");
        assert_eq!(remote.session.as_deref(), Some("fixture-session"));
        release.send(()).unwrap();
        server.join().unwrap();
    }

    #[test]
    fn remote_rejects_mismatched_responses_and_preserves_tool_errors() {
        let mismatched = br#"{"jsonrpc":"2.0","id":2,"result":{}}"#;
        assert!(Remote::decode(mismatched.as_slice(), false, 1).is_err());
        let failed =
            br#"{"jsonrpc":"2.0","id":1,"error":{"code":-32601,"message":"unknown tool"}}"#;
        assert!(
            Remote::decode(failed.as_slice(), false, 1)
                .unwrap_err()
                .contains("unknown tool")
        );
        let tool_error = br#"{"jsonrpc":"2.0","id":1,"result":{"isError":true,"content":[{"type":"text","text":"permission denied"}]}}"#;
        let result = Remote::decode(tool_error.as_slice(), false, 1).unwrap();
        assert_eq!(result["isError"], true);
        assert_eq!(result["content"][0]["text"], "permission denied");
    }
}
