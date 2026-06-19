# Page Agent Compute Relocation to vmux_service (SP1)

## Goal

Move all Page/API agent compute (provider HTTP streaming, the agentic turn loop,
tool orchestration) out of the `vmux` desktop process and into the `vmux_service`
daemon.

Success criteria:

- During an active Page-agent turn, CPU is attributed to **`vmux_service`** in
  Activity Monitor, not to **`vmux`**. The desktop GUI row stays light.
- The desktop process runs no agent inference loop and holds no agent run-state.
  It sends user input down and renders a stream coming up.
- No behavior regression: prompt -> streamed deltas -> tool calls -> tool results
  -> turn completion all still work end to end.

Non-goals (separate sub-projects, out of scope here):

- SP2: relocate CLI session-file watching (`notify` watcher + discovery/exit
  detection) into the daemon.
- SP3: relocate CLI launch-spec building (exe resolution + MCP arg/env) into the
  daemon.

## Motivation

This is driven by both architecture and perceived cost:

- **Process attribution.** Users accept CPU under a background service or under a
  terminal child process (e.g. a `cargo` build). They do not accept the GUI app
  looking like a hog. The desktop row should read as "just a UI thing".
- **Future-proofing.** Today's *measured* desktop agent cost is modest: CLI
  session watching is OS-event-driven and gated behind a dirty flag
  (`vmux_agent::session`), and Page streaming only runs during an active turn on
  `IoTaskPool` threads. The value grows as Page agents mature (real chat UI,
  long/concurrent sessions). The Page chat UI is still a placeholder
  (`vmux_agent::plugin::page_agent_placeholder_url`), so now is the moment to set
  the data flow before a UI hardcodes desktop-origin state.

## Current architecture

Page agents run entirely inside the desktop Bevy app (`PageAgentPlugin`):

- **State machine** (`run_state::AgentRunState`, polled by Update systems):
  `Idle -> Streaming -> {AwaitingApproval | RunningTool} -> Idle`, with
  `continue_after_tool` re-entering `Streaming` when the last message is a
  `ToolResult` (the agentic loop).
- **Streaming** (`systems/process_input.rs`): builds a `reqwest::Request` via the
  provider's `build_request`, then spawns an `IoTaskPool` task that *creates its
  own current-thread tokio runtime* and runs `http::drive_sse(request, parse_sse,
  tx)`, pushing `StreamEvent`s through a crossbeam channel. `drain_stream` drains
  that channel each frame and mutates `AgentMessages` / `AgentRunState`.
- **Tool dispatch** (`tool_dispatch.rs`): on `IoTaskPool`, resolves a tool call to
  a `vmux_mcp::tools::DispatchTarget`, emits it on a global `EMIT_CHANNEL`, and
  blocks on a oneshot for the `DispatchResult`. A desktop consumer applies the
  target against the ECS and delivers the result.

Two facts make relocation clean:

1. The streaming + tool-resolution code is **provider-pure** (no Bevy):
   `message`, `stream`, `http`, `providers/*`, `tools`. The only Bevy coupling in
   those modules is two `Reflect` derives (`message.rs`, `variant.rs`).
2. The tool round-trip **already exists as an async path in the service.** The
   external `vmux mcp` sidecar connects to the daemon as a `ServiceClient` and
   issues `AgentCommand`/`AgentQuery` over IPC; `server.rs` broadcasts them to the
   desktop subscriber on `agent_tx` and resolves the originator's oneshot from
   `pending_commands`/`pending_queries`. `vmux_mcp::protocol` already maps
   `DispatchTarget::Command -> run_agent_command().await` and
   `DispatchTarget::Query -> run_agent_query().await`.

## Target architecture

