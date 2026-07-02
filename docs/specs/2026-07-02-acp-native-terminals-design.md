# ACP-native terminals design

**Goal:** Make vmux a fully ACP-compliant client for terminals. Implement the five ACP
client terminal methods, backed by real (visible) vmux terminal panes, so any ACP agent's
shell/Bash execution flows through vmux as standard ACP terminal tool-calls with embedded live
output. Drop the vmux-custom `run` + `read_terminal` tools for ACP agents; keep `terminal_send`
as the one documented vmux extension for interactivity ACP cannot express.

Decided (2026-07-02): **ACP-native + keep send.**

## Why

- Compliance: every ACP agent gets terminals for free, no vmux-specific tool knowledge.
- UX: terminal work surfaces as ACP terminal tool-calls with embedded output (the "show what's
  running" the transcript work aimed at), not opaque custom-tool cards.
- The custom `run` (interactive shell + done-marker) is non-standard and fragile (pagers,
  multiline). ACP `terminal/create` is command-scoped and clean.

## The five methods (client-provided; agent → client)

Currently stubbed in `crates/vmux_service/src/acp/driver.rs` with `respond_with_internal_error`,
and `init.client_capabilities.terminal = false`. Flip to `true` and implement:

1. `terminal/create(command, args, env, cwd, outputByteLimit)` → `{ terminalId }`
   - Spawn a real PTY via the daemon's process manager (command/args/env/cwd).
   - Store `terminalId → ProcessId` in `AcpShared.terminals`.
   - Emit `ServiceMessage::AcpTerminalCreated { sid, terminal_id, process_id, anchor }` so the GUI
     opens a visible terminal pane beside the agent (anchor), attached to that `ProcessId`.
   - Respond with the ACP `terminalId` (use the vmux `ProcessId` string as the id).
2. `terminal/output(terminalId)` → `{ output, truncated, exitStatus? }`
   - Look up `ProcessId`; read scrollback (as `ReadTerminalFull` does today) + exit status.
3. `terminal/wait_for_exit(terminalId)` → `{ exitStatus }`
   - Await the process exit (see "Exit signalling" below).
4. `terminal/kill(terminalId)` → kill the process (keep the pane for output reading).
5. `terminal/release(terminalId)` → drop from the map; leave the pane (user may still want it).

## Architecture

The daemon (`vmux_service`) owns the process manager (it answers `ReadTerminal`/`ReadTerminalFull`
directly — `server.rs:557`). So the ACP driver, which runs as a daemon-side tokio task, can spawn /
read / kill PTYs **without a GUI round-trip**. The GUI's only job is to *display* the pane.

Plumbing to add:
- A shareable handle to the process manager (e.g. `Arc<...>` or an mpsc command channel) passed
  into `AcpSessionManager::spawn` → `driver::run` → stored on `AcpShared`. The manager already
  lives in the daemon; expose the spawn/read/kill/subscribe-exit surface the driver needs.
- **Exit signalling:** `terminal/wait_for_exit` needs to await a specific process's exit. Add a
  per-process exit `watch`/`oneshot` (or broadcast of `ProcessExited { process_id, code }`) the
  driver can await. The manager already tracks exits (GUI shows `ProcessExited`); surface it to the
  daemon side.

GUI side (`vmux_agent`):
- Handle `AcpTerminalCreated` → create a terminal pane beside the anchor attached to `process_id`
  (reuse the existing "attach terminal to ProcessId" + open-beside logic used by `run`).

## Projector / chat surfacing

- ACP agents reference the terminal via `ToolCallContent::Terminal { terminalId }` inside tool
  calls. Extend `AcpProjector` to fold that into the transcript (a tool-call block that points at
  the terminal / shows its captured output), analogous to the `Diff` handling. Live output already
  shows in the pane; the chat card can show a compact tail or a "Terminal" label.

## Tool changes (`vmux_mcp`)

- ACP sessions must **not** be offered `run` / `read_terminal` (they use ACP terminals instead),
  but **keep** `terminal_send` (no ACP equivalent for keystrokes/TUIs).
- Other MCP clients (CLI agents, direct callers) still need `run`/`read_terminal` until the CLI
  path is retired. So gate the toolset per-client: pass a flag to the sidecar for ACP sessions
  (e.g. `vmux mcp --acp-terminals`) that hides `run` + `read_terminal` from `tool_definitions()`.
  `terminal_send` stays in both.

## Testing

- Unit: driver terminal map create/lookup/release; projector folding of
  `ToolCallContent::Terminal`.
- Integration: `terminal/create` → `AcpTerminalCreated` emitted with the right process id;
  `wait_for_exit` resolves on `ProcessExited`; toolset excludes `run`/`read_terminal` under the ACP
  flag but keeps `terminal_send`.
- Manual (end): ask an ACP agent to run a few commands → they appear as ACP terminal tool-calls in
  visible panes with live output; `git log` etc. work; `terminal_send` still drives a TUI.

## Open risks

- Whether `claude-code-acp` / `codex-acp` actually delegate their shell tool to client terminals
  when `terminal: true` is advertised (they should — it's the ACP design — but verify early; if an
  agent runs Bash internally regardless, advertising the capability is a no-op for it and we keep
  `run` available to that agent).
- `outputByteLimit` + truncation semantics must match ACP (report `truncated: true`).
- Pane lifecycle on `release`/session close (don't orphan panes or kill user-adopted terminals).

## Scope note

Sizeable, cross-layer. Fits `feat/acp-host` (core compliance) but is large; can also be its own
follow-up PR stacked on it. Implement per `docs/plans/` once this design is approved.
