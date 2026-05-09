# Chat New Tab Design

## Summary

Replace the empty-tab + auto-open-command-bar new-tab flow with an embedded chat UI loaded from `https://chat.mistral.ai/` (le-chat-web). Expose vmux as an HTTP MCP connector so the chat slash menu (`/`) surfaces vmux tools — the same tools shipped in vmx-107 for the CLI. Users invoke vmux commands by picking from the slash menu; the LLM dispatches via MCP.

Cmd+L command bar overlay remains unchanged for in-tab navigation/search.

## Motivation

- Reuse le-chat-web instead of building a chat UI in vmux.
- Unify human and agent surfaces: the same MCP tools serve both `vmux mcp` (stdio) and the in-app chat (HTTP).
- Remove the empty-tab special path introduced earlier — its responsibilities collapse into the chat tab.

## Scope

Build all of v1 in one go: chat tab, HTTP MCP transport, manual connector registration. Defer auto-registration, direct (non-LLM) tool execution, and a custom chat UI.

Branch base: `feature/vmx-107-expose-all-mcp-tools`. Implementation lives in a new worktree (`.worktrees/vmx-NNN`) per `AGENTS.md`.

## Settings Changes

`crates/vmux_desktop/src/settings.ron`:

```ron
browser: (
    startup_url: "https://chat.mistral.ai/",
),
chat_mcp: (
    bind: "127.0.0.1:0",   // 0 = OS picks port
    token: None,            // generated on first launch if absent
),
```

`crates/vmux_desktop/src/settings.rs`:

- Repurpose `BrowserSettings.startup_url` for the new-tab URL. Drop `#[allow(dead_code)]`.
- New `ChatMcpSettings { bind: String, token: Option<String> }`.
- Validate `bind` host is `127.0.0.1` (reject `0.0.0.0` etc.).

## New Tab Spawn Flow

`crates/vmux_layout/src/tab.rs` — `TabCommand::New` (browser variant):

- Spawn `Tab + Browser(settings.browser.startup_url)` directly, mirroring how the terminal/processes variants spawn today.
- No empty tab. No `BackgroundColor` glass. No deferred command-bar open.

Cleanup (delete from vmx-107 base):

- `NewTabContext` resource and all references in `command_bar.rs`, `tab.rs`, `browser.rs`.
- `CommandBarOpenEvent.new_tab` field and the `new_tab` mode logic.
- Empty-tab `BackgroundColor` / glass code.
- `docs/specs/2026-04-23-new-tab-design.md` — superseded by this spec.

Untouched:

- Cmd+Shift+T → terminal directly (`TerminalCommand::New`).
- Cmd+L → command bar overlay (works on any tab including chat).

## MCP HTTP Transport

`crates/vmux_mcp/src/protocol.rs`:

- Keep `handle_message(Value) -> Option<Value>` as the shared dispatcher.
- Add `pub async fn run_http(addr: SocketAddr, token: String) -> io::Result<()>`.

`crates/vmux_mcp/Cargo.toml` — add deps:

- `axum` (HTTP framework)
- `tower-http` (CORS layer)

Endpoint shape (Streamable HTTP per MCP 2025-03-26 transport):

```
POST /mcp   JSON-RPC request → JSON-RPC response
GET  /mcp   SSE stream for server → client notifications (optional v1)
```

Auth:

- Require header `Authorization: Bearer <token>`.
- Mismatch → 401.
- Token: random 32-byte hex, generated at startup if `settings.chat_mcp.token` is `None`. Persist on first generation so the registered connector survives restart.

Bind safety:

- Localhost only. Reject any non-loopback bind in config validation.
- CORS: allow `https://chat.mistral.ai` origin (and a configurable local-dev origin if needed).

## Host Integration

New module: `crates/vmux_desktop/src/mcp_host.rs`.

- App startup reads `settings.chat_mcp`, resolves port (0 → OS-assigned), spawns tokio task running `vmux_mcp::run_http(addr, token)`.
- Effective `URL` and `token` logged on startup so the user can paste them into le-chat-web's Connections form.
- Add a small "MCP" status row in the vmux settings UI (or `vmux --print-mcp` CLI flag) showing the URL and token for copy-paste.

`vmux mcp` CLI (stdio entry) is unchanged; both transports coexist.

## Connector Registration (v1, manual)

One-time setup per le-chat account:

1. Open vmux. Cmd+T spawns chat tab loading `https://chat.mistral.ai/`.
2. In chat: Connections → Add custom connector.
3. Form values:
   - `name`: `vmux`
   - `server`: `http://localhost:<PORT>/mcp` (from vmux startup log / settings)
   - `headerName`: `Authorization`
   - `headerType`: `Bearer`
   - `apiToken`: `<token>` (from vmux startup log / settings)
4. Save. Connector active.

Auto-registration (deeplink or one-click button) is out of scope for v1.

## Slash Flow (no le-chat-web changes)

```
User in chat input: types "/"
  → existing slash menu opens
  → fuzzy match shows vmux tools: tab_close, browser_navigate,
    terminal_send, list_tabs, list_spaces, switch_tab, ...
User picks `browser_navigate`
  → inserted as inline entity (existing slash-command behavior)
User completes prompt + sends ("navigate to google")
  → LLM sees inline tool reference + free text
  → LLM calls vmux.browser_navigate({"url": "https://google.com"})
  → POST localhost:PORT/mcp → handle_message → vmux_service
  → AgentCommand::Browser(BrowserCommand::Navigate { url, ... })
  → desktop handler executes; tool returns Ok
  → chat shows tool success
```

Latency: each command = 1 LLM round-trip + MCP call (~1-2s typical). Acceptable for v1; bypassing the LLM requires a le-chat-web fork.

## Edge Cases

| Scenario | Behavior |
|---|---|
| Chat URL unreachable / offline | Standard CEF browser error page in tab |
| chat.mistral.ai requires login | Standard web auth in CEF; cookies persist in CEF profile |
| MCP port already bound | `127.0.0.1:0` lets OS pick; log effective URL |
| Token leaked / rotated | User clears `chat_mcp.token`, regenerated on next launch, re-register connector |
| Multiple vmux instances on same machine | Each binds its own port; each token unique |
| User without le-chat account | Chat tab shows login wall; not vmux's concern |
| Cmd+T from terminal-focused pane | Spawns chat tab in same pane (current `TabCommand::New` semantics) |
| Cmd+L on chat tab | Existing command bar overlay, unchanged |
| `vmux_service` unreachable | Tool returns error; chat surfaces "tool failed" |
| Chat tab opened before connector registered | Slash menu lacks vmux tools; everything else works |

## Out of Scope

- Auto-register vmux as a connector via deeplink.
- Direct tool execution bypassing the LLM round-trip.
- Inline non-slash detection (URL / keyword auto-suggest without `/`).
- Multiple chat sessions or session sharing across tabs.
- Custom in-house chat UI.
- Deprecating `vmux mcp` (stdio) — both transports stay.
