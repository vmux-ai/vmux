# vmux as an ACP Host — Design

- **Date:** 2026-06-30
- **Status:** Approved (direction); spec under review
- **Scope:** Milestones B + C (build + prove the host), then D (retire Page + Cli; vmux becomes ACP-only)
- **Branch/worktree:** `feat/acp-host` / `.worktrees/acp-host`

## 1. Goal

vmux adopts ACP (Agent Client Protocol) as the integration protocol for **external coding
agents** — the model Zed uses. vmux implements the ACP **Client** role (ACP inverts naming:
the host is the `Client`, the agent is the spawned subprocess); every external agent is an ACP
agent driven through one host. Mistral's local stack (`vibe-acp`) is the **first consumer**,
not a thing to embed — the host is agent-agnostic.

This **replaces vmux's current bespoke, per-CLI integration.** Today claude/codex/vibe are
spawned as raw PTY TUIs whose state vmux recovers by scraping session logs + filesystem hooks +
an MCP sidecar (`AgentVariant::Cli`). Under the Zed model they instead run as ACP adapters
(`@zed-industries/claude-code-acp`, `@zed-industries/codex-acp`, `vibe-acp`) driven over a real
bidirectional protocol, and the bespoke machinery retires (Milestone D, §10).

**vmux is ACP-only.** There is no agent-integration taxonomy and no `AgentVariant`: every agent
is an ACP agent, reached at `vmux://agent/<id>`. Both current paths retire — the `Cli` raw-PTY
path **and** the `Page` provider-direct SSE loop (Anthropic/OpenAI/Mistral; vmux's own-agent
analog of Zed's native thread). Provider chat re-enters as an ACP agent (vibe-acp /
claude-code-acp / a provider→ACP adapter). **This plan (B+C) builds the host without touching
Page/Cli;** the retirement is Milestone D once the host is proven.

ACP sessions are identified by a dedicated **`AcpSession`** component (not a variant value); the
old variant-carrying `components::AgentSession` belongs to Page/Cli and is removed in D.

- **Milestone B** — agent-agnostic ACP-native host: `session/update` streaming,
  `request_permission`, fs read/write with editor diffs, terminal lifecycle → real panes.
- **Milestone C** — layer the `vmux_mcp` toolset onto the ACP session (browser/editor/history
  tools) via `newSession.mcpServers`.
- **Milestone D** — migrate claude/codex/vibe onto ACP adapters and **retire both the `Cli` and
  `Page` machinery + the `AgentVariant` enum** (vmux becomes ACP-only), after B+C prove the host
  at runtime (§10).

B + C are built together and sequenced so the tree compiles and runs at every step (§10); D
follows once the host is proven.

## 2. Background: ACP 1.0 is a rewrite (the binding constraints)

Verified against the extracted crate sources for `agent-client-protocol 1.0.1` +
`agent-client-protocol-schema 1.1.0` + `agent-client-protocol-rmcp 1.0.1`.

1. **No `Client` trait to implement.** `Client`/`Agent` are zero-sized role markers. You build
   a connection from `Client.builder()`, register handlers with `.on_receive_request(...)` /
   `.on_receive_notification(...)`, then `.connect_with(transport, |cx| async { ... })`. You
   call the agent via `cx.send_request(req).block_task().await`.
2. **Wire types live in `agent-client-protocol-schema`** (pinned `=1.1.0` by core 1.0.1),
   re-exported as `agent_client_protocol::schema::v1::*`. All `serde` camelCase. Enums use
   mixed tagging (`ContentBlock`/`McpServer`/`ToolCallContent` → `tag="type"`; `SessionUpdate`
   → `tag="sessionUpdate"`; `RequestPermissionOutcome` → `tag="outcome"`). Many `#[non_exhaustive]`
   → always match with a wildcard arm.