```
                  vmux (desktop)                         vmux_service (daemon)
   +--------------------------------------+      +-----------------------------------+
   | chat webview (renders deltas)        |      | run_session(sid) async task       |
   | apply AgentCommand -> ECS (existing) |      |   build_request / drive_sse       |
   | ServiceClient pump (existing)        |      |   parse_sse / dispatch_from_tool  |
   +--------------------------------------+      |   AgentBroker.dispatch(target)    |
        |  ClientMessage  ^  ServiceMessage      +-----------------------------------+
        v                 |                              |          ^
   AgentInput / AgentApprove / SpawnPageAgent     AgentDelta / AgentRunState / ...
        |                 |                              |          |
        +-----------------+------ Unix socket -----------+----------+
                          |
              AgentBroker round-trip (tool calls):
              service broadcasts AgentCommand -> desktop applies -> oneshot result
```

Ownership:

| Concern | Today | After SP1 |
| --- | --- | --- |
| Provider HTTP streaming | desktop `IoTaskPool` | **service** tokio task |
| Agentic turn loop / run-state | desktop ECS | **service** `run_session` |
| Tool resolution + execution | desktop `EMIT_CHANNEL` | **service** resolves; desktop applies via `AgentBroker` round-trip |
| Chat rendering | desktop | desktop (unchanged role) |
| Apply `AgentCommand` to ECS | desktop | desktop (unchanged) |

## Design details

### 1. Relocate agent compute DOWN into `vmux_service` (no new crate)

`vmux_agent` already depends on `vmux_service` (`ServiceClient` + `protocol`), and
so does `vmux_mcp`. `vmux_service` is the low crate. So the daemon **cannot** depend
on `vmux_agent` — that is a `vmux_agent ⇄ vmux_service` package cycle, which Cargo
rejects, and feature-gating does not change the package edge. To run page-agent
compute in the daemon, the pure code must live at `vmux_service`'s layer, so we
**move it down** rather than depend upward.

Relocate from `vmux_agent` into `vmux_service` (these modules are already
Bevy-free — see §"Current architecture"): `message`, `stream`, `http`,
`providers/{anthropic,openai,mistral,openai_shared,builtin}`, and `tools`
(`mcp_tool_defs`). `vmux_service` gains their deps (`reqwest`, `crossbeam-channel`,
`futures-util`); these are already `vmux_agent` deps.

Tool *resolution* (`DispatchTarget` + `dispatch_from_tool_call`) **stays in
`vmux_mcp`** — it is entangled with the MCP tool registry (`McpParamTool`,
`vmux_macro`, `vmux_command`) and moving it would drag that subtree into the
daemon. Instead the service treats a tool call as opaque: `run_session` forwards
the raw `{ name, args_json }` to the desktop over a round-trip; the desktop (which
has `vmux_mcp` + the ECS) resolves via `dispatch_from_tool_call` and applies it,
returning the result. This reuses the existing desktop apply path. The tool
*schema* the model sees (`mcp_tool_defs` — self-contained static data) moves down
with the provider modules so the relocated `build_request` can include it.

`vmux_agent` stays a desktop/Bevy crate (no feature-gating). In Phase B it repoints
its imports to the new `vmux_service` locations and keeps running its ECS loop;
Phase C deletes that loop and turns `vmux_agent` into the thin client.

`drive_sse` keeps its `crossbeam_channel::Sender<StreamEvent>` signature; the
service loop bridges the crossbeam receiver into each session's async stream.

### 2. Provider registry in the service

Desktop registers providers as ECS entities (`Strategy` components indexed by
`PageStrategyIndex`). The service needs the same mapping as plain data:

```rust
struct PageProvider {
    build_request: BuildRequestFn,   // fn(&str,&[Message],&[ToolDef],&str)->reqwest::Request
    parse_sse: ParseSseFn,           // fn(&str)->Option<StreamEvent>
    env_var: &'static str,
}
// HashMap<(provider, model), PageProvider>
```

Provide the registry constructor in the relocated `vmux_service::providers` (e.g.
`builtin::page_provider_registry()`) returning the anthropic/openai/mistral entries
from the moved pure builders. Desktop's ECS strategy registration is rebuilt on top
of the same data so there is a single source of truth.

### 3. Service-side session runtime (`run_session`)

New module `vmux_service::agent` (native-only). One async task per Page session:

