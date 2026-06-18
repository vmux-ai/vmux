# Phase C — Desktop cutover to the service-hosted page agent

> Status: C1 (tool round-trip) DONE. This plan covers the remaining C2 (spawn/attach/input rewire + delete the in-desktop loop). The page-agent **chat rendering UI** is a separate effort ("step 4 of the Page agent design") and is out of scope here.

**Goal:** When a Page agent is opened, it runs entirely in `vmux_service`. The desktop spawns it, sends user input down, consumes the streamed deltas/run-state up, and no longer runs any inference loop.

**Reference:** `docs/specs/2026-06-18-page-agent-service-relocation-design.md`. Backend (A, B1, B1b, B2, B3, C1) is committed on `feat/page-agent-service`.

## What already works (committed)

- `vmux_service` hosts `run_session` + `AgentSessionManager` + provider registry (B3).
- Protocol: `SpawnPageAgent`, `AttachPageAgent`, `DetachPageAgent`, `AgentInput`, `AgentApprove`, `ClosePageAgent` (down); `AgentDelta`, `AgentRunStatusChanged`, `AgentAwaitingApproval`, `AgentToolCall`, `AgentMessagesSnapshot` (up) (B2).
- Tool round-trip: service `AgentToolCall` → desktop resolves via `vmux_mcp` + applies + result routed back to `pending_tool_calls` (C1).

## C2 tasks

### Task 1: Spawn a service session when a Page agent opens

**Files:** `crates/vmux_agent/src/plugin.rs` (`attach_page_agent_to_stack` / `handle_agent_page_open` / `respond_page_agent_*`), `crates/vmux_agent/src/client/page/plugin.rs`.

- When a Page-agent stack is attached (currently `attach_page_agent_to_stack`), additionally send `ClientMessage::SpawnPageAgent { sid, provider, model, cwd, auto_tools, tools_json }` via `ServiceClient`, then `ClientMessage::AttachPageAgent { sid }`.
  - `tools_json` = `serde_json::to_string(&mcp_tool_defs())`.
  - `auto_tools` = read from `AppSettings.agent` approval policy (the read-only tool names), matching today's `AgentApprovalPolicy`.
  - `sid` = the existing Page `AgentSession.sid` (already a uuid).
- Keep the desktop `AgentSession` entity (it identifies the stack/view) but it no longer drives a local loop.
- Send `ClosePageAgent { sid }` + `DetachPageAgent { sid }` when the stack/agent entity despawns (hook into `detect_agent_session_process_exit` or an `OnRemove` observer for Page variant).

### Task 2: Route user input to the service

**Files:** `crates/vmux_agent/src/systems/process_input.rs` (replace), plugin wiring.

- Replace `process_user_input` (which built a local request + spawned `drive_sse`) with a system that, on `PendingUserInput` for a Page `AgentSession`, sends `ClientMessage::AgentInput { sid, text }` and removes `PendingUserInput`.
- Approval replies: on `AgentApprovalReply`, send `ClientMessage::AgentApprove { sid, call_id, decision }`.

### Task 3: Consume the service stream into desktop events/state

**Files:** `crates/vmux_terminal/src/plugin.rs` (ServiceMessage match — add arms), `crates/vmux_agent/src/systems/` (new consumer), `crates/vmux_agent/src/events.rs`.

- In `vmux_terminal`'s `ServiceMessage` match, forward the agent-stream variants to ECS messages (mirroring the `AgentToolCall` arm added in C1): `AgentDelta`, `AgentRunStatusChanged`, `AgentAwaitingApproval`, `AgentMessagesSnapshot`. Add the corresponding `agent_events` message types (sid-keyed).
- Add a `vmux_agent` system that maps `sid → AgentSession entity` (via a `Sid → Entity` resource, like `AgentSessionToEntity` but for Page sids) and applies:
  - `AgentDelta` → trigger the existing `events::AgentDelta { session, text }` (the chat UI/webview consumer is unchanged).
  - `AgentMessagesSnapshot` → deserialize `Vec<Message>` → set `AgentMessages`.
  - `AgentRunStatusChanged` → set `AgentRunState` (Idle/Streaming/Errored).
  - `AgentAwaitingApproval` → trigger `events::AgentApprovalRequest`.

### Task 4: Delete the in-desktop loop

**Files:** delete `crates/vmux_agent/src/systems/{drain_stream,dispatch_tool,continue_after_tool}.rs` and `crates/vmux_agent/src/tool_dispatch.rs`; trim `process_input.rs` to Task 2; update `client/page/plugin.rs` to stop registering the deleted systems; remove now-unused `AgentRunState::{Streaming,RunningTool}` task plumbing if fully unused.

- Keep `message`/`stream`/`http`/`providers` usage out of the desktop loop (they live in `vmux_service` now; desktop only needs the `Message` type for rendering, available via `vmux_agent::message` re-export).
- `EMIT_CHANNEL`/`tool_dispatch` removed (tool execution is C1's desktop resolver now).

### Task 5: Verify end-to-end (manual, with the app)

- `make dev`, open a Page agent (e.g. `vmux://agent/anthropic/...` with `ANTHROPIC_API_KEY` set).
- Type a prompt → observe streamed deltas; trigger a tool (e.g. "open example.com") → observe it applied; confirm in Activity Monitor that turn CPU lands on `vmux_service`, not `vmux`.
- Note: full chat *rendering* depends on the separate Page chat-UI work; if that isn't built, verify via service logs + `AgentDelta` events reaching the desktop.

## Risks

- **sid ↔ entity mapping:** Page agents are keyed by `sid` over IPC but by `Entity` in the desktop ECS. Add a dedicated resource; reuse the `AgentSession.sid` already on the entity.
- **Approval policy source:** pass `auto_tools` at spawn from desktop-read settings (decided in the spec) so the service needn't read settings.
- **Deleting the loop** must happen together with Tasks 1-3 or the Page agent regresses; land C2 as one PR.
- **Chat UI:** out of scope; C2 makes the desktop a thin client but the visible chat surface is a separate task.