3. **No `-tokio` at 1.0.** `agent-client-protocol-tokio 0.11.1` depends on acp **0.11**, a
   different semver-major; it cannot co-exist with acp 1.0.1. We **drop `-tokio`** and inline
   the transport (~40 lines) using core primitives:
   ```rust
   // child = tokio::process::Command spawn with piped stdin/stdout
   use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};
   let transport = agent_client_protocol::ByteStreams::new(
       child.stdin.take().unwrap().compat_write(),
       child.stdout.take().unwrap().compat(),
   );
   ```
4. **rmcp not required.** Scope C reuses vmux's existing `vmux mcp --anchor` stdio sidecar
   (§11), so we do **not** add `agent-client-protocol-rmcp`. Fewer deps ⇒ less CEF-rebuild
   surface. (Recorded as an alternative considered, not chosen.)

### Host call surface (agent-side requests we send)

```rust
cx.send_request(InitializeRequest::new(ProtocolVersion::V1)        // + client_capabilities
    /* fs.read=true, fs.write=true, terminal=true */).block_task().await?;
let sess = cx.send_request(NewSessionRequest::new(cwd) /* .mcp_servers = [...] */)
    .block_task().await?;                                          // -> NewSessionResponse{ session_id, .. }
let resp = cx.send_request(PromptRequest::new(sess.session_id.clone(), prompt_blocks))
    .block_task().await?;                                          // -> PromptResponse{ stop_reason, .. }
cx.send_notification(CancelNotification::new(session_id));         // session/cancel
```

`StopReason` ∈ `{ EndTurn, MaxTokens, MaxTurnRequests, Refusal, Cancelled }`.

### Host handler surface (client-side requests/notifications we answer)

`on_receive_request` for: `RequestPermissionRequest`, `ReadTextFileRequest`,
`WriteTextFileRequest`, `CreateTerminalRequest`, `TerminalOutputRequest`,
`WaitForTerminalExitRequest`, `ReleaseTerminalRequest`, `KillTerminalRequest`.
`on_receive_notification` for: `SessionNotification` (the `session/update` stream).

## 3. Architecture overview

vmux already splits "owns subprocesses/PTYs + async runtime" (the **`vmux_service` daemon**)
from "renders panes" (the GUI: `vmux_terminal`, `vmux_agent`, …). An ACP agent is a
subprocess that *also* spawns terminals and reads/writes files — it belongs in the daemon,
exactly where `AgentSessionManager::run_session` already drives streaming LLM turns.

```
GUI process                                   vmux_service daemon
-----------                                   -------------------
AcpSession{agent_id,sid,cwd} --SpawnAcpAgent-> AcpSessionManager
  (+ reuse AgentMessages/RunState/Approval)      ├─ spawn ACP subprocess (tokio::process)
                                                 ├─ ByteStreams transport + Client.builder()
PendingUserInput ----------AgentInput--------->  ├─ cx.send_request(Prompt/Initialize/NewSession)
AgentApprovalReply --------AgentApprove------->  ├─ handlers: permission/fs/terminal
                                                 └─ projector: session/update -> Message
consume_page_agent_stream <--AgentDelta---------┘   (emits existing ServiceMessages)
  (UNCHANGED pump)        <--AgentMessagesSnapshot
                          <--AgentAwaitingApproval
                          <--AgentRunStatusChanged
                          <--AcpTerminalCreated (NEW: GUI spawns visible pane)
                          <--AcpProposedDiff   (NEW: editor overlay)
```