```text
run_session(sid, session, mut input_rx, broker, stream_tx, registry):
  messages: Vec<Message> = session.seed
  loop:                                # await user turns
    match input_rx.recv().await:
      Input(text) => messages.push(User(text))
      Approve(..) when not awaiting => ignore
      Close => return
    loop:                              # agentic turn
      let p = registry.get(provider, model) else { emit RunState(Errored); break }
      let key = env(p.env_var)?        # emit Errored on missing
      let req = (p.build_request)(model, &messages, &tools, &key)
      emit RunState(Streaming)
      for ev in drive_sse(req, p.parse_sse):    # via crossbeam->async bridge
        TextDelta(t)     => messages tail += t; emit AgentDelta(sid, t)
        ToolUse*         => accumulate partial
        StopTurn/Error   => record outcome
      if let Some(tool) = pending_tool:
        emit ToolStatus(Pending)
        if !policy.auto(tool.name):
          emit AwaitingApproval(sid, tool); 
          match input_rx.recv().await { Approve(deny) => push ToolResult(denied); continue
                                        Approve(allow) => {} ; Close => return }
        # forward the raw tool call to the desktop, which resolves + applies it
        let result = broker.tool_call(new_id(), tool.name, tool.args_json).await
        messages.push(ToolResult(result)); 
        emit MessagesSnapshot(sid); continue            # loop back to provider
      else:
        emit RunState(Idle); break                      # turn complete
```

- `policy` (auto-approval set) lives in the session, seeded by `SpawnPageAgent`
  (from agent settings). Desktop renders approval prompts and sends decisions; it
  does not hold run-state.
- `stream_tx` is the session's own `broadcast::Sender<ServiceMessage>` — one per
  session, mirroring how each terminal `Process` owns a broadcast. The desktop
  receives a session's stream by *attaching* to it (§5), exactly like
  `AttachProcess`; the daemon spawns a per-session forwarder that pumps that
  session's broadcast to the socket and drops it on detach.
- `AgentSessionManager`: a registry `HashMap<Sid, SessionHandle { input_tx,
  stream_tx, messages, join_handle }>`, parallel to `ProcessManager` (sessions are
  not PTYs, so they don't live in `ProcessManager`, but they reuse its
  subscribe/forward shape). `SpawnPageAgent` inserts + spawns `run_session`;
  `ClosePageAgent` drops + aborts.
- The manager retains `messages` per session so a freshly-attaching desktop can
  rebuild chat via `AgentMessagesSnapshot` — the forwarder sends a snapshot first,
  then live deltas, the same way a terminal sends `Snapshot` before `ViewportPatch`.

### 4. `AgentBroker` (tool round-trip refactor)

Factor the inline `AgentCommand`/`AgentQuery` handling in `server.rs`
(~L420-472 command, ~L522-591 query) into a reusable type:

```rust
#[derive(Clone)]
struct AgentBroker {
    agent_tx: broadcast::Sender<ServiceMessage>,   // singleton control channel to desktop
    pending_commands: PendingCommands,
    pending_queries: PendingQueries,
}
impl AgentBroker {
    // Err(message) -> ServiceMessage::Error; Ok(result) -> the success result.
    async fn command(&self, id: AgentRequestId, c: AgentCommand) -> Result<AgentCommandResult, String>;
    async fn query(&self, id: AgentRequestId, q: AgentQuery) -> Result<AgentQueryResult, String>;
    // Phase B3: forward an opaque tool call; desktop resolves (vmux_mcp) + applies.
    async fn tool_call(&self, id: AgentRequestId, name: String, args_json: String) -> Result<ToolResult, String>;
}
```

`command`/`query` exist today (Phase A). `tool_call` is added in B3 with its own
`pending_tool_calls` map + `ServiceMessage::AgentToolCall` / `ClientMessage::
AgentToolResult` round-trip, mirroring `command`. The external-MCP handler still
uses `command`/`query`; the internal `run_session` uses `tool_call` (it has no
`vmux_mcp`, so it cannot resolve locally). `ReadTerminal` stays answered locally in
`server.rs`. The broker uses the singleton `agent_tx` control broadcast; this is
unrelated to the per-session *stream* forwarders in §5.

