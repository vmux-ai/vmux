//! Host-MCP bridge for le-chat (chat.mistral.ai).
//!
//! Injects `window.__LE_CHAT_MCP__` into the chat page on navigation and answers
//! the page's JSON-RPC-ish calls over CEF IPC:
//!   - `listTools` returns vmux's real MCP tool definitions.
//!   - `callTool` returns a stub error (execution wired in a later slice).
//!
//! Page -> Rust:  `cef.emit({ channel, id, method, params })` is delivered to the
//! `on_bridge_request` observer via `Receive<ChatBridgeRequest>`.
//! Rust -> page:  `Browsers::execute_js` calls `window.__LE_CHAT_MCP__.__deliver(msg)`.

use bevy::prelude::*;
use bevy_cef::prelude::{Browsers, JsEmitEventPlugin, Receive, WebviewCommittedNavigationEvent};

pub struct LeChatBridgePlugin;

impl Plugin for LeChatBridgePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(JsEmitEventPlugin::<ChatBridgeRequest>::default())
            .add_observer(on_bridge_request)
            .add_systems(Update, inject_shim_on_chat_nav);
    }
}

/// Distinctive shape so `receive_events::<ChatBridgeRequest>` only fires for our
/// payloads (every emitted JS object is attempted against every registered type).
#[derive(serde::Deserialize)]
struct ChatBridgeRequest {
    channel: String,
    id: String,
    method: String,
    // Carried from the page now; consumed when callTool execution is wired in the next slice.
    #[serde(default)]
    #[allow(dead_code)]
    params: serde_json::Value,
}

const BRIDGE_CHANNEL: &str = "le-chat-mcp";

/// Authorities allowed to receive the bridge shim. Mirrors the CEF IPC gate in
/// `bevy_cef_core` (`is_bridge_allowed_origin`); kept as a small local dup to
/// avoid depending on bevy_cef_core internals.
fn is_chat_origin(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else {
        return false;
    };
    // Authority ends at the first '/', '?' or '#'.
    let authority = rest
        .split(['/', '?', '#'])
        .next()
        .unwrap_or(rest);
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

fn on_bridge_request(trigger: On<Receive<ChatBridgeRequest>>, browsers: NonSend<Browsers>) {
    let req = &trigger.payload;
    if req.channel != BRIDGE_CHANNEL {
        return;
    }

    let delivery = match req.method.as_str() {
        "listTools" => {
            let result = serde_json::to_value(vmux_mcp::tools::tool_definitions())
                .unwrap_or_else(|_| serde_json::json!([]));
            serde_json::json!({ "id": req.id, "result": result })
        }
        "callTool" => {
            let result = serde_json::json!({
                "content": [{ "type": "text", "text": "vmux callTool not yet implemented" }],
                "isError": true
            });
            serde_json::json!({ "id": req.id, "result": result })
        }
        other => {
            serde_json::json!({ "id": req.id, "error": format!("unknown method: {other}") })
        }
    };

    let payload = delivery.to_string();
    browsers.execute_js(
        &trigger.webview,
        &format!("window.__LE_CHAT_MCP__&&window.__LE_CHAT_MCP__.__deliver({payload})"),
    );
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
}