**Reuse, don't rebuild.** The daemon emits the *existing* `ServiceMessage::AgentDelta /
AgentMessagesSnapshot / AgentAwaitingApproval / AgentRunStatusChanged`. The existing pump in
`vmux_terminal/src/plugin.rs:1280` already translates those into `PageAgent*` messages, and
`consume_page_agent_stream` (`client/page/plugin.rs:144`) already updates `AgentMessages` /
`AgentRunState`. The ACP path inherits all of it. ACP entities carry `AgentMessages` /
`AgentRunState` / `AgentApprovalPolicy` and register their `sid → entity` in the shared
`AgentSessionToEntity` map so the consumer finds them; generalize the consumer's query if it
hard-requires `components::AgentSession` (use `Or<(&AgentSession, &AcpSession)>` or a shared
marker).

### Crate map

| Crate | Change |
|---|---|
| `vmux_service` | **New** `AcpSessionManager` + ACP client (transport, handlers, projector); new `SpawnAcpAgent` ClientMessage + `AcpTerminalCreated`/`AcpProposedDiff` ServiceMessages; route `AgentInput`/`AgentApprove`/`ClosePageAgent` by sid to ACP when applicable |
| `vmux_agent` | new `AcpSession` component; `AcpAgentPlugin` (spawn/input/close systems mirroring Page); URL parse `vmux://agent/<id>`; ACP-aware approval routing; config loading. **No `AgentVariant::Acp`** — ACP is not a variant |
| `vmux_editor` | **New** proposed-edit diff overlay primitive (`ProposedEdit` component + `ProposedDiff*` wire events + accept/reject) |
| `vmux_core` | shared `AcpAgentConfig` serde type; `agent.acp` settings section; Bevy messages `AcpTerminalCreated`/`AcpProposedDiff` |
| `vmux_cli` / `vmux_mcp` | tool-filter flag on the `vmux mcp` sidecar (`--omit-terminal-tools`) for scope C |

No new workspace crate (project rule). Shared contracts go through `vmux_core` serde types.

### Dependencies

- `agent-client-protocol = "1.0.1"` (in `vmux_service`).
- `tokio_util` compat (already transitively present; confirm feature `compat`).
- **Not** added: `agent-client-protocol-tokio`, `agent-client-protocol-rmcp`.

## 4. Launch + session lifecycle

1. **Config.** `agent.acp` settings section lists agents; built-in defaults shipped in the
   embedded `settings.ron` (no auto-seed of the user file — per project rule):
   ```ron
   agent: (
     acp: [
       (id: "claude-code-acp", name: "Claude Code (ACP)", command: "npx",
        args: ["-y", "@zed-industries/claude-code-acp@latest"]),
       (id: "gemini", name: "Gemini CLI (ACP)", command: "npx",
        args: ["-y", "--", "@google/gemini-cli@latest", "--experimental-acp"]),
     ],
   )
   ```
   `AcpAgentConfig { id, name, command, args, env: Vec<(String,String)>, cwd: Option<PathBuf> }`
   in `vmux_core`. Only npx-spawnable agents are seeded (zero setup); `vibe-acp` is added by the
   user since its `uv run --directory <vibe-dir>` path is machine-specific.
2. **URL.** `vmux://agent/<agent-id>` (new) or `vmux://agent/<agent-id>/<sid>` (resume). **No
   `/acp/` segment** — every `vmux://agent/<id>` is ACP. `AgentUrl` parsing (`url.rs`): `<id>`
   resolves against the `agent.acp` config ids (precedence over the legacy Page/Cli forms, which
   are removed in D).
3. **Attach pane.** Insert a dedicated **`AcpSession { agent_id, sid, cwd }`** component plus the
   reused `AgentMessages`, `AgentApprovalPolicy`, `AgentRunState::default()`; register `sid →
   entity` in `AgentSessionToEntity`. The agent pane is the **same chat UI** (it renders
   `AgentMessages`/`AgentRunState`). No `AgentVariant`, no `AgentKind`.
4. **Spawn.** `Added<AcpSession>` → `spawn_acp_session_on_add` →
   `ClientMessage::SpawnAcpAgent { sid, agent_id, command, args, env, cwd }`. Daemon spawns
   subprocess → `initialize` (advertise caps) → mint an **anchor `ProcessId`** → build
   `mcp_servers` itself from that anchor (§9) → `session/new(cwd, mcp_servers)` → store
   `session_id`.
