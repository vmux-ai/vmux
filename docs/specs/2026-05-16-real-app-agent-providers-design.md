# Real App Agent Providers — Design

**Goal:** Replace `EchoAppStrategy` with three real provider implementations (Mistral / Anthropic / OpenAI) wired end-to-end through the existing `AppAgentStrategy` trait. Includes the full multi-turn agent loop (text + MCP tool use + approval), default-provider routing via environment, and UI error feedback in the Dioxus chat.

**Non-goals:** Vibe CLI parity, custom enterprise endpoints, key storage in keychain, model-list-fetching APIs, telemetry.

**Status:** Replaces the temporary echo stub introduced by the GUI agent skeleton. Builds on `crates/vmux_agent` as it exists on `vmx-gui-agent`.

---

## Architecture

Three concrete `AppAgentStrategy` impls + shared parsing helpers + an SSE driver. All orchestration stays in Bevy systems against per-session components. Strategies are pure data: they build a `reqwest::Request` and parse SSE events into the existing `StreamEvent` enum. Bevy systems own state transitions, channel pumping, tool dispatch, and the multi-turn loop.

```
crates/vmux_agent/src/
  providers/
    mod.rs           re-exports
    builtin.rs       BUILTIN_PROVIDERS + resolve_default_app_provider()
    anthropic.rs     AnthropicStrategy + helpers (messages_to_anthropic, parse_anthropic_sse, tools_to_anthropic)
    openai.rs        OpenAiResponsesStrategy + helpers (parse_openai_sse — shared with mistral)
    mistral.rs       MistralStrategy (reuses openai parsing, different endpoint/env/headers)
  http.rs            SseDriver: drives reqwest streaming and emits StreamEvent
  systems/
    process_input.rs       (existing) rewired to use strategy + SseDriver
    drain_stream.rs        (existing) extended for tool-use accumulation + approval gate
    dispatch_tool.rs       (existing) unchanged
    approval.rs            (existing) Allow path spawns MCP tool task
    continue_after_tool.rs (new) re-streams after ToolResult
    surface_errors.rs      (new) Errored → inline message + toast event
```

### ECS data flow

```
Resources:
  AgentStrategies (existing)

Components (per session):
  AgentSession         durable
  AgentMessages        durable
  AgentApprovalPolicy  durable
  AgentRunState        runtime (Idle | Streaming | RunningTool | AwaitingApproval | Errored)
  PendingUserInput     transient

Bevy events (existing + new):
  AgentInput, AgentDelta, AgentToolStatus, AgentApprovalRequest, AgentApprovalReply  (existing)
  AgentToast { session, level, message }                                              (new)

State transitions:
  PendingUserInput + Idle ─process_user_input──▶ Streaming
  Streaming  ─drain_stream─ StopTurn(EndTurn) ─▶ Idle
  Streaming  ─drain_stream─ ToolUseEnd       ─▶ AwaitingApproval | RunningTool (if auto)
  AwaitingApproval ─handle_approval_reply(Allow) ─▶ RunningTool
  AwaitingApproval ─handle_approval_reply(Deny)  ─▶ Idle (with ToolResult error)
  RunningTool ─dispatch_tool─▶ Idle (with ToolResult)
  Idle + tail==ToolResult ─continue_after_tool─▶ Streaming
  Any ─SseDriver error─▶ Errored
  Errored + PendingUserInput ─process_user_input─▶ Streaming (recover)
```

The loop is emergent: each system reacts to a single state, no system orchestrates the whole turn.

---

## Trait & helpers

`AppAgentStrategy` (existing, unchanged signature, one new method):

```rust
pub trait AppAgentStrategy: AgentStrategy {
    fn provider(&self) -> &str;
    fn model(&self) -> &str;
    fn endpoint(&self) -> &str;
    fn build_request(
        &self,
        model: &str,
        messages: &[Message],
        tools: &[ToolDef],
        api_key: &str,
    ) -> reqwest::Request;
    fn parse_sse_event(&self, payload: &str) -> Option<StreamEvent>;

    /// Env var name to source API key from. Empty string ⇒ no auth.
    fn env_var(&self) -> &'static str;
}
```

Concrete impls hold `(provider: String, model: String)` and a `kind: AgentKind` (for URL routing).

### Shared helpers

