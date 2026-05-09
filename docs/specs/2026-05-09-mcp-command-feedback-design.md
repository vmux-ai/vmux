# MCP Command Feedback (extension of VMX-107) — Design

Linear: [VMX-107](https://linear.app/vmux/issue/VMX-107/expose-all-app-commands-to-mcp) (scope extended again)

## Goal

Two related fixes that surfaced during dogfooding the MCP query tools:

1. **Auto-create a browser tab** when `browser_navigate(url)` targets a focused pane that has no tabs (or only terminal tabs). Today the call is silently dropped.
2. **Replace fire-and-forget command ACKs** with real request/response so MCP returns success/failure based on the desktop's actual handling, not just IPC delivery. Today the service ACKs immediately on receipt; the agent thinks every command succeeded even when the desktop dropped it.

## Why

Bug #1 surfaced when an agent ran `split_h` → `select_pane_right` → `browser_navigate(google.com)`. The split worked, focus moved to the empty new pane, but the navigate hit `active_webview_for_tab → None` and dropped silently. The agent reported "Done" because Bug #2 made the failure invisible.

Bug #2 affects every command, not just navigate. `select_pane_right` into a non-existent pane, `tab_close` with no tabs, `browser_reload` with no webview — all silently fake-succeed. Without honest feedback, agents can't tell which actions worked.

## Approach

### Fix #1: Auto-spawn browser tab

Targeted change in `vmux_desktop::agent::handle_agent_commands::BrowserNavigate`. Pattern borrowed from existing `spawn_terminal_tab` helper.

### Fix #2: Mirror the AgentQuery pattern for AgentCommand

Already-established pattern from this branch's Tasks 1-4:
- New `AgentCommandResult` enum (`Ok` / `Error(String)`).
- New `ClientMessage::AgentCommandResponse { request_id, result }` and `ServiceMessage::AgentCommandResult { request_id, result }`.
- `vmux_service` registers a oneshot per command, broadcasts `ServiceMessage::AgentCommand`, awaits the response, and routes the result back to the originating MCP client. Same `pending_commands: PendingCommands` shared state, same `AGENT_COMMAND_TIMEOUT` constant (mirrors `AGENT_QUERY_TIMEOUT`).
- `vmux_desktop` makes each `handle_agent_commands` arm produce an `AgentCommandResult` and sends it via `ClientMessage::AgentCommandResponse` after each request is handled.
- `vmux_mcp::run_agent_command` waits for `ServiceMessage::AgentCommandResult` matching the request_id. On `Ok`, returns `{content: [{type: "text", text: "ok"}]}`. On `Error(msg)`, returns `Err(msg)` (becomes MCP `isError: true`).
- The legacy `ServiceMessage::AgentCommandAccepted` variant is kept on the wire for backward compatibility but unused. Removal is a follow-up if no consumers remain.

## Changes

### 1. `vmux_service::protocol`

Add:

```rust
pub const AGENT_COMMAND_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

#[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum AgentCommandResult {
    Ok,
    Error(String),
}
```

Add to `ClientMessage`:
```rust
AgentCommandResponse {
    request_id: AgentRequestId,
    result: AgentCommandResult,
},
```

Add to `ServiceMessage`:
```rust
AgentCommandResult {
    request_id: AgentRequestId,
    result: AgentCommandResult,
},
```

Tests: rkyv roundtrip for `AgentCommandResult` (Ok and Error variants) and for the new ClientMessage/ServiceMessage variants.

### 2. `vmux_service::server`

Mirror the AgentQuery routing exactly. Add `pending_commands: PendingCommands` (same Arc/Mutex/HashMap shape as `pending_queries`), plumb through `handle_client`. Per-connection `in_flight_command_ids` set for cleanup on disconnect.

Replace the existing `ClientMessage::AgentCommand` arm:
- Old behaviour (lines ~342-367): validate, broadcast, immediately ACK with `AgentCommandAccepted`.
- New behaviour: validate; create oneshot, register in `pending_commands`; broadcast `ServiceMessage::AgentCommand` (unchanged); spawn a task that awaits with `AGENT_COMMAND_TIMEOUT` and writes either `ServiceMessage::AgentCommandResult` or a timeout `ServiceMessage::Error` back to the originating client.

Add `ClientMessage::AgentCommandResponse` arm: look up sender in `pending_commands`, remove, fire oneshot, drain `in_flight_command_ids`.

Cleanup on disconnect: same pattern as queries.

Tests: `pending_commands_roundtrips_oneshot` (mirror the existing `pending_queries_roundtrips_oneshot`).

### 3. `vmux_desktop::agent`

Restructure `handle_agent_commands` so each arm produces an `AgentCommandResult` and sends `ClientMessage::AgentCommandResponse` after handling.

```rust
let result = match &request.command {
    ServiceAgentCommand::AppCommand { id } => {
        if let Some(command) = AppCommand::from_agent_id(id) {
            app_commands.write(command);
            AgentCommandResult::Ok
        } else {
            AgentCommandResult::Error(format!("unknown app command: {id}"))
        }
    }
    ServiceAgentCommand::NewTerminalTab { cwd } => {
        match handle_new_terminal_tab(...) {
            Ok(()) => AgentCommandResult::Ok,
            Err(msg) => AgentCommandResult::Error(msg),
        }
    }
    // … same shape for RunShell, BrowserNavigate, TerminalSend
};
service.0.send(ClientMessage::AgentCommandResponse {
    request_id: request._request_id,
    result,
});
```

Note: `request._request_id` becomes `request.request_id` (drop the underscore now that it's actually used). The struct field rename is a tiny API break inside the crate; rename the field in `AgentCommandRequest` and fix the only call site (`terminal.rs` bridging arm).

Each handler arm becomes a small helper that returns `Result<(), String>`:
- `handle_app_command(id, &mut app_commands) -> Result<(), String>` — validates id, writes event, returns Ok or Error.
- `handle_new_terminal_tab(cwd, focus, panes, ..., commands, meshes, webview_mt, settings) -> Result<(), String>`
- `handle_run_shell(...)`
- `handle_browser_navigate(url, focus, browsers, terminals, panes, commands, meshes, webview_mt, settings) -> Result<(), String>` — **includes the auto-spawn logic from Fix #1**.
- `handle_terminal_send(text, focus, terminals, commands) -> Result<(), String>`

The auto-spawn lives entirely inside `handle_browser_navigate`:
1. Try `active_webview_for_tab(focus.tab, browsers, terminals)`. If Some, trigger `RequestNavigate { webview, url }` and return Ok.
2. Else try focused pane via `focus.pane.filter(|p| panes.contains(*p))`. If Some, call new helper `spawn_browser_tab(pane, &url, commands, meshes, webview_mt)` and return Ok.
3. Else return `Err("browser_navigate: no focused pane".to_string())`.

### 4. `vmux_desktop::agent` — `spawn_browser_tab` helper

New helper next to `spawn_terminal_tab`:

```rust
pub(crate) fn spawn_browser_tab(
    pane: Entity,
    url: &str,
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
) -> Entity {
    let tab = commands
        .spawn((
            crate::layout::tab::tab_bundle(),
            LastActivatedAt::now(),
            ChildOf(pane),
        ))
        .id();
    commands.entity(tab).insert(PageMetadata {
        url: url.to_string(),
        title: url.to_string(),
        ..default()
    });
    commands.spawn((
        Browser::new(meshes, webview_mt, url),
        ChildOf(tab),
    ));
    tab
}
```

`Browser::new(meshes, webview_mt, url)` is the public constructor at `crates/vmux_layout/src/chrome.rs:67`. Pattern verified against `crates/vmux_desktop/src/persistence.rs:351-355` (session restore).

### 5. `vmux_mcp::protocol::run_agent_command`

Replace the recv loop:

```rust
match message {
    ServiceMessage::AgentCommandResult {
        request_id: received,
        result,
    } if received == request_id => {
        return match result {
            AgentCommandResult::Ok => Ok(json!({
                "content": [{"type": "text", "text": "ok"}]
            })),
            AgentCommandResult::Error(msg) => Err(msg),
        };
    }
    ServiceMessage::Error { message } => return Err(message),
    _ => {}
}
```

The legacy `AgentCommandAccepted` arm is removed from the matcher (the variant stays in the wire enum for back-compat).

### Tests

**vmux_service::protocol:**
- `agent_command_result_roundtrips` — both Ok and Error variants.
- `agent_command_response_messages_roundtrip` — ClientMessage and ServiceMessage variants.

**vmux_service::server:**
- `pending_commands_roundtrips_oneshot` — mirror of the queries test.

**vmux_desktop::agent:**
- `browser_navigate_auto_spawns_tab_when_pane_is_empty` — fixture: pane with no tabs; send BrowserNavigate; assert a `Browser` entity appears as a grandchild of the pane (Tab in between) with the URL set on PageMetadata.
- Update existing `browser_navigate_triggers_request_navigate_with_url` — still works on existing webview.
- New `app_command_unknown_id_produces_error_result` — assert sending `AgentCommand{id:"nope"}` produces `AgentCommandResult::Error`. (Requires capturing the `ClientMessage::AgentCommandResponse` sent by the handler — use a fake `ServiceClient` or assert via the underlying `service.0.send` capture pattern. If wiring this in unit tests proves messy, document the behavior coverage gap and rely on integration testing.)

**vmux_mcp::tools:**
- Update existing `auto_generated_tool_dispatches_as_app_command` and other dispatch tests — no change needed; they only check the `AgentCommand` construction, not the response loop.

## Out of Scope

- Removing the deprecated `ServiceMessage::AgentCommandAccepted` variant (separate cleanup ticket; keep wire compatible for now).
- True end-to-end "actually-finished" semantics for AppCommand (today we only confirm the event was queued — downstream handlers run async). Acceptable per spec; documented as a known limitation.
- Per-tool timeouts (single 5s `AGENT_COMMAND_TIMEOUT` for all commands).
- Streaming progress / partial results.

## Risks

- **AgentCommand backward compatibility**: existing clients that wait for `AgentCommandAccepted` will hang. Migration: vmux_mcp is the only known consumer; updating it in this same PR avoids the issue. If other consumers appear, they need updating. Keeping the wire variant available for back-compat reduces the risk to "behavioural drift" rather than "ABI break".
- **AppCommand ack truthiness**: `AppCommand` arms return Ok as soon as the event is written, not after downstream systems handle it. Some downstream handlers may silently drop the event (e.g., `BrowserCommand::Find` is a stub today). For agent purposes this is acceptable — they'll see the request reached the desktop. Future work could thread completion signals from individual command handlers, but that's a per-command effort.
- **Latency**: round-trip is now bounded by Bevy frame rate (~16ms typical) plus IPC. Same characteristic as queries. Total ~20ms typical, well under the 5s timeout.
- **Browser auto-spawn UX**: if the pane already had a focused empty tab waiting for input (from a `tab_new` that hasn't received a URL yet), our auto-spawn creates a SECOND tab rather than reusing the empty one. This mirrors the existing `tab_new` "empty tab pending" logic only on the user-driven path; replicating it from agent code is out of scope. If the issue surfaces, file a follow-up.

## File Map

- **Modify** `crates/vmux_service/src/protocol.rs` — `AgentCommandResult` enum, `AGENT_COMMAND_TIMEOUT`, two new ClientMessage/ServiceMessage variants, rkyv tests.
- **Modify** `crates/vmux_service/src/server.rs` — `PendingCommands` shared state, AgentCommand handler rewritten to register-broadcast-await, AgentCommandResponse handler, cleanup on disconnect.
- **Modify** `crates/vmux_desktop/src/agent.rs` — restructure `handle_agent_commands` to produce `AgentCommandResult` per arm and send via `ClientMessage::AgentCommandResponse`. Add `spawn_browser_tab` helper. Add Browser auto-spawn in BrowserNavigate handler. Rename `AgentCommandRequest._request_id` → `request_id`.
- **Modify** `crates/vmux_desktop/src/terminal.rs` — bridging arm at line 776 uses the renamed `request_id` field.
- **Modify** `crates/vmux_mcp/src/protocol.rs` — `run_agent_command` waits for `ServiceMessage::AgentCommandResult`; converts result to MCP response or error.
