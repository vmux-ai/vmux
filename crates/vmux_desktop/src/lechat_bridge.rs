//! Host-MCP bridge for le-chat (chat.mistral.ai).
//!
//! Injects `window.__LE_CHAT_MCP__` into the chat page on navigation and answers
//! the page's JSON-RPC-ish calls over CEF IPC:
//!   - `listTools` returns vmux's real MCP tool definitions.
//!   - `callTool` executes the tool against the running workspace via the in-app
//!     `ServiceClient` and returns the MCP `{ content, isError? }` result.
//!
//! Page -> Rust:  `cef.emit({ channel, id, method, params })` is delivered to the
//! `on_bridge_request` observer via `Receive<ChatBridgeRequest>`.
//! Rust -> page:  `Browsers::execute_js` calls `window.__LE_CHAT_MCP__.__deliver(msg)`.
//!
//! ## callTool execution + correlation
//! Bevy observers are sync, but a tool's result arrives later (the service routes
//! it back asynchronously). So `callTool` is split across two systems:
//!   1. `on_bridge_request` parses `{ name, arguments }`, builds the
//!      `AgentCommand`/`AgentQuery` via `vmux_mcp::tools::dispatch_with_anchor`,
//!      sends it on the shared `ServiceClient` with a fresh `request_id`, and
//!      records `request_id -> { webview, bridge_id }` in `PendingBridgeCalls`.
//!   2. `deliver_bridge_results` reads `AgentCommandResultEvent` /
//!      `AgentQueryResultEvent` (forwarded from the service drain), matches the
//!      `request_id`, maps the result to MCP `{ content, isError? }` (reusing the
//!      `vmux_mcp::protocol` mappers), and `execute_js`-delivers it to the page.
//!
//! Anchor: the chat webview has no `ProcessId`/`AgentSession`, so it can't act as
//! an agent self-anchor. Pane-targeting tools (`open_page`, `open_file`, `run`)
//! therefore fail fast in `dispatch_with_anchor` and return a clean MCP error for
//! that tool only; every non-anchor tool (`get_settings`, `list_spaces`,
//! `screenshot`, `read_layout`, `read_terminal`, `update_layout`,
//! `browser_navigate`, settings/space/app commands, ...) executes end-to-end.

use std::collections::HashMap;

use bevy::prelude::*;
use bevy_cef::prelude::{Browsers, JsEmitEventPlugin, Receive, WebviewCommittedNavigationEvent};
use vmux_service::agent_events::{AgentCommandResultEvent, AgentQueryResultEvent};
use vmux_service::client::ServiceClient;
use vmux_service::protocol::{AgentRequestId, ClientMessage};

pub struct LeChatBridgePlugin;

impl Plugin for LeChatBridgePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(JsEmitEventPlugin::<ChatBridgeRequest>::default())
            .init_resource::<PendingBridgeCalls>()
            .add_observer(on_bridge_request)
            .add_systems(Update, (inject_shim_on_chat_nav, deliver_bridge_results));
    }
}

/// Distinctive shape so `receive_events::<ChatBridgeRequest>` only fires for our
/// payloads (every emitted JS object is attempted against every registered type).
#[derive(serde::Deserialize)]
struct ChatBridgeRequest {
    channel: String,
    id: String,
    method: String,
    #[serde(default)]
    params: serde_json::Value,
}

/// A `callTool` request awaiting its async service result. Keyed by the
/// `AgentRequestId` sent to the service; carries what we need to deliver the
/// result back to the right page promise.
struct PendingBridgeCall {
    webview: Entity,
    /// The page-side promise id (`msg.id` the shim's `__deliver` matches on).
    bridge_id: String,
}

#[derive(Resource, Default)]
struct PendingBridgeCalls(HashMap<AgentRequestId, PendingBridgeCall>);

const BRIDGE_CHANNEL: &str = "le-chat-mcp";

/// Authorities allowed to receive the bridge shim. Mirrors the CEF IPC gate in
/// `bevy_cef_core` (`is_bridge_allowed_origin`); kept as a small local dup to
/// avoid depending on bevy_cef_core internals.
fn is_chat_origin(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else {
        return false;
    };
    // Authority ends at the first '/', '?' or '#'.
    let authority = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    authority == "chat.mistral.ai" || authority == "chat.local.mistral.ai:8443"
}

