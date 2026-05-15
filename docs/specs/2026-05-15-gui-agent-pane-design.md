# GUI Agent Pane

Date: 2026-05-15
Status: Approved (brainstorm), pending implementation plan

## Context

vmux today proxies AI chat through external CLIs (`vibe`, `claude`, `codex`) wrapped by `vmux_agent` strategies. Each strategy spawns the binary as a terminal process, registers vmux as an MCP server (so the CLI can call vmux tools), and watches the binary's session directory to map sessions back into Bevy entities.

This spec adds **GUI** agent panes: vmux talks to provider APIs directly, runs the agent loop in-process, renders chat UI in Dioxus. The CLI wrappers stay; GUI is an additional variant per provider.

## Decisions

| Topic | Decision |
| --- | --- |
| Relationship to CLI wrappers | Add alongside; CLI variants stay |
| Providers (v1) | Anthropic, Mistral, OpenAI |
| Purpose | vmux operator (drive spaces/tabs/terminals) |
| Tool autonomy | Autonomous loop; read-only auto, mutating prompts; per-session "always allow" |
| Tool surface | `vmux_mcp` only (no remote MCP, no bundled file/bash/web) |
| Persistence | `moonshine-save` on Bevy ECS components |
| Model selection | Encoded in URL path |
| Auth | macOS Keychain via `keyring` crate |
| New chat UX | Command bar action per provider |

## URL routing

```
vmux://agent/<provider>/<sid>          # GUI (new)
vmux://agent/<provider>/cli/<sid>      # CLI wrapper (existing)
```

`<provider>` ∈ {`vibe`, `claude`, `codex`}. The CLI wrappers currently live at `vmux://vibe/<sid>` etc. (see `crates/vmux_agent/src/kind.rs:17`); migration is mechanical: rewrite `AgentKind::url_scheme()` to emit the nested form, update `AgentKind::from_host()` to parse `agent` host plus a `cli/` segment, update persistence dispatch in `vmux_desktop` accordingly. Old URLs in saved scenes can be rewritten on load.

## Strategy split

Add `AgentVariant`:

```rust
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum AgentVariant { Gui, Cli }
```

`AgentStrategy` becomes a thin core trait. CLI-only methods (`build_args`, `build_env`, `discover_session`, `detect_end_time`, `sessions_root`) move to `CliAgentStrategy: AgentStrategy`. GUI concerns (spawn task, drive turn, cancel, list models) move to `GuiAgentStrategy: AgentStrategy`.

```rust
pub trait AgentStrategy: Send + Sync + 'static {
    fn kind(&self) -> AgentKind;
    fn variant(&self) -> AgentVariant;
}

pub trait CliAgentStrategy: AgentStrategy {
    fn build_args(&self, mcp: &McpServerConfig, sid: Option<&str>) -> Vec<String>;
    fn build_env(&self, mcp: &McpServerConfig) -> Vec<(String, String)>;
    fn discover_session(&self, cwd: &Path, spawn: SystemTime, claimed: &HashSet<String>) -> Option<String>;
    fn detect_end_time(&self, sid: &str) -> bool;
    fn sessions_root(&self) -> PathBuf;
}

pub trait GuiAgentStrategy: AgentStrategy {
    fn models(&self) -> &'static [&'static str];
    fn default_model(&self) -> &'static str;
    fn endpoint(&self) -> &'static str;
    fn build_request(
        &self,
        model: &str,
        messages: &[Message],
        tools: &[ToolDef],
        api_key: &str,
    ) -> reqwest::Request;
    fn parse_sse_event(&self, payload: &str) -> Option<StreamEvent>;
}
```

Strategies are **pure data + parsing** — no I/O, no spawned tasks. Generic ECS systems do the side effects, calling into the strategy to build the HTTP request body and to translate provider-specific SSE chunks into a common `StreamEvent` enum (`TextDelta`, `ToolUseStart`, `ToolUseArgsDelta`, `ToolUseEnd`, `StopTurn { reason }`, `Error`).

One `AgentStrategies` registry keyed by `(AgentKind, AgentVariant)`. Existing Vibe/Claude/Codex strategies become `(_, Cli)`; new strategies fill `(_, Gui)`.