- `providers::openai::parse_chat_completions_sse(line: &str) -> Option<StreamEvent>` — parses `data: {...}` lines from OpenAI Chat Completions / Mistral SSE. Returns `StreamEvent::TextDelta`, `ToolUseStart/Args/End`, `StopTurn`.
- `providers::openai::messages_to_chat_completions(&[Message]) -> Vec<serde_json::Value>` — maps internal messages to OpenAI message objects (`user`/`assistant`/`tool` roles, content arrays, tool_calls/tool_call_id fields).
- `providers::openai::tools_to_function_specs(&[ToolDef]) -> Vec<serde_json::Value>` — `{type:"function", function:{name,description,parameters}}` shape.
- `providers::anthropic::messages_to_blocks(&[Message]) -> (Option<String> system, Vec<serde_json::Value> messages)` — separates system prompt, builds content blocks (`text` / `tool_use` / `tool_result`).
- `providers::anthropic::tools_to_anthropic(&[ToolDef]) -> Vec<serde_json::Value>` — `{name, description, input_schema}` shape.
- `providers::anthropic::parse_messages_sse(line: &str) -> Option<StreamEvent>` — parses Anthropic event-named SSE (`event: content_block_delta\\ndata: ...`). Tracks block index state via a small parser struct passed into `parse_sse_event` (strategy owns mutable parser state via interior `Mutex`? No — see below).

Anthropic SSE quirk: events span multiple lines (`event:` + `data:`). The `SseDriver` must group lines into frames before calling `parse_sse_event`. So:

- `SseDriver` is line-based; it accumulates lines until a blank line, then passes the **entire frame text** (event name + data) to `strategy.parse_sse_event`.
- Each `parse_sse_event` call is stateless — it receives one full SSE frame, returns at most one `StreamEvent`. State machine for "which content block is open" lives in `drain_stream` via `AgentRunState::Streaming { partial: Option<PartialToolUse> }` (existing).

---

## SseDriver

`http::SseDriver` is a free function (no struct needed):

```rust
pub async fn drive_sse(
    request: reqwest::Request,
    strategy: Arc<dyn AppAgentStrategy>,  // 'static via Arc
    tx: crossbeam_channel::Sender<StreamEvent>,
) {
    let client = reqwest::Client::new();
    let response = match client.execute(request).await {
        Ok(r) => r,
        Err(e) => {
            let _ = tx.send(StreamEvent::Error(format!("HTTP request failed: {e}")));
            return;
        }
    };
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        let _ = tx.send(StreamEvent::Error(format!(
            "HTTP {status}: {}",
            body.chars().take(500).collect::<String>()
        )));
        return;
    }
    let mut stream = response.bytes_stream();
    let mut buf = String::new();
    while let Some(chunk) = stream.next().await {
        let bytes = match chunk {
            Ok(b) => b,
            Err(e) => {
                let _ = tx.send(StreamEvent::Error(format!("stream chunk: {e}")));
                return;
            }
        };
        buf.push_str(&String::from_utf8_lossy(&bytes));
        while let Some(idx) = buf.find("\n\n") {
            let frame: String = buf.drain(..idx + 2).collect();
            let frame = frame.trim_end();
            if frame.is_empty() { continue; }
            if let Some(event) = strategy.parse_sse_event(frame) {
                if tx.send(event).is_err() { return; }
            }
        }
    }
}
```

`Arc<dyn AppAgentStrategy>` lets us hand a strategy reference into a `'static` async task. Strategies registered into `AgentStrategies` become `Arc<dyn AppAgentStrategy>` (change registry storage from `Box` to `Arc`).

---

## Concrete strategies

### MistralStrategy

```
endpoint: "https://api.mistral.ai/v1/chat/completions"
env_var:  "MISTRAL_API_KEY"
headers:  Authorization: Bearer {key}, Content-Type: application/json, Accept: text/event-stream
body:     { model, messages, tools?, tool_choice: "auto", stream: true }
parser:   shared openai::parse_chat_completions_sse
```

### OpenAiResponsesStrategy