5. **Prompt.** User text → `PendingUserInput` → `send_acp_input` → reuse
   `ClientMessage::AgentInput { sid, text }`. Daemon enqueues into the ACP session's input
   channel; the driver sends `PromptRequest`. Prompts are **queued and drained serially** per
   session (ACP agents reject concurrent prompts — mirror `client.ts`'s prompt queue).
6. **Close.** Entity removed → `close_acp_session_on_remove` → reuse
   `ClientMessage::ClosePageAgent { sid }` → daemon sends `CancelNotification` and drops the
   connection.

Only **one** new ClientMessage (`SpawnAcpAgent`); input/approve/close reuse the Page wire by
sid, dispatched in the daemon to the right manager.

## 5. Daemon: `AcpSessionManager` (mirrors `AgentSessionManager`)

`crates/vmux_service/src/acp.rs` (filename-module pattern; no mod.rs).

```rust
struct AcpSessionHandle {
    input_tx: mpsc::UnboundedSender<AcpInput>,   // User(String) | Approve{call_id, decision} | Close
    stream_tx: broadcast::Sender<ServiceMessage>,
    session_id: SessionId,                        // ACP session id (filled after session/new)
    anchor: ProcessId,                            // for scope C MCP tool calls
    terminals: Mutex<HashMap<TerminalId, ProcessId>>,
    pending_perms: Mutex<HashMap<String /*call_id*/, oneshot::Sender<PermissionOptionId>>>,
    pending_writes: Mutex<HashMap<String /*call_id*/, oneshot::Sender<bool>>>, // accept/reject for proposed edits
    messages: Arc<Mutex<Vec<Message>>>,
    task: JoinHandle<()>,
}
```

`spawn()` → `tokio::spawn(run_acp_session(...))`. The driver:

```rust
async fn run_acp_session(cfg, sid, anchor, stream_tx, mut input_rx, mcp_servers) {
    let child = tokio::process::Command::new(cfg.command).args(cfg.args)
        .envs(cfg.env).stdin(piped).stdout(piped).stderr(piped).spawn()?;
    let transport = ByteStreams::new(child.stdin.compat_write(), child.stdout.compat());

    Client.builder()
      .on_receive_request(perm_handler(stream_tx, pending_perms), on_receive_request!())
      .on_receive_request(read_handler(stream_tx, cwd), on_receive_request!())
      .on_receive_request(write_handler(stream_tx, pending_writes, cwd), on_receive_request!())
      .on_receive_request(term_create_handler(stream_tx, terminals, anchor), on_receive_request!())
      .on_receive_request(term_output_handler(terminals), on_receive_request!())
      .on_receive_request(term_wait_handler(terminals), on_receive_request!())
      .on_receive_request(term_kill_handler(terminals), on_receive_request!())
      .on_receive_request(term_release_handler(terminals), on_receive_request!())
      .on_receive_notification(session_update_handler(stream_tx, projector), on_receive_notification!())
      .connect_with(transport, |cx| async move {
          cx.send_request(InitializeRequest::new(ProtocolVersion::V1)
              .client_capabilities(caps_fs_and_terminal())).block_task().await?;
          let s = cx.send_request(NewSessionRequest::new(cwd).mcp_servers(mcp_servers))
              .block_task().await?;
          set_session_id(s.session_id);
          emit(stream_tx, ServiceMessage::AgentRunStatusChanged{ sid, status: Idle });
          // serial input loop
          while let Some(input) = input_rx.recv().await {
              match input {
                  AcpInput::User(text) => {
                      emit(RunStatus::Streaming);
                      let r = cx.send_request(PromptRequest::new(s.session_id.clone(),
                                  vec![ContentBlock::Text(TextContent::new(text))]))
                              .block_task().await;
                      emit(status_from_stop_reason(r));
                  }
                  AcpInput::Approve{call_id, decision} => resolve_pending(call_id, decision),
                  AcpInput::Close => { cx.send_notification(CancelNotification::new(s.session_id.clone())); break; }
              }
          }
          Ok(())
      }).await
}
```

The handlers and the input loop share the per-session maps via `Arc`. Permission and
proposed-write handlers **park on a `oneshot`** resolved by an incoming `Approve` input, so a
blocked handler never stalls the reactor (each handler is its own future).

### Projector: `SessionUpdate` → `Message`

`crates/vmux_service/src/acp/projector.rs`. Port the logic of the reference `projector.ts`.
Produces the same `Message`/`AssistantBlock` vec the Page chat renderer already consumes, then
emits `ServiceMessage::AgentDelta` (incremental text) and `AgentMessagesSnapshot` (full state).

| `SessionUpdate` variant | Projection |
|---|---|
| `AgentMessageChunk(ContentChunk)` | append text to current assistant message → `AgentDelta` |
| `AgentThoughtChunk` | reasoning/thought block |
| `UserMessageChunk` | skip if it echoes a locally-authored prompt; else append user msg |
| `ToolCall(ToolCall)` | upsert tool block (by `tool_call_id`); if `content` has `Diff` → `AcpProposedDiff`; if `Terminal{terminal_id}` → link the live pane |
| `ToolCallUpdate` | patch the tool block (status/content/raw_output) |
| `Plan` / `PlanUpdate` / `PlanRemoved` | todo-style block (reference renders plan as a `todo` tool call) |
| `AvailableCommandsUpdate` / `CurrentModeUpdate` / `ConfigOptionUpdate` | session capabilities (B: store/log; surface later) |
| `SessionInfoUpdate` / `UsageUpdate` | ignore/log (B) |

`ContentBlock` handling: `Text` now; `Image`/`Audio`/`ResourceLink`/`Resource` deferred
(log + skip) for B.

## 6. Protocol additions (`vmux_service/src/protocol.rs`)

```rust
// ClientMessage (GUI -> daemon)
SpawnAcpAgent { sid: String, agent_id: String, command: String, args: Vec<String>,
                env: Vec<(String,String)>, cwd: PathBuf },  // daemon builds mcp_servers itself (§9)
// reuse: AgentInput{sid,text}, AgentApprove{sid,call_id,decision}, ClosePageAgent{sid}

// ServiceMessage (daemon -> GUI)
AcpTerminalCreated { sid: String, terminal_id: String, process_id: ProcessId,
                     command: String, args: Vec<String>, cwd: Option<PathBuf> },
AcpProposedDiff { sid: String, call_id: String, path: PathBuf,
                  old_text: Option<String>, new_text: String },
// reuse: AgentDelta, AgentMessagesSnapshot, AgentAwaitingApproval, AgentRunStatusChanged
```

The daemon dispatches `AgentInput`/`AgentApprove`/`ClosePageAgent` to the ACP manager when the
sid belongs to an ACP session (a sid→manager registry), else to the existing page manager.

## 7. Callback → vmux primitive (detailed)

### 7.1 `session/update` → chat
Projector (§5) → existing `AgentDelta`/`AgentMessagesSnapshot` → existing pump →
`AgentMessages`/`AgentRunState`. No GUI changes.

### 7.2 `request_permission` → `approval.rs`
- Handler emits `ServiceMessage::AgentAwaitingApproval { sid, call_id, name, args_json }` (existing)
  and registers a `oneshot` in `pending_perms[call_id]`.
- Existing pump → `AgentRunState::AwaitingApproval` + `commands.trigger(AgentApprovalRequest)`.
- Page chat shows allow/allow-always/deny buttons → `AgentApprovalReply { session, call_id, decision }`.
- **ACP-aware approval routing.** `approval.rs:handle_approval_reply` (`systems/approval.rs:9`)
  branches on the presence of `AcpSession` on the entity: for ACP, send
  `ClientMessage::AgentApprove { sid, call_id, decision }` (reusing the wire) instead of resuming
  an SSE turn. `AllowAlways` still records the tool name in `AgentApprovalPolicy.auto`; a small
  ACP system auto-replies `Allow` when an incoming `AgentApprovalRequest.name ∈ policy.auto`
  (the ACP agent re-asks each time).
- The **wire `ApprovalDecision` is `Allow`/`Deny` only** (`AllowAlways` collapses to `Allow` at
  `approval.rs:30`). Daemon maps it → `PermissionOptionId` by `PermissionOptionKind`:
  `Allow → AllowOnce` (fallback `AllowAlways`), `Deny → RejectOnce` (fallback `RejectAlways`),
  else first option. Resolves `pending_perms[call_id]`; handler returns
  `RequestPermissionResponse::new(Selected(SelectedPermissionOutcome::new(option_id)))`.

### 7.3 fs read/write
- `read_text_file`: read file (sandbox to session cwd; reject paths outside, mirroring the
  reference `assertInside`), honor `line`/`limit`. Emit `FileTouched{Read}`-equivalent so the
  file opens beside the agent pane (gated by `settings.agent.follow_files`). Return
  `ReadTextFileResponse::new(content)`.
- `write_text_file`: see §7.4 (gated apply).

### 7.4 Proposed-edit overlay (new `vmux_editor` primitive)
ACP edits surface in two coordinated ways:
- **`ToolCall` with `ToolCallContent::Diff { path, old_text, new_text }`** (in `session/update`) →
  `ServiceMessage::AcpProposedDiff` → GUI opens the file in an editor pane beside the agent and
  shows a **pending** diff overlay (old vs proposed).
- **`request_permission`** for that edit tool → the accept/reject gate (§7.2). Accept → the agent
  proceeds; reject → it does not.
- **`write_text_file`** → applies bytes to disk (after permission), clears the overlay; the
  editor auto-reloads and the working-tree `vmux_git` diff then reflects the change.

New in `vmux_editor`:
- `ProposedEdit { call_id, old_text: Option<String>, new_text: String }` component on the `FileView`.
- `ProposedDiffViewportEvent` (+ `show_proposed_diff` page signal), modeled on
  `vmux_git`'s `DiffLine` / `GitDiffViewportEvent` (`vmux_git/src/event.rs:31,47`).
- Accept/reject controls in the editor page emit `AgentApprovalReply` tied to `call_id`
  (so the editor overlay and the chat approval are the same decision).

### 7.5 Terminal lifecycle → real panes (daemon owns PTYs)
The daemon already implements `CreateProcess`/`ProcessInput`/`ProcessExited`
(`protocol.rs:315/330/551`). ACP terminal handlers are near-direct passthroughs **plus** a GUI
notification to show the pane:
- `create_terminal` → mint `ProcessId`, spawn via the daemon's own process API
  (shell-wrap when `args` empty: `["-lc", command]`), store `terminals[terminal_id]=process_id`,
  emit `AcpTerminalCreated`. GUI handles it with a `TerminalStackSpawnRequest { pane: <beside
  agent>, process_id: Some(pid), activate }` (`vmux_terminal/src/plugin.rs:276`,
  `PlacementMode::Auto` → stacks-over-splits, agent pane stays priority). Return
  `CreateTerminalResponse::new(terminal_id)`.
- `terminal_output` → read the PTY buffer (daemon has `process.full_text()` /
  `AgentQuery::ReadTerminal` path), return `{ output, truncated, exit_status }`.
- `wait_for_terminal_exit` → resolve on `ProcessExited`; return `TerminalExitStatus{exit_code,signal}`.
- `kill_terminal` → `KillProcess`. `release_terminal` → drop the mapping (kill if running).

Result: ACP terminals are **watchable** (live `ViewportPatch` to the visible pane) and
**takeoverable** (PTY input is focus-routed — identical to a human-typed terminal).

## 8. Double-exposure resolution

`vmux_mcp` already exposes the full terminal lifecycle as tools (`run`, `read_terminal`,
`terminal_send`, `terminal_clear` — `vmux_mcp/src/tools.rs:410+`). ACP also has native
terminals. To avoid handing the agent two names for one capability:

- **Advertise ACP `terminal:true`** and route native terminals to real panes (§7.5).
- The vmux_mcp toolset handed to ACP agents (scope C) **omits** the four terminal tools and
  keeps browser/editor/history — which is exactly what the C scope enumerates.

This matches the team's earlier consolidation (the removed `new_terminal_tab`/`run_shell`/
`in_pane` tools, asserted gone in `tools.rs:1130`).