const SHIM_JS: &str = r#"(function(){
  if (window.__LE_CHAT_MCP__) return;
  var pending = new Map(); var seq = 0;
  function call(method, params){
    var id = "lcm-" + (seq++);
    return new Promise(function(resolve, reject){
      pending.set(id, {resolve: resolve, reject: reject});
      cef.emit({ channel: "le-chat-mcp", id: id, method: method, params: params || {} });
    });
  }
  window.__LE_CHAT_MCP__ = {
    protocolVersion: "1.0.0",
    listTools: function(){ return call("listTools", {}); },
    callTool: function(name, args){ return call("callTool", { name: name, arguments: args }); },
    __deliver: function(msg){ var p = pending.get(msg.id); if(!p) return; pending.delete(msg.id); if (msg.error) { p.reject(new Error(msg.error)); } else { p.resolve(msg.result); } }
  };
})();"#;

fn inject_shim_on_chat_nav(
    mut events: MessageReader<WebviewCommittedNavigationEvent>,
    browsers: NonSend<Browsers>,
) {
    for ev in events.read() {
        if ev.is_main_frame && is_chat_origin(&ev.url) {
            browsers.execute_js(&ev.webview, SHIM_JS);
        }
    }
}

/// Build the `__deliver` payload for a successful tool result.
fn delivery_result(bridge_id: &str, result: serde_json::Value) -> serde_json::Value {
    serde_json::json!({ "id": bridge_id, "result": result })
}

/// Build the `__deliver` payload for a transport-level error (unknown method,
/// malformed request). Tool *execution* failures are delivered as a resolved
/// MCP error result (`isError: true`) instead, so the page block submits a clean
/// tool result rather than rejecting.
fn delivery_error(bridge_id: &str, message: String) -> serde_json::Value {
    serde_json::json!({ "id": bridge_id, "error": message })
}

fn execute_js_deliver(browsers: &Browsers, webview: &Entity, payload: &serde_json::Value) {
    browsers.execute_js(
        webview,
        &format!(
            "window.__LE_CHAT_MCP__&&window.__LE_CHAT_MCP__.__deliver({})",
            payload
        ),
    );
}

fn on_bridge_request(
    trigger: On<Receive<ChatBridgeRequest>>,
    browsers: NonSend<Browsers>,
    service: Option<Res<ServiceClient>>,
    mut pending: ResMut<PendingBridgeCalls>,
) {
    let req = &trigger.payload;
    if req.channel != BRIDGE_CHANNEL {
        return;
    }

    match req.method.as_str() {
        "listTools" => {
            let result = serde_json::to_value(vmux_mcp::tools::tool_definitions())
                .unwrap_or_else(|_| serde_json::json!([]));
            execute_js_deliver(
                &browsers,
                &trigger.webview,
                &delivery_result(&req.id, result),
            );
        }
        "callTool" => {
            handle_call_tool(
                req,
                trigger.webview,
                &browsers,
                service.as_deref(),
                &mut pending,
            );
        }
        other => {
            execute_js_deliver(
                &browsers,
                &trigger.webview,
                &delivery_error(&req.id, format!("unknown method: {other}")),
            );
        }
    }
}

/// Parse `{ name, arguments }`, dispatch to an `AgentCommand`/`AgentQuery`, and
/// send it to the service. The result is delivered later by
/// `deliver_bridge_results`. Synchronous failures (bad params, dispatch error,
/// no service) are delivered immediately as a resolved MCP error result.
fn handle_call_tool(
    req: &ChatBridgeRequest,
    webview: Entity,
    browsers: &Browsers,
    service: Option<&ServiceClient>,
    pending: &mut PendingBridgeCalls,
) {
    let deliver_tool_error = |message: String| {
        execute_js_deliver(
            browsers,
            &webview,
            &delivery_result(&req.id, vmux_mcp::protocol::tool_error(&message)),
        );
    };

    let Some(name) = req.params.get("name").and_then(serde_json::Value::as_str) else {
        deliver_tool_error("callTool missing name".to_string());
        return;
    };
    let arguments = req
        .params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));

    // The chat webview is not an agent, so there is no self-anchor. Pane-targeting
    // tools fail fast inside dispatch and surface as a clean MCP error here.
    let target = match vmux_mcp::tools::dispatch_with_anchor(name, arguments, None) {
        Ok(target) => target,
        Err(message) => {
            deliver_tool_error(message);
            return;
        }
    };

    let Some(service) = service else {
        deliver_tool_error("vmux service is not connected".to_string());
        return;
    };

    let request_id = AgentRequestId::new();
    let message = match target {
        vmux_mcp::tools::DispatchTarget::Command(command) => ClientMessage::AgentCommand {
            request_id,
            anchor: None,
            command,
        },
        vmux_mcp::tools::DispatchTarget::Query(query) => {
            ClientMessage::AgentQuery { request_id, query }
        }
    };
    pending.0.insert(
        request_id,
        PendingBridgeCall {
            webview,
            bridge_id: req.id.clone(),
        },
    );
    service.0.send(message);
}