```
endpoint: "https://api.openai.com/v1/responses"
env_var:  "OPENAI_API_KEY"
headers:  Authorization: Bearer {key}, Content-Type: application/json, Accept: text/event-stream
body:     { model, input: [...messages...], tools?, stream: true }
parser:   providers::openai::parse_responses_sse (different event names than chat completions:
          response.output_text.delta, response.output_item.added (function_call), 
          response.function_call_arguments.delta, response.completed, etc.)
```

Note: Responses API uses different streaming events than Chat Completions. They share `messages_to_*` only superficially. v1 ships them as separate parsers; refactor for shared abstractions later if duplication grows.

### AnthropicStrategy

```
endpoint: "https://api.anthropic.com/v1/messages"
env_var:  "ANTHROPIC_API_KEY"
headers:  x-api-key: {key}, anthropic-version: 2023-06-01, Content-Type: application/json, Accept: text/event-stream
body:     { model, max_tokens: 8192, system?, messages, tools?, stream: true }
parser:   providers::anthropic::parse_messages_sse
```

Prompt caching: send `cache_control: { type: "ephemeral" }` on the last system block and the last `tool_result` content block (per Anthropic guidance for tool-heavy conversations). v1 does this unconditionally.

---

## Default provider resolution

```rust
// providers::builtin
pub struct BuiltinProvider {
    pub provider: &'static str,
    pub kind: AgentKind,
    pub default_model: &'static str,
    pub env_var: &'static str,
}

pub const BUILTIN_PROVIDERS: &[BuiltinProvider] = &[
    BuiltinProvider { provider: "mistral",   kind: AgentKind::Vibe,   default_model: "devstral-2",        env_var: "MISTRAL_API_KEY"   },
    BuiltinProvider { provider: "anthropic", kind: AgentKind::Claude, default_model: "claude-sonnet-4-6", env_var: "ANTHROPIC_API_KEY" },
    BuiltinProvider { provider: "openai",    kind: AgentKind::Codex,  default_model: "gpt-5",             env_var: "OPENAI_API_KEY"    },
];

pub fn resolve_default_app_provider() -> Option<&'static BuiltinProvider> {
    BUILTIN_PROVIDERS.iter().find(|p| std::env::var(p.env_var).is_ok())
}
```

`register_app_agents_from_settings` becomes `register_app_agents`:

1. For each `BuiltinProvider`: instantiate the matching concrete strategy, register under `(provider, default_model)`. Always register, even without env key (request will fail with clear `Errored` message).
2. For each `AppProviderSettings` in `settings.agent.app_providers`: instantiate based on `provider` field, register. Overrides built-in for matching `(provider, model)` key.

Default `AppSettings::agent.app_providers = vec![]`. User configures only overrides.

---

## URL resolution for default agent

Extend `AgentUrl`:

```rust
pub enum AgentUrl {
    Cli { kind: AgentKind, sid: String },
    App { provider: String, model: String, sid: String },
    AppDefault,
}

impl AgentUrl {
    pub fn parse(url: &str) -> Option<Self> {
        let body = url.strip_prefix("vmux://agent/")?;
        let segs: Vec<&str> = body.split('/').filter(|s| !s.is_empty()).collect();
        match segs.as_slice() {
            [] => Some(AgentUrl::AppDefault),
            [kind_seg, sid] => Some(AgentUrl::Cli { kind: AgentKind::from_url_segment(kind_seg)?, sid: (*sid).to_string() }),
            [provider, model, sid] => Some(AgentUrl::App { provider: (*provider).to_string(), model: (*model).to_string(), sid: (*sid).to_string() }),
            _ => None,
        }
    }
}
```

In `vmux_desktop`'s spawn handler, when an `AppDefault` URL is received:

1. `resolve_default_app_provider()` → `Some(BuiltinProvider)` → mint UUID for sid → reformat URL → spawn `App` session.
2. `None` (no env key) → spawn a placeholder pane that displays "No agent provider available. Set MISTRAL_API_KEY, ANTHROPIC_API_KEY, or OPENAI_API_KEY." + link to vmux_settings.

**Command bar.** Replace the three per-kind "New Vibe/Claude/Codex chat" actions with:
- "New chat" → opens `vmux://agent/` (uses default)
- "New chat with mistral / anthropic / openai" (three actions) → opens explicit `vmux://agent/<provider>/<default_model>/<uuid>`

---

## Tool integration