## 9. Scope C: vmux_mcp toolset on the ACP session

- `newSession.mcp_servers = [ McpServer::Stdio(McpServerStdio { name: "vmux", command: "vmux",
  args: ["mcp", "--anchor", <acp_pid>, "--profile", <name>, "--omit-terminal-tools"], env }) ]`.
- The agent spawns the sidecar; its tool calls flow back over the **anchor** to the daemon →
  `vmux_mcp::tools::dispatch_with_anchor` → `AgentCommand`/`AgentQuery` → panes. This is the
  exact path `lechat_bridge.rs` already uses (`vmux_desktop/src/lechat_bridge.rs`), reused
  wholesale.
- New: a `--omit-terminal-tools` (or capability-profile) flag on the `vmux mcp` sidecar that
  filters `run`/`read_terminal`/`terminal_send`/`terminal_clear` from `tool_definitions()`.
- `vmux mcp` sidecar resolution already exists (`vmux_agent/src/mcp.rs:8 resolve`,
  `McpServerConfig`); reuse it to build the `McpServerStdio`.

## 10. Build sequence

Each step compiles + runs.

1. **Daemon ACP spine.** Add `agent-client-protocol`; inline transport; `AcpSessionManager`;
   `SpawnAcpAgent`; `initialize` + `session/new` + serial `prompt`; projector for
   message/thought/tool-call → `AgentDelta`/`AgentMessagesSnapshot`. Smoke: `npx claude-code-acp`
   (point a temporary spawn at it) streams into the chat.