## GUI agent loop (ECS systems)

Loop is a Bevy state machine driven by systems on `Update`. Per-session state lives entirely in components; async I/O happens via `IoTaskPool::spawn` returning `Task<T>` polled each frame. No long-lived Tokio task owns conversation state.

**Run-state component:**

```rust
#[derive(Component)]
pub enum AgentRunState {
    Idle,
    Streaming {
        rx: crossbeam_channel::Receiver<StreamEvent>,
        _task: Task<()>,         // dropped → cancels the I/O
        partial: Option<PartialToolUse>,
    },
    RunningTool {
        call_id: String,
        task: Task<ToolResult>,
    },
    AwaitingApproval {
        call_id: String,
        name: String,
        args: serde_json::Value,
    },
    Errored(String),
}
```

**Systems** (each runs every frame, queries entities in matching state):

- `process_user_input` — sees entities with a `PendingUserInput` component. Appends `User` message, calls strategy `build_request()`, spawns `IoTaskPool::spawn` task that pumps SSE chunks into a `crossbeam_channel`, transitions to `Streaming`. Removes `PendingUserInput`.
- `drain_stream` — for entities in `Streaming`, drains `rx`. Appends text deltas to the current `Assistant` message; on `ToolUseEnd` decides:
  - read-only or in `AgentApprovalPolicy.auto` → transitions to `RunningTool` (spawns dispatch task)
  - else → transitions to `AwaitingApproval`
  - on `StopTurn { end_turn }` → `Idle`
- `dispatch_tool` — for `RunningTool`, calls `future::poll_once(&mut task)`. On completion appends `ToolResult`, kicks the next turn by re-spawning the request task (no new user message — model continues).
- `handle_approval_reply` — reacts to `AgentApprovalReply` events; flips `AwaitingApproval` → `RunningTool` (or `Idle` with synthetic error `ToolResult` on Deny).

**Cancel** = system removes `AgentRunState::Streaming` (or `RunningTool`); dropping the component drops the held `Task<T>`, aborting the I/O.

**Streaming to UI** = `drain_stream` emits Bevy events (`AgentDelta`, `AgentToolStatus`, `AgentApprovalRequest`); `BinJsEmitEventPlugin` forwards them as rkyv frames to the Dioxus wasm side.

**moonshine-save interaction** — `Streaming` and `RunningTool` variants hold non-serializable handles. On save these are skipped; on load `AgentRunState` defaults to `Idle`. Persisted `AgentMessages` and `AgentApprovalPolicy` are durable; in-flight turns are dropped on quit (user re-sends).

## Tool dispatch

Reuse `vmux_mcp::tools::dispatch_from_tool_call()` directly. GUI bypasses JSON-RPC framing entirely; same `DispatchTarget::{Command, Query}` enum routes through `vmux_service::client::ServiceConnection` exactly as the stdio MCP server does.

Tool definitions gain a `read_only: bool` annotation. Default classification (using current `vmux_mcp` tool names from `crates/vmux_mcp/src/tools.rs`):

| Read-only (auto) | Mutating (approval) |
| --- | --- |
| `McpQueryTool` variants: `get_state`, `list_tabs`, `list_spaces`, `list_terminals`, `get_focused` | `McpParamTool` variants: `open_command_bar`, `new_terminal_tab`, `run_shell`, `browser_navigate`, `terminal_send`, `select_tab`, `split_and_navigate` |

Plus any `AppCommand` entries (also currently mutating). Per-session approvals stored in `AgentApprovalPolicy.auto: HashSet<String>`. No global "always allow" in v1.

## Persistence (moonshine-save)

Each session is a Bevy entity with components:

```rust
#[derive(Component)] struct AgentSession {
    kind: AgentKind,
    variant: AgentVariant,    // Gui here
    sid: String,              // UUIDv4
    provider: ProviderId,
    model: String,
}
#[derive(Component)] struct AgentMessages(Vec<Message>);
#[derive(Component)] struct AgentApprovalPolicy { auto: HashSet<String> }
// AgentRunState shape is in the agent-loop section above
```