`process_user_input` and `continue_after_tool` build `Vec<ToolDef>` from MCP. `vmux_mcp::tools::tool_definitions()` already returns `Vec<ToolDefinition>` (name, description, input_schema). A new free function `vmux_agent::tools::mcp_tool_defs() -> Vec<ToolDef>` wraps it, mapping each `ToolDefinition` into `ToolDef` (the existing type in `vmux_agent::stream`) with `read_only: false` for v1 (every MCP tool needs approval unless added to `AgentApprovalPolicy.auto`). Sent on every request — providers cache schemas where possible (Anthropic prompt cache on the last tool block).

### drain_stream extensions

State machine in `drain_stream`:

```
StreamEvent::TextDelta(t)           → ensure assistant tail, append text, trigger AgentDelta
StreamEvent::ToolUseStart{id, name} → partial = Some(PartialToolUse{id, name, args:""})
StreamEvent::ToolUseArgsDelta{id, c}→ partial.args.push_str(c)
StreamEvent::ToolUseEnd{id}         → push AssistantBlock::ToolUse{id, name, args} to current assistant message
                                      if policy.auto.contains(name) → spawn MCP task → RunningTool
                                      else → trigger AgentApprovalRequest → AwaitingApproval
                                      partial = None
StreamEvent::StopTurn{EndTurn}      → state = Idle
StreamEvent::StopTurn{ToolUse}      → keep current state (RunningTool/AwaitingApproval already set above)
StreamEvent::StopTurn{MaxTokens}    → state = Idle (no continuation; surface as toast)
StreamEvent::StopTurn{Other}        → state = Idle
StreamEvent::Error(msg)             → state = Errored(msg)
```

### continue_after_tool (new)

```rust
pub fn continue_after_tool(
    mut q: Query<(Entity, &mut AgentRunState, &AgentMessages, &AgentSession)>,
    strategies: Res<AgentStrategies>,
) {
    for (entity, mut state, messages, session) in &mut q {
        if !matches!(*state, AgentRunState::Idle) { continue; }
        if !matches!(messages.0.last(), Some(Message::ToolResult { .. })) { continue; }
        // mirrors process_user_input body: look up strategy, build request, spawn driver, set Streaming
        ...
    }
}
```

Same code path as `process_user_input` minus the `PendingUserInput` consumption. Factor the body into a helper `spawn_turn(commands, entity, session, messages, strategies) -> Option<AgentRunState>`.

### approval Allow path (modify existing)