2. **AcpSession + plugin + URL + config.** new `AcpSession` component, `agent.acp` settings +
   built-in defaults, `AcpAgentPlugin` (`spawn_acp_session_on_add` / `send_acp_input` /
   `close_acp_session_on_remove`), URL `vmux://agent/<id>` attaches an `AcpSession`. Launchable
   from URL. (No `AgentVariant`/`AgentKind` changes.)
3. **Permission.** `request_permission` → `AwaitingApproval` → `approval.rs` (ACP branch) →
   daemon resolves the RPC.
4. **Terminal.** `create/output/wait/kill/release` → daemon process API + `AcpTerminalCreated`
   → visible pane.
5. **fs + proposed-edit overlay.** read/write handlers; `vmux_editor` `ProposedEdit` primitive;
   `ToolCallContent::Diff` → overlay gated by permission; `write_text_file` applies + clears.
6. **Scope C.** `--omit-terminal-tools` sidecar flag; `mcp_servers` into `session/new`; confirm
   browser/editor/history tools reach panes.
7. **Validate.** End-to-end `npx claude-code-acp`, then `vibe-acp`.

### Milestone D — retire Page + Cli; vmux becomes ACP-only (follow-on, after B+C proven)

End-state: every agent is an ACP agent. claude/codex/vibe run as ACP adapters; provider chat
runs as an ACP agent too.