### 5. Protocol additions (`vmux_service::protocol`)

`ClientMessage` (desktop -> service):

- `SpawnPageAgent { sid, provider, model, cwd, policy }`
- `AgentInput { sid, text }`
- `AgentApprove { sid, call_id, decision }`  (`decision: Allow | Deny`)
- `ClosePageAgent { sid }`
- `AttachPageAgent { sid }` / `DetachPageAgent { sid }`  (mirror `AttachProcess` /
  `DetachProcess`: attach starts a per-session forwarder, detach stops it)
- `AgentToolResult { request_id, content, is_error }`  (desktop's resolved +
  applied tool result, returned to the session loop via the broker)

`ServiceMessage` (service -> desktop):

- `AgentDelta { sid, text }`
- `AgentRunState { sid, state }`  (`Streaming | Idle | Errored(String)`)
- `AgentToolStatus { sid, call_id, status }`
- `AgentAwaitingApproval { sid, call_id, name, args_json }`
- `AgentMessagesSnapshot { sid, messages }`
- `AgentToast { sid, ... }`
- `AgentToolCall { request_id, name, args_json }`  (raw tool call to the desktop to
  resolve via `vmux_mcp` + apply; rides the singleton `agent_tx` control broadcast,
  not the per-session stream)

All new types derive the existing rkyv + serde framing used by the protocol.
Message/AssistantBlock cross the boundary by serializing the pure
`vmux_agent::message` types (already serde) or protocol mirrors.

These `Agent*` stream messages travel the **per-session forwarder** started by
`AttachPageAgent` — not the singleton `agent_tx` control broadcast. `agent_tx`
stays exclusively the `AgentCommand`/`AgentQuery` round-trip channel (Phase A).
This keeps N concurrent page-agent streams independent and reuses the terminal's
proven lag/backpressure handling (`broadcast::error::RecvError::Lagged`).

### 6. Desktop client changes

- Replace the Page systems (`process_input`, `drain_stream`, `dispatch_tool`,
  `continue_after_tool`, `tool_dispatch::EMIT_CHANNEL`) with:
  - on opening a Page-agent view, send `AttachPageAgent { sid }` (like the terminal
    sends `AttachProcess`); send `DetachPageAgent { sid }` when it closes,
  - a system that turns `PendingUserInput` into `ClientMessage::AgentInput`,
  - a system that drains the forwarded agent-stream `ServiceMessage`s from the
    existing `ServiceClient` pump and re-emits them to the chat webview (reusing the
    `AgentDelta`/`AgentToast` bin-event emitters),
  - approval UI -> `ClientMessage::AgentApprove`.
- `SpawnAgentInStackRequest` for a Page provider sends `SpawnPageAgent` instead of
  constructing a local session.
- Delete the now-dead desktop loop modules (`process_input`, `drain_stream`,
  `dispatch_tool`, `continue_after_tool`, `tool_dispatch`); `vmux_agent` keeps only
  thin-client systems + the chat view.
- The CEF/terminal/CLI agent paths are untouched in SP1.

## Phased delivery

Each phase is one PR, builds green (fmt + clippy + tests), and is runtime-verified
before the next.

**Phase A — AgentBroker extraction. (done)**
Factor the inline `AgentCommand`/`AgentQuery` round-trip out of `server.rs` into a
reusable `AgentBroker`; route the external-MCP path through it. Behavior-preserving
(orphaned in-flight entries now clear via the existing 5s broker timeout rather
than per-connection disconnect cleanup). The new protocol variants are deferred to
Phase B, where they are constructed — adding them here would be dead code.
Verify: broker unit tests (no-subscriber / resolve / timeout); full `vmux_service`
suite green; clippy + fmt clean.