`Message` enum: `User { text }`, `Assistant { blocks: Vec<AssistantBlock> }`, `ToolUse { call_id, name, args }`, `ToolResult { call_id, content, is_error }`. `AssistantBlock`: `Text(String)`, `ToolUse { call_id, name, args }`.

`moonshine-save` snapshots `AgentSession`, `AgentMessages`, `AgentApprovalPolicy`. `AgentRunState` is **not** persisted (holds Tasks/channels); on load it defaults to `Idle`, dropping any in-flight turn. SID is generated at chat-start; GUI does not need a filesystem watcher (CLI keeps its discoverer untouched).

## Dioxus UI

New page in `vmux_webview_app` at route `agent/<provider>/<sid>`:

- **Header**: provider name + model dropdown (changes future turns; existing turns keep their model)
- **Body**: scrolling message list
  - User bubble: plain text
  - Assistant: streaming text + inline `ToolUseCard` for each call (name, args JSON, status: pending → running → result/error)
  - Approval card for `AwaitingApproval`: "Allow" / "Allow always this session" / "Deny"
- **Input**: textarea + Send (Cmd+Enter); Stop button when `Running`

Bridge: rkyv events host↔wasm via `BinJsEmitEventPlugin` (already wired). New event types: `AgentInput`, `AgentDelta`, `AgentToolStatus`, `AgentApprovalRequest`, `AgentApprovalReply`.

## Auth

`vmux_agent::keys` module wraps `keyring::Entry`:

- `service = "ai.vmux.agent"`, `account = "anthropic" | "mistral" | "openai"`
- Settings page (separate `vmux://settings/keys` page) with paste field per provider, Save → keychain, Test button (1-token completion to verify).
- On chat start, GUI strategy loads key; if missing, opens settings before first turn.

No env-var fallback in v1 (keep one path; add later if requested).

## Tool annotations

Annotation lives in `vmux_mcp::tools` next to existing `tool_definitions()`. The GUI side is the only consumer in v1; CLI wrappers ignore it (the CLIs do their own permission UX).

## v1 ship order

Each step is its own PR + worktree:

1. **URL migration** — rewrite scheme to `vmux://agent/<host>/[cli/]<sid>`, update routing in `kind.rs`, persistence dispatch in `vmux_desktop`. CLI wrappers continue working under new URLs. No GUI code yet. Migrate existing saved scenes.
2. **GUI skeleton** — `AgentVariant`, trait split, registry rework, session components, `moonshine-save` wiring. Stub `EchoStrategy` that echoes user input as assistant response. Confirms end-to-end plumbing without hitting any provider API.
3. **Anthropic provider** — real Claude API via `reqwest`, SSE streaming, tool-use loop, approval flow. First production-quality GUI provider.
4. **Dioxus chat UI** — chat layout, streaming render, tool/approval cards, model dropdown. Replaces stub UI introduced in step 2.
5. **Mistral provider** — clone Anthropic with Mistral's tool-use shape (function calling).
6. **OpenAI provider** — Responses API or chat-completions with tool calls.
7. **Keychain settings** — in-app `vmux://settings/keys` page, Test button per provider.

Each step gets its own design subspec + plan + worktree.

## Non-goals (v1)

- Remote MCP server registration (Mistral-style connector flow). Vmux only exposes its own tools.
- Bundled file/bash/web tools. CLI wrappers cover that use case.
- Cost / token-usage display.
- Subscription-based auth (Claude Pro, ChatGPT Plus). Anthropic's Feb 2026 policy bars third parties; same de-facto for OpenAI/Mistral.
- Local model providers (Ollama, llama.cpp). Tool-use reliability too low under 70B; revisit later.
- Per-tool global allow/deny. Session-scoped only.
- Mid-thread provider switch (only model can change within a thread; provider is fixed per session because it's in the URL).

## Open questions

- Should `<sid>` for GUI be a UUIDv4 or a slugified short form? Going with UUIDv4 for uniqueness; revisit if URLs become user-typed.
- Where do per-session tool annotations live if a user wants to override read-only-ness? Defer to v2.
- Cancel semantics during a tool call: kill the in-flight tool? Wait for it then stop? Going with "let in-flight tool finish, then stop loop" — predictable and avoids partial mutations.
