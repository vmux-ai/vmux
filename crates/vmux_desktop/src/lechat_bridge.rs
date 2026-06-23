use std::collections::HashMap;

use bevy::ecs::relationship::Relationship;
use bevy::prelude::*;
use bevy_cef::prelude::{Browsers, JsEmitEventPlugin, Receive, WebviewCommittedNavigationEvent};
use vmux_layout::pane::{Pane, PaneSplit};
use vmux_service::agent_events::{AgentCommandResultEvent, AgentQueryResultEvent};
use vmux_service::client::ServiceClient;
use vmux_service::protocol::{AgentCommand, AgentRequestId, ClientMessage};

pub struct LeChatBridgePlugin;

impl Plugin for LeChatBridgePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(JsEmitEventPlugin::<ChatBridgeRequest>::default())
            .init_resource::<PendingBridgeCalls>()
            .add_observer(on_bridge_request)
            .add_systems(
                Update,
                (
                    clear_pending_on_main_frame_nav,
                    inject_shim_on_chat_nav,
                    deliver_bridge_results,
                )
                    .chain(),
            );
    }
}

#[derive(serde::Deserialize)]
struct ChatBridgeRequest {
    channel: String,
    id: String,
    method: String,
    #[serde(default)]
    params: serde_json::Value,
}

struct PendingBridgeCall {
    webview: Entity,
    bridge_id: String,
}

#[derive(Resource, Default)]
struct PendingBridgeCalls(HashMap<AgentRequestId, PendingBridgeCall>);

const BRIDGE_CHANNEL: &str = "le-chat-mcp";

fn is_chat_origin(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else {
        return false;
    };
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

fn clear_pending_on_main_frame_nav(
    mut events: MessageReader<WebviewCommittedNavigationEvent>,
    mut pending: ResMut<PendingBridgeCalls>,
) {
    for ev in events.read() {
        if ev.is_main_frame {
            pending.0.retain(|_, call| call.webview != ev.webview);
        }
    }
}

fn delivery_result(bridge_id: &str, result: serde_json::Value) -> serde_json::Value {
    serde_json::json!({ "id": bridge_id, "result": result })
}

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

fn url_opening_tool_url(name: &str, arguments: &serde_json::Value) -> Option<String> {
    let name = name.strip_prefix("vmux_").unwrap_or(name);
    match name {
        "browser_navigate"
            if arguments
                .get("pane")
                .and_then(serde_json::Value::as_str)
                .filter(|s| !s.trim().is_empty())
                .is_none() => {}
        "open_page" => {}
        "in_place" => {}
        _ => return None,
    }
    let url = arguments
        .get("url")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .trim();
    if url.starts_with("http://") || url.starts_with("https://") {
        Some(url.to_string())
    } else {
        None
    }
}

fn chat_pane_for_webview(
    webview: Entity,
    child_of: &Query<&ChildOf>,
    leaf_panes: &Query<Entity, (With<Pane>, Without<PaneSplit>)>,
) -> Option<Entity> {
    let stack = child_of.get(webview).ok()?.get();
    let pane = child_of.get(stack).ok()?.get();
    leaf_panes.contains(pane).then_some(pane)
}

fn on_bridge_request(
    trigger: On<Receive<ChatBridgeRequest>>,
    browsers: NonSend<Browsers>,
    service: Option<Res<ServiceClient>>,
    mut pending: ResMut<PendingBridgeCalls>,
    child_of: Query<&ChildOf>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
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
            let chat_pane = chat_pane_for_webview(trigger.webview, &child_of, &leaf_panes);
            handle_call_tool(
                req,
                trigger.webview,
                &browsers,
                service.as_deref(),
                &mut pending,
                chat_pane,
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

fn handle_call_tool(
    req: &ChatBridgeRequest,
    webview: Entity,
    browsers: &Browsers,
    service: Option<&ServiceClient>,
    pending: &mut PendingBridgeCalls,
    chat_pane: Option<Entity>,
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

    let target = match chat_pane
        .and_then(|pane| url_opening_tool_url(name, &arguments).map(|url| (pane, url)))
    {
        Some((pane, url)) => {
            vmux_mcp::tools::DispatchTarget::Command(AgentCommand::BrowserNavigate {
                url,
                pane: Some(pane.to_bits().to_string()),
            })
        }
        None => match vmux_mcp::tools::dispatch_with_anchor(name, arguments, None) {
            Ok(target) => target,
            Err(message) => {
                deliver_tool_error(message);
                return;
            }
        },
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
        assert!(!is_chat_origin("http://chat.mistral.ai/"));
        assert!(!is_chat_origin("https://evil.com/"));
        assert!(!is_chat_origin("https://chat.mistral.ai.evil.com/"));
        assert!(!is_chat_origin("https://notchat.mistral.ai/"));
        assert!(!is_chat_origin("https://chat.local.mistral.ai/"));
        assert!(!is_chat_origin("vmux://terminal/"));
        assert!(!is_chat_origin(""));
    }

    #[test]
    fn list_tools_delivery_carries_tools_array() {
        let result = serde_json::to_value(vmux_mcp::tools::tool_definitions()).unwrap();
        assert!(result.is_array());
        let arr = result.as_array().unwrap();
        assert!(!arr.is_empty());
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

    #[test]
    fn remap_browser_navigate_http_url_to_pane() {
        let url = url_opening_tool_url(
            "browser_navigate",
            &serde_json::json!({"url": "https://example.com"}),
        );
        assert_eq!(url.as_deref(), Some("https://example.com"));

        let url = url_opening_tool_url(
            "vmux_browser_navigate",
            &serde_json::json!({"url": "http://example.com/path"}),
        );
        assert_eq!(url.as_deref(), Some("http://example.com/path"));
    }

    #[test]
    fn remap_open_page_and_in_place_http_url() {
        assert_eq!(
            url_opening_tool_url("open_page", &serde_json::json!({"url": "https://a.com"}))
                .as_deref(),
            Some("https://a.com"),
        );
        assert_eq!(
            url_opening_tool_url(
                "vmux_in_place",
                &serde_json::json!({"url": "https://b.com"})
            )
            .as_deref(),
            Some("https://b.com"),
        );
    }

    #[test]
    fn remap_skips_browser_navigate_with_explicit_pane() {
        assert!(
            url_opening_tool_url(
                "browser_navigate",
                &serde_json::json!({"url": "https://example.com", "pane": "12345"}),
            )
            .is_none()
        );
    }

    #[test]
    fn remap_skips_non_http_urls() {
        assert!(
            url_opening_tool_url(
                "browser_navigate",
                &serde_json::json!({"url": "vmux://terminal/"}),
            )
            .is_none()
        );
        assert!(
            url_opening_tool_url("in_place", &serde_json::json!({"url": "file:///tmp/x"}))
                .is_none()
        );
        assert!(url_opening_tool_url("in_place", &serde_json::json!({})).is_none());
        assert!(url_opening_tool_url("open_page", &serde_json::json!({"url": "  "})).is_none());
    }

    #[test]
    fn remap_skips_unrelated_tools() {
        for name in [
            "in_new_stack",
            "in_new_tab",
            "in_new_space",
            "open_file",
            "run",
            "list_spaces",
            "screenshot",
        ] {
            assert!(
                url_opening_tool_url(name, &serde_json::json!({"url": "https://x.com"})).is_none(),
                "{name} should not be remapped"
            );
        }
    }
}