**Phase B — relocate compute into the service + add the session runtime.** Three
sub-PRs:
- **B1:** move the self-contained provider modules — `message`, `stream`, `http`
  (relocating the `ParseSse` fn-pointer alias out of ECS `strategy_components` into
  `stream`), `providers/*`, and `tools` (`mcp_tool_defs`) — from `vmux_agent` into
  `vmux_service` (which gains `reqwest`/`crossbeam-channel`/`futures-util`). Repoint
  `vmux_agent` imports to the new `vmux_service` paths; its ECS loop still runs.
  Tool *resolution* stays in `vmux_mcp` (untouched). Verify: `vmux_agent`,
  `vmux_service`, `vmux_mcp` build + test green; no new dependency cycle.
- **B2:** add the new `ClientMessage`/`ServiceMessage` variants (§5, rkyv/serde
  roundtrip tests), including the `AgentToolCall`/`AgentToolResult` round-trip and
  `AgentBroker::tool_call`; add `vmux_service::agent` with `run_session`, the
  provider registry, and the `AgentSessionManager` (per-session broadcast +
  forwarder, mirroring `AttachProcess`); wire `SpawnPageAgent` / `AttachPageAgent` /
  `AgentInput` / `AgentApprove` / `ClosePageAgent`; add the desktop handler that
  resolves + applies an `AgentToolCall` via `vmux_mcp`. Not yet consumed by the
  desktop chat UI. Verify: `run_session` unit tests with a mock provider (canned SSE
  fixture) + mock broker covering the full loop (delta -> tool -> result -> continue
  -> idle), approval allow/deny, missing-API-key, provider error.

**Phase C — Desktop cutover.**
Switch the desktop to send-down + stream-consume + webview re-emit; delete the
dead loop. Page providers spawn via `SpawnPageAgent`.
Verify end to end in the running app: send a prompt, observe streamed deltas, a
tool call applied via the round-trip, turn completion — and confirm in Activity
Monitor that turn CPU lands on `vmux_service`, not `vmux`.

## Testing strategy

- **Protocol:** rkyv + serde roundtrip for every new variant (mirror existing
  `protocol.rs` tests).
- **Broker:** async test issuing a command/query through `AgentBroker` against a
  fake desktop subscriber, asserting the oneshot resolves.
- **`run_session`:** drive the loop with a mock provider serving a canned SSE
  stream and a mock broker; assert emitted stream messages and final state for:
  plain text turn, single tool turn, multi-tool agentic loop, approval-deny,
  missing-API-key error, provider error.
- **Layering:** after the moves, `vmux_service` still has no dependency on
  `vmux_mcp`/`vmux_agent` (no cycle); `vmux_mcp` is untouched (tool resolution stays
  there); `vmux_agent` compiles against the relocated `vmux_service` modules.
- **End to end (manual, Phase C):** real provider turn with a tool call; Activity
  Monitor attribution check.

## Risks and mitigations

- **Daemon idle CPU.** `run_session` must be fully await-driven (no busy polling);
  tasks park on `input_rx.recv()` / SSE I/O. No second Bevy loop is introduced, so
  the project's no-`Continuous` rule is unaffected.
- **Relocation churn / cycle.** `vmux_service` cannot depend on `vmux_agent`
  (existing `vmux_agent → vmux_service` edge), so the pure modules move *down* into
  `vmux_service` and `vmux_agent` repoints imports. Mitigated by `git mv` + import
  fixes per crate, landed as small B1/B2 PRs that each keep all three crates green
  before any new runtime code (B3) is added.
- **Stream/UI protocol rework later.** The chat UI is greenfield, so it is built
  against the service stream from the start — no desktop-first throwaway.
- **CEF build fragility / large rebuilds.** Implement directly in a warm target
  dir; do not subagent-drive. Land A/B/C as separate PRs to keep diffs reviewable.
- **Reconnect/ordering.** `AgentMessagesSnapshot` lets a resubscribing desktop
  rebuild state; deltas are advisory between snapshots.

## Open questions

- Exact home of the auto-approval policy source (agent settings read in the daemon
  vs. passed in `SpawnPageAgent`). Leaning: passed at spawn from desktop-read
  settings to avoid the daemon depending on settings load order.
- Whether `drive_sse` gets an async-sink variant or keeps the crossbeam bridge.
  Leaning: keep the bridge in Phase B to minimize changes to proven code.