1. **Provide ACP entries for all agents.** `claude` → `@zed-industries/claude-code-acp`,
   `codex` → `@zed-industries/codex-acp`, `vibe` → `vibe-acp`; provider chat (Anthropic/OpenAI/
   Mistral) → a provider→ACP adapter agent (or the vendor's own adapter). All become `agent.acp`
   entries; `vmux://agent/<id>` resolves to them.
2. **Retire the `Cli` path.** Remove `CliAgentStrategy` + `vibe.rs`/`claude.rs`/`codex.rs` impls,
   session-log discovery (`discover_session`/`detect_end_time`, `~/.vibe/logs`), the filesystem
   hooks (`hooks.toml`, `vmux notify-file-touch`). The `vmux mcp --anchor` sidecar **stays** — it
   is now the ACP tool channel (§9).
3. **Retire the `Page` path.** Remove the provider plugins (`anthropic_plugin`/`openai_plugin`/
   `mistral_plugin`/`echo_plugin`), `client/page/*`, the `vmux_service` SSE `run_session` loop,
   and `components::AgentSession`'s Page usage. The streaming/approval/projector plumbing reused
   by ACP stays.
4. **Delete `AgentVariant`** and the `kind`/`variant` fields once Page/Cli are gone; `AgentKind`
   collapses (or is removed) since identity is the `agent.acp` id. Fold `AgentMessages` /
   `AgentRunState` / `AgentApprovalPolicy` onto `AcpSession` as the sole agent-session shape.
5. **UX shift (intended).** Agents move from raw CLI TUIs / bespoke chat to vmux's native
   chat + diff + terminal panes (the ACP/Zed agent-panel experience). No escape hatch kept.

Sequencing rationale: B+C ship a working host with ACP agents as additive entries; D deletes the
old machinery **only after** the ACP path is runtime-proven, so no working integration is removed
before its replacement is validated.

## 11. Testing

Per `AGENTS.md` (system+message integration, no ad-hoc helpers) and memory ("verify
observable behavior", "workspace test before push", "finish then test").

- **Native unit tests (`vmux_service`, `vmux_core`):** projector table (each `SessionUpdate`
  variant → expected `Message`/`AgentDelta`); permission `ApprovalDecision`→`PermissionOptionId`
  mapping across option-kind sets; `TerminalId↔ProcessId` map; acp URL parse round-trip;
  `agent.acp` settings defaults present in embedded `settings.ron`.
- **Bevy plugin tests (`vmux_agent`):** register the plugin's written messages in `build()`
  (idempotent); send `AgentInput`/`AgentApprovalReply` for an `Acp` session entity and assert
  the resulting `ClientMessage` / `AgentRunState` transitions. Assert on the
  `AgentMessages`/ServiceMessages the frontend sees, not internals.
- **Runtime test = one pass at the end** (user runtime-tests). Implement directly — **no
  subagent-driven build** (CEF builds are large; long agents drop sockets). Warm the target dir
  with a background `cargo build` first, then iterate incrementally.
- CI runs fmt + clippy + tests on the PR. `git checkout -- patches/` after `cargo fmt` if it
  touches vendored crates.

## 12. Risks / open questions

- **acp 1.0 API churn.** Pin `=1.0.1`; wildcard-match `#[non_exhaustive]` enums. Confirm the
  generated handler-registration shape against the actual crate at build time (the `on_receive_*!`
  macros are mandatory final args).
- **Method-name spellings** (`session/new`, `session/prompt`, `session/cancel`) — confirm against
  the live agent during the step-1 smoke test; older agents may differ.
- **Blocking handlers.** Permission and proposed-write handlers park on `oneshot`s; verify the
  builder runs handlers concurrently with the input loop (each is an independent future) so a
  pending approval never deadlocks `prompt`.
- **CEF rebuild cost** for the new dep — expected, warm the target dir.
- **`AcpSession` vs the Page stream consumer.** `consume_page_agent_stream` currently keys on
  `components::AgentSession`. ACP entities use `AcpSession` + the shared `AgentMessages`/
  `AgentRunState`; register them in `AgentSessionToEntity` and generalize the consumer's query
  (`Or<(&AgentSession, &AcpSession)>` or a shared marker) so it drives ACP entities too. Verify
  the chat page renders from `AgentMessages` (not `AgentSession`).
- **URL disambiguation during transition.** `vmux://agent/<id>` (ACP) must take precedence over
  the legacy `agent/<provider>/<model>` (Page) parse until D removes the latter — match `<id>`
  against `agent.acp` config ids first.

## 13. Out of scope (future)

- ACP `loadSession`/`fork`/`resume`/`list`/`delete`, session modes, model/thinking selectors.
- `Image`/`Audio`/`Resource` content blocks in prompts and updates.
- `agent-client-protocol-rmcp` in-process tool server (alternative to the stdio sidecar).
- MCP-over-ACP (`McpServer::Acp`) and elicitation.
- Token-usage surfacing (`UsageUpdate`).
