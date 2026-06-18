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

### 1. `vmux_agent` feature-gating (no new crate)

Per the decision to keep one crate: feature-gate `vmux_agent` so its pure modules
compile without Bevy, and the daemon depends on it with `default-features = false`.

- `vmux_agent/Cargo.toml`:
  - `[features] default = ["desktop"]`
  - `desktop` enables `bevy`, `bevy_cef`, and the ECS/UI modules.
  - Bevy and `bevy_cef` deps become `optional = true`, pulled in by `desktop`.
- `vmux_agent/src/lib.rs`: gate ECS modules behind `#[cfg(feature = "desktop")]`
  (`plugin`, `client/page`, `systems`, `components`, `run_state`, `session`,
  `snapshot_updater`, `toast`, `echo_plugin`, provider `*_plugin` modules). Leave
  pure modules ungated: `message`, `stream`, `http`, `providers` (the non-plugin
  builders/parsers), `tools`, `variant`, `url`, `exec`, `mcp`.
- Replace the two `Reflect` derives with
  `#[cfg_attr(feature = "desktop", derive(bevy::prelude::Reflect))]` and gate the
  corresponding `use` (`message.rs`, `variant.rs`).
- `vmux_desktop` keeps `vmux_agent = { path = ... }` (default features on) — no
  change.
- `vmux_service` adds `vmux_agent = { path = "../vmux_agent", default-features = false }`.

Verification: `cargo build -p vmux_agent --no-default-features` succeeds and the
dependency tree contains no `bevy`/`bevy_cef`.

`drive_sse` currently takes a `crossbeam_channel::Sender<StreamEvent>`. Keep that
signature; the service loop bridges it to its async output (a small task draining
the crossbeam receiver into the per-session stream), or `drive_sse` gains an
async-sink variant. Either is acceptable; the bridge keeps `drive_sse` untouched.

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

Provide a bevy-free constructor in `vmux_agent::providers` (e.g.
`builtin::page_provider_registry()`) returning the anthropic/openai/mistral
entries from the existing pure builders. Desktop's ECS strategy registration is
rebuilt on top of the same data so there is a single source of truth.

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
        let target = dispatch_from_tool_call(tool.name, tool.args)?
        let result = match target {                     # AgentCommand/Query round-trip
          Command(c) => broker.command(new_id(), c).await,
          Query(q)   => broker.query(new_id(), q).await,
        }
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
}
```

Both the external-MCP-client handler and the internal `run_session` loop call the
same broker; the internal loop maps a `DispatchTarget` to `command`/`query`.
`ReadTerminal` is **not** in the broker — it stays answered locally in `server.rs`
(it needs the `ProcessManager`). No new desktop-side application path: the desktop
keeps applying `AgentCommand`s exactly as today. The broker uses the singleton
`agent_tx` control broadcast; this is unrelated to the per-session *stream*
forwarders in §5.

### 5. Protocol additions (`vmux_service::protocol`)

`ClientMessage` (desktop -> service):

- `SpawnPageAgent { sid, provider, model, cwd, policy }`
- `AgentInput { sid, text }`
- `AgentApprove { sid, call_id, decision }`  (`decision: Allow | Deny`)
- `ClosePageAgent { sid }`
- `AttachPageAgent { sid }` / `DetachPageAgent { sid }`  (mirror `AttachProcess` /
  `DetachProcess`: attach starts a per-session forwarder, detach stops it)

`ServiceMessage` (service -> desktop):

- `AgentDelta { sid, text }`
- `AgentRunState { sid, state }`  (`Streaming | Idle | Errored(String)`)
- `AgentToolStatus { sid, call_id, status }`
- `AgentAwaitingApproval { sid, call_id, name, args_json }`
- `AgentMessagesSnapshot { sid, messages }`
- `AgentToast { sid, ... }`

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
- Delete the now-dead desktop loop modules under the `desktop` feature.
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

**Phase B — `vmux_agent` feature-gating + service runtime.**
Feature-gate `vmux_agent` (`default = ["desktop"]`); confirm
`--no-default-features` is bevy-free. Add the new `ClientMessage`/`ServiceMessage`
variants (§5) with rkyv/serde roundtrip tests. Add `vmux_service::agent` with
`run_session`, the provider registry, and the `AgentSessionManager` (per-session
broadcast + forwarder, mirroring `AttachProcess`); wire `SpawnPageAgent` /
`AttachPageAgent` / `AgentInput` / `AgentApprove` / `ClosePageAgent`; emit stream
messages on each session's broadcast. Not yet consumed by desktop.
Verify: `run_session` unit tests with a mock provider (local SSE fixture) and a
mock broker asserting the full loop (delta -> tool -> result -> continue ->
idle), including approval allow/deny.

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
- **Feature-gate:** CI builds `vmux_agent --no-default-features`; assert no
  bevy in the resolved tree for that build.
- **End to end (manual, Phase C):** real provider turn with a tool call; Activity
  Monitor attribution check.

## Risks and mitigations

- **Daemon idle CPU.** `run_session` must be fully await-driven (no busy polling);
  tasks park on `input_rx.recv()` / SSE I/O. No second Bevy loop is introduced, so
  the project's no-`Continuous` rule is unaffected.
- **Provider extraction churn.** Mitigated by feature-gating instead of moving
  files: the pure modules stay in place; only Bevy derives/imports get cfg-gated.
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
