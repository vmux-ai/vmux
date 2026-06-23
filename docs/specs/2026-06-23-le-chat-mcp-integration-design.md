# Integrating vmux with le-chat's `__LE_CHAT_MCP__` host bridge — design

Date: 2026-06-23
Status: Draft
Repo: vmux-ai/vmux
Depends on: le-chat "host-provided MCP tools" feature (the `window.__LE_CHAT_MCP__`
contract; spec lives in the dashboard repo).

## Context

le-chat is gaining a host-agnostic extension point: any client rendering
chat.mistral.ai may expose an in-page MCP surface at `window.__LE_CHAT_MCP__`,
and the le-chat model can then discover + call those tools, with results fed back
into the orchestration loop (server-side, via le-chat's task-callback resume seam).

vmux already renders chat.mistral.ai in a CEF webview, so it can implement that
bridge and surface vmux's **in-process** MCP server to the le-chat model. This is
the browser analog of how vmux already injects its MCP into the vibe CLI over
stdio — here the transport is the in-page bridge instead of stdio + env.

## Goal

When vmux renders chat.mistral.ai, the le-chat model can discover and call vmux's
MCP tools (`vmux_read_layout`, `vmux_update_layout`, `vmux_open_page`, `vmux_run`;
`vmux_`-namespaced as of #133), and results resume the chat turn.

**Non-goals**
- No changes to the le-chat contract itself (that is the dashboard spec). This
  spec only covers vmux's host-side implementation of the bridge.
- No stdio/sidecar for this path — vmux serves MCP in-process within the CEF host.

## The endpoint vmux must implement (contract recap)

```ts
window.__LE_CHAT_MCP__ = {
  protocolVersion: string,
  listTools: () => Promise<McpToolDef[]>,            // MCP tools/list
  callTool: (name, args) => Promise<McpToolResult>,  // MCP tools/call
  onToolsChanged?: (cb) => () => void,
}
```

## vmux implementation

### Webview targeting
- Implement the bridge **only** for the chat.mistral.ai frame. Hook committed
  navigation (`bevy_cef_core::prelude::WebviewCommittedNavigationEvent`, see
  `crates/vmux_history/src/spawn.rs`) and check origin (chat.mistral.ai is already
  recognized, `crates/vmux_ui/src/favicon.rs:18`). Never inject into other pages.

### JS shim injection
- On navigation to chat.mistral.ai, inject a JS shim that defines
  `window.__LE_CHAT_MCP__`. `listTools`/`callTool` marshal requests over
  bevy_cef's JS↔Rust IPC channel and await replies.
- bevy_cef IPC primitives already used in vmux:
  `BinEventEmitterPlugin` / `BinIpcEventRawBuffer`
  (`crates/vmux_agent/src/client/page/plugin.rs`,
  `crates/vmux_ui/src/bin_ipc_envelope.rs`).
- Persist the global across SPA client-side navigations (re-inject if the frame
  context is recreated).

### Rust host handler
- Receive `list`/`call` IPC envelopes from the shim → dispatch to the in-process
  vmux MCP (`crates/vmux_mcp/src/protocol.rs` — `tool_definitions`, tool dispatch).
- Map vmux MCP `tools/list` → `McpToolDef[]`; `tools/call` result → `McpToolResult`
  (`content[]`, `isError`). Reply over IPC to resolve the JS promise.
- `protocolVersion`: advertise a value matching le-chat's expected minimum.
- `onToolsChanged`: emit when the vmux tool set changes (optional for v1).

### Reuse / relationship to the vibe path
- Mirrors the existing vibe injection (`crates/vmux_agent/src/client/cli/vibe.rs`,
  `crates/vmux_agent/src/mcp.rs`, `crates/vmux_agent/src/launch.rs`;
  `McpServerConfig` in `crates/vmux_core/src/agent.rs`) but over the in-page
  bridge instead of `VIBE_MCP_SERVERS` + stdio. The MCP surface (`vmux_mcp`) is
  shared between both paths.

## Security
- Expose `window.__LE_CHAT_MCP__` **only** in the chat.mistral.ai-origin webview;
  do not inject into arbitrary pages.
- Validate inbound `callTool` names against the live vmux MCP tool list before
  dispatch.

## Open questions
- **Auth/session:** does the vmux-rendered chat.mistral.ai run an authenticated
  session, and is the le-chat feature flag enabled for that account? Both are
  required for the model to receive the advertised tools.
- **Anchor scoping:** if multiple chat.mistral.ai webviews exist, scope each
  bridge instance to its webview (cf. the `--anchor` concept in the stdio path).
- **Namespacing:** vmux tools are `vmux_`-prefixed (#133); le-chat additionally
  namespaces host tools `host_*` server-side — confirm the combined naming is
  unambiguous and that the model sees sensible tool names.
- **Injection timing:** confirm the shim lands before le-chat reads
  `window.__LE_CHAT_MCP__` on page load (and on re-auth/navigation).

## Build sequence
1. JS shim defining `window.__LE_CHAT_MCP__` over the bevy_cef IPC channel.
2. Rust host handler bridging IPC ↔ in-process vmux MCP (`vmux_mcp`).
3. Inject on committed navigation to the chat.mistral.ai origin only.
4. Validate end-to-end against a local le-chat with the feature flag enabled.