`handle_approval_reply` Allow branch currently sets `Idle`. Change to spawn the MCP tool task and set `RunningTool` (consistent with `drain_stream`'s auto-approve branch). Share via `spawn_tool_task(call_id, name, args) -> Task<ToolDispatchOutput>` helper.

---

## Settings

`AppProviderSettings` unchanged. `default_agent_settings()` returns empty `app_providers`. Built-ins are not stored in settings — they live in code.

```rust
fn default_agent_settings() -> AgentSettings {
    AgentSettings { app_providers: vec![] }
}
```

Backward compatibility: if a user has the existing `stub`/`echo` entry in their `settings.ron`, it still registers (creates `EchoAppStrategy` so the legacy URL works) — but the default resolution skips `stub`.

---

## Error handling

Three failure modes map to distinct surfaces:

| Failure | Where | `AgentRunState` | UI |
|---|---|---|---|
| Missing env API key (request build time) | `process_user_input` / `continue_after_tool` | `Errored("Missing {ENV_VAR}")` | inline message + toast (Error) |
| HTTP non-2xx, network error, stream chunk error | `SseDriver` emits `StreamEvent::Error(msg)` | `drain_stream` transitions to `Errored(msg)` | inline + toast |
| SSE parse failure | inside `parse_sse_event` | log only | none |
| MCP tool failure | `dispatch_tool` | stays `Idle` | flows as `Message::ToolResult { is_error: true }` (existing) |

`surface_errors` system (new) watches for transitions into `Errored` and:
1. Appends `Message::Assistant { blocks: [AssistantBlock::Text("⚠ {msg}")] }`.
2. Triggers `AgentToast { session: entity, level: Error, message: msg }`.

Transition tracking via `Changed<AgentRunState>` filter + a sibling `LastRunStateKind` component holding only the discriminant (new pure-enum `AgentRunStateKind { Idle, Streaming, RunningTool, AwaitingApproval, Errored }` mirroring the variants of `AgentRunState`). `surface_errors` reads `AgentRunState`, compares its kind against `LastRunStateKind`, fires inline+toast on `Errored` transitions, and writes the new kind back. Bevy lacks built-in previous-value access on components, so this sibling-component pattern is the idiomatic ECS workaround.

Recovery: next `PendingUserInput` lets `process_user_input` overwrite `Errored` → `Streaming`.

---

## UI feedback (Dioxus)

New event registered with `BinJsEmitEventPlugin<AgentToast>`:

```rust
#[derive(Event, Clone, Debug, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct AgentToast {
    pub session_sid: String,     // not Entity — serializable
    pub level: ToastLevel,
    pub message: String,
}

#[derive(Clone, Copy, Debug, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum ToastLevel { Info, Warning, Error }
```

JS event id: `vmux-agent-toast`. Dioxus side: hook in the agent page subscribes via `webview-events` API and calls the existing `vmux_ui::components::toast::ToastProvider` API.

Inline error rendering reuses the existing chat message renderer (toasts are additive — inline message is the canonical record, toast is the transient notice).

---

## Testing

### Unit tests (per file)

- `providers::openai`: `parse_chat_completions_sse` with captured Mistral fixtures (`tests/fixtures/mistral/text.sse`, `tools.sse`) → asserts emitted `StreamEvent` sequence.
- `providers::openai::parse_responses_sse`: OpenAI Responses API fixtures (`tests/fixtures/openai/text.sse`, `tools.sse`).
- `providers::anthropic::parse_messages_sse`: Anthropic fixtures (`tests/fixtures/anthropic/text.sse`, `tools.sse`).
- `build_request` for each strategy: assert URL, method, headers, body shape.
- `builtin::resolve_default_app_provider`: env-var precedence (set each in turn via `temp_env` crate or direct `std::env::set_var` + cleanup).
- `AgentUrl::parse("vmux://agent/")` → `AppDefault`.

### System tests (mocked strategy)

- `process_user_input` with a `MockAppStrategy` that records the call → asserts transitions Idle → Streaming and request was built.
- `continue_after_tool` with a `MockAppStrategy` + a `ToolResult`-terminated message list → asserts re-stream.
- `drain_stream` with a tool-use sequence → asserts `AwaitingApproval` (or `RunningTool` if policy auto) and `AssistantBlock::ToolUse` pushed.
- `surface_errors` injecting `Errored` → asserts inline `Message::Assistant` appended + `AgentToast` event triggered.

### Integration smoke

`crates/vmux_agent/tests/streaming_smoke.rs` — wires a `MockAppStrategy` whose `build_request` points at a `mockito` HTTP server returning canned SSE bodies. Asserts:
1. Single text turn: user input → final assistant message contains the streamed text.
2. Tool use turn: user input → assistant tool_use → auto-approve → MCP tool returns → continuation streams a final assistant message → state Idle.

### Manual smoke (in implementation plan)

Real API hits (one per provider) — driven by `cargo run -p vmux_desktop` with each env var set; verify text + one tool call.

---

## Dependencies

Add to `crates/vmux_agent/Cargo.toml`:

```toml
[dependencies]
reqwest = { version = "0.12", default-features = false, features = ["rustls-tls", "stream", "json"] }
futures-util = "0.3"
uuid = { workspace = true }       # already in workspace; used for default URL sid minting
vmux_mcp = { path = "../vmux_mcp" }   # tool_definitions() bridge

[dev-dependencies]
mockito = "1"
```

Workspace `tokio` is not needed — `IoTaskPool::spawn` provides the runtime via Bevy. `mockito` is stable + zero-fuss for HTTP fixtures and adds no runtime dep.

---

## Rollout

This spec lands in one PR on top of `vmx-gui-agent` branch. Echo strategy is removed in the same PR (legacy `stub`/`echo` settings entry still registers `EchoAppStrategy` for back-compat, but new defaults don't reference it).

Out of scope for follow-up specs:
- Per-tool approval UX polish (modal vs inline)
- Token usage display in toolbar
- Streaming `thinking` blocks (Anthropic) and reasoning (OpenAI o-series)
- Provider model lists fetched at runtime
- Cost tracking
- Conversation export / import