/// Match async service results to pending `callTool` requests and deliver the
/// mapped MCP payload to the originating page. A failed command (`Err`) is
/// delivered as a resolved MCP error result, not a rejection.
fn deliver_bridge_results(
    mut command_results: MessageReader<AgentCommandResultEvent>,
    mut query_results: MessageReader<AgentQueryResultEvent>,
    browsers: NonSend<Browsers>,
    mut pending: ResMut<PendingBridgeCalls>,
) {
    for ev in command_results.read() {
        let Some(call) = pending.0.remove(&ev.request_id) else {
            continue;
        };
        let result = match vmux_mcp::protocol::command_result_to_mcp_response(ev.result.clone()) {
            Ok(value) => value,
            Err(message) => vmux_mcp::protocol::tool_error(&message),
        };
        execute_js_deliver(
            &browsers,
            &call.webview,
            &delivery_result(&call.bridge_id, result),
        );
    }
    for ev in query_results.read() {
        let Some(call) = pending.0.remove(&ev.request_id) else {
            continue;
        };
        let result = vmux_mcp::protocol::query_result_to_mcp_response(ev.result.clone());
        execute_js_deliver(
            &browsers,
            &call.webview,
            &delivery_result(&call.bridge_id, result),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_chat_origin_accepts_prod_and_local() {
        assert!(is_chat_origin("https://chat.mistral.ai/"));
        assert!(is_chat_origin("https://chat.mistral.ai/chat/abc?x=1"));
        assert!(is_chat_origin("https://chat.local.mistral.ai:8443/"));
        assert!(is_chat_origin("https://chat.local.mistral.ai:8443"));
    }

    #[test]
    fn is_chat_origin_rejects_others() {
        assert!(!is_chat_origin("http://chat.mistral.ai/")); // not https
        assert!(!is_chat_origin("https://evil.com/"));
        assert!(!is_chat_origin("https://chat.mistral.ai.evil.com/"));
        assert!(!is_chat_origin("https://notchat.mistral.ai/"));
        assert!(!is_chat_origin("https://chat.local.mistral.ai/")); // wrong port
        assert!(!is_chat_origin("vmux://terminal/"));
        assert!(!is_chat_origin(""));
    }

    #[test]
    fn list_tools_delivery_carries_tools_array() {
        let result = serde_json::to_value(vmux_mcp::tools::tool_definitions()).unwrap();
        assert!(result.is_array());
        let arr = result.as_array().unwrap();
        assert!(!arr.is_empty());
        // Tool definitions serialize camelCase: name/description/inputSchema.
        let first = &arr[0];
        assert!(first.get("name").is_some());
        assert!(first.get("inputSchema").is_some());
    }

    #[test]
    fn delivery_result_shapes_id_and_result() {
        let payload = delivery_result("lcm-3", serde_json::json!({"content": []}));
        assert_eq!(payload["id"], "lcm-3");
        assert_eq!(payload["result"], serde_json::json!({"content": []}));
        assert!(payload.get("error").is_none());
    }

    #[test]
    fn delivery_error_shapes_id_and_error() {
        let payload = delivery_error("lcm-4", "boom".to_string());
        assert_eq!(payload["id"], "lcm-4");
        assert_eq!(payload["error"], "boom");
        assert!(payload.get("result").is_none());
    }

    #[test]
    fn command_error_maps_to_resolved_mcp_error() {
        // A failed command must resolve as an MCP error result (isError:true),
        // not reject — so the le-chat block submits a clean tool result.
        let mapped = match vmux_mcp::protocol::command_result_to_mcp_response(
            vmux_service::protocol::AgentCommandResult::Error("nope".to_string()),
        ) {
            Ok(value) => value,
            Err(message) => vmux_mcp::protocol::tool_error(&message),
        };
        let payload = delivery_result("lcm-5", mapped);
        assert!(payload.get("error").is_none(), "must resolve, not reject");
        assert_eq!(payload["result"]["isError"], true);
        assert_eq!(payload["result"]["content"][0]["text"], "nope");
    }

    #[test]
    fn command_ok_maps_to_text_content() {
        let mapped = vmux_mcp::protocol::command_result_to_mcp_response(
            vmux_service::protocol::AgentCommandResult::Ok,
        )
        .unwrap();
        assert_eq!(mapped["content"][0]["text"], "ok");
    }

    #[test]
    fn settings_query_maps_to_text_content() {
        let value = vmux_mcp::protocol::query_result_to_mcp_response(
            vmux_service::protocol::AgentQueryResult::Settings("{\"a\":1}".to_string()),
        );
        assert_eq!(value["content"][0]["text"], "{\"a\":1}");
    }
}
