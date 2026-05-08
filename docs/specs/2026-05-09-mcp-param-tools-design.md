# MCP Param-Bearing Tools (extension of VMX-107) — Design

Linear: [VMX-107](https://linear.app/vmux/issue/VMX-107/expose-all-app-commands-to-mcp) (scope extended)

## Goal

Add three new param-bearing MCP tools to fill gaps that the auto-generated zero-arg tools cannot cover:

- `browser_navigate(url)` — load an arbitrary URL in the active webview.
- `select_tab(index)` — select tab by index (collapsing the eight zero-arg `tab_select_N` variants behind a cleaner agent-facing API).
- `terminal_send(text)` — write raw text to the active terminal without `run_shell`'s auto-appended carriage return.

## Why

The first batch of VMX-107 work auto-exposed every `AppCommand` variant, but those tools are all zero-arg by construction. Real agent workflows need to pass strings/integers — opening a URL, picking a specific tab, typing partial commands. These three additions fill the most obvious gaps from initial usage.

`browser_find(query)` was considered but deferred to a separate ticket: the underlying in-app find feature (`BrowserCommand::Find`) is a stub today and needs its own design pass.

## Approach

Follow the existing param-bearing-tool pattern (`new_terminal_tab`, `run_shell`):

1. Add new variants to `vmux_service::protocol::AgentCommand`.
2. Add validation in `validate_agent_command`.
3. Handle each variant in `vmux_desktop::agent::handle_agent_commands`.
4. Add hand-written tools in `vmux_mcp::tools::tool_definitions()` and dispatch in `agent_command_from_tool_call()`.

`select_tab` is special: it does not need a new `AgentCommand` variant. The MCP layer translates `select_tab(index)` to the existing `AgentCommand::AppCommand { id: format!("tab_select_{index}") }`, leveraging the auto-generated `from_agent_id` resolution from the first VMX-107 batch.

## Changes

### 1. `vmux_service::protocol`

Add to `AgentCommand`:

```rust
BrowserNavigate {
    url: String,
},
TerminalSend {
    text: String,
},
```

Extend `validate_agent_command` to reject:
- `BrowserNavigate { url }` when `url.trim().is_empty()` → `"browser_navigate.url is empty"`.
- `TerminalSend { text }` when `text.is_empty()` → `"terminal_send.text is empty"`. (Use raw `is_empty`, not `trim().is_empty()`, since whitespace-only sends are sometimes meaningful for terminals.)

### 2. `vmux_desktop::agent`

In `handle_agent_commands`, add two arms:

- `ServiceAgentCommand::BrowserNavigate { url }`:
  - Resolve the active webview the same way `vmux_desktop::browser::handle_browser_commands` does (focused tab → child webview entity, excluding terminals).
  - If found and not a terminal, `commands.trigger(RequestNavigate { webview, url: url.clone() })`.
  - Otherwise warn and drop.

  Implementation note: the active-webview lookup currently lives inline in `handle_browser_commands`. Extract a small helper `active_webview_for_tab` (next to the existing `active_terminal_for_tab`) so both `browser.rs` and `agent.rs` can call it. This is targeted improvement of code we're already touching, not unrelated refactoring.

- `ServiceAgentCommand::TerminalSend { text }`:
  - Resolve active terminal via the existing `active_terminal_for_tab` helper.
  - If found, insert `PendingTerminalInput { data: text.into_bytes() }`. **No `\r` appended** (this is the distinguishing behaviour from `RunShell`).
  - Otherwise warn and drop.

### 3. `vmux_mcp::tools`

`tool_definitions()` — append three new hand-written tools after the existing three:

```jsonc
{
  "name": "browser_navigate",
  "description": "Navigate the active webview to a URL.",
  "inputSchema": { "type": "object", "properties": { "url": {"type": "string"} }, "required": ["url"] }
}
{
  "name": "select_tab",
  "description": "Select a tab by index (1-8).",
  "inputSchema": { "type": "object", "properties": { "index": {"type": "integer", "minimum": 1, "maximum": 8} }, "required": ["index"] }
}
{
  "name": "terminal_send",
  "description": "Send raw text to the active terminal (no carriage return appended).",
  "inputSchema": { "type": "object", "properties": { "text": {"type": "string"} }, "required": ["text"] }
}
```

`agent_command_from_tool_call()` — add three new match arms before the auto-gen fallback:

- `"browser_navigate"`:
  - Read `url` (required string, non-empty).
  - Return `Ok(AgentCommand::BrowserNavigate { url })`.
- `"select_tab"`:
  - Read `index` (required integer 1-8). Reject out-of-range with `"select_tab.index must be between 1 and 8"`.
  - Return `Ok(AgentCommand::AppCommand { id: format!("tab_select_{index}") })`.
- `"terminal_send"`:
  - Read `text` (required string, non-empty per the validation rule).
  - Return `Ok(AgentCommand::TerminalSend { text })`.

### 4. Tests

`vmux_service::protocol::tests`:
- `empty_browser_navigate_url_is_invalid`
- `empty_terminal_send_text_is_invalid`

`vmux_mcp::tools::tests`:
- `tool_list_includes_param_tools` — assert `browser_navigate`, `select_tab`, `terminal_send` are present.
- `browser_navigate_dispatches_with_url`
- `browser_navigate_missing_url_returns_error`
- `select_tab_dispatches_to_tab_select_id` — assert `select_tab(3)` → `AppCommand { id: "tab_select_3" }`.
- `select_tab_out_of_range_returns_error` — index 0 and index 9.
- `terminal_send_dispatches_with_text`
- `terminal_send_missing_text_returns_error`

`vmux_desktop::agent::tests`:
- `browser_navigate_triggers_request_navigate_on_active_webview` — spawn pane + tab + browser, write `BrowserNavigate { url: "https://example.com" }`, assert a `RequestNavigate` trigger fired with the right url. Use the existing test scaffold pattern from `agent_launch_request_uses_registered_provider_to_spawn_terminal_tab`.
- `terminal_send_writes_raw_text_to_active_terminal` — spawn terminal entity, write `TerminalSend { text: "ls" }`, assert `PendingTerminalInput.data == b"ls".to_vec()` (no trailing `\r`).

## Out of Scope

- `browser_find(query)` and the underlying in-app find feature (separate ticket).
- `select_tab("last")` / index 9. Use the existing zero-arg `tab_select_last` tool.
- Sending control characters (`\x03`, etc.) via `terminal_send` — the bytes are passed through, but no escape-sequence helpers are added.
- `browser_navigate` targeting a non-active tab or opening a new tab. Active webview only.

## Risks

- **Active-webview resolution duplication**: the `focused_tab` lookup logic lives in `browser.rs`. Extracting `active_webview_for_tab` is a small refactor of code we're touching anyway. If the extraction proves messy, fall back to inlining in `agent.rs` and revisit later.
- **Terminal send race**: `PendingTerminalInput` is consumed asynchronously. Two `terminal_send` calls in quick succession may overwrite each other's pending input rather than concatenating. Document this; defer fixing to a follow-up if it bites.
- **Single commit per tool vs. one combined commit**: implementation plan will use one commit per tool to keep review focused.
