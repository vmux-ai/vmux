# Real App Agent Providers Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace `EchoAppStrategy` with three real provider implementations (Mistral, Anthropic, OpenAI Responses) wired end-to-end through `AppAgentStrategy`, including built-in defaults, default-URL routing, full multi-turn MCP tool loop, and Dioxus toast/inline UI feedback.

**Architecture:** Pure-data strategies (build request + parse SSE) backed by Bevy systems that own all state transitions. Shared SSE driver runs in `IoTaskPool::spawn`. Multi-turn loop is emergent from per-state systems: `process_user_input` → `drain_stream` → `dispatch_tool` → `continue_after_tool` → ... → `Idle`.

**Tech Stack:** Bevy 0.18 ECS, `reqwest` 0.12 (rustls, stream, json), `crossbeam-channel`, `futures-util`, `mockito` (dev), `bevy_cef` BinJsEmitEventPlugin for Dioxus events, existing `vmux_mcp::tools::tool_definitions()`.

**Pre-commit checks:** Per `AGENTS.md`, after every commit run on the changed crates only:
```
PKGS=$(BASE=origin/main ./scripts/changed-crates.sh)
for pkg in $PKGS; do cargo fmt -p "$pkg" -- --check; done
for pkg in $PKGS; do env -u CEF_PATH cargo clippy -p "$pkg" --all-targets -- -D warnings; done
for pkg in $PKGS; do env -u CEF_PATH cargo test -p "$pkg"; done
```
If `cargo fmt` fails: `cargo fmt -p "$pkg"` then re-stage and amend the next commit (don't rewrite a published commit). Each task lists which crates will appear in the changed set so you can pre-narrow the loop.

---

## File Structure

**New files in `crates/vmux_agent/src/`:**
- `providers.rs` — module root
- `providers/builtin.rs` — `BuiltinProvider`, `BUILTIN_PROVIDERS`, `resolve_default_app_provider`
- `providers/openai_shared.rs` — chat-completions request/parse helpers shared by mistral
- `providers/mistral.rs` — `MistralStrategy`
- `providers/openai.rs` — `OpenAiResponsesStrategy` (Responses API; different SSE event names)
- `providers/anthropic.rs` — `AnthropicStrategy` + Messages API helpers
- `http.rs` — `drive_sse` async function
- `tools.rs` — `mcp_tool_defs()` bridge from `vmux_mcp::tools::tool_definitions()`
- `systems/continue_after_tool.rs` — re-stream when last message is a ToolResult
- `systems/surface_errors.rs` — emit inline message + AgentToast on Errored transitions
- `toast.rs` — `AgentToast` event + `ToastLevel` enum (rkyv-serializable)
- `run_state_kind.rs` — `AgentRunStateKind` discriminant mirror + `LastRunStateKind` component
- `tests/fixtures/mistral/text.sse`, `tests/fixtures/mistral/tools.sse`
- `tests/fixtures/openai/text.sse`, `tests/fixtures/openai/tools.sse`
- `tests/fixtures/anthropic/text.sse`, `tests/fixtures/anthropic/tools.sse`
- `tests/streaming_smoke.rs`

**Modified files in `crates/vmux_agent/src/`:**
- `lib.rs` — register new modules + re-exports
- `Cargo.toml` — add `futures-util`, `mockito` (dev), `vmux_mcp` path dep, extend `reqwest` features
- `app.rs` — add `env_var()` method to `AppAgentStrategy` trait
- `kind.rs` — add `AgentUrl::AppDefault` variant + parser case for bare URL
- `strategy.rs` — change registry storage from `Box<dyn AppAgentStrategy>` to `Arc<dyn AppAgentStrategy>`
- `app_plugin.rs` — add `continue_after_tool` + `surface_errors` systems; register `AgentToast` event
- `systems/process_input.rs` — replace echo path with strategy lookup + `drive_sse` task
- `systems/drain_stream.rs` — extend for `ToolUseStart/ArgsDelta/End` + `AwaitingApproval`/`RunningTool` transitions + `Error`
- `systems/approval.rs` — Allow path spawns MCP tool task → `RunningTool`
- `echo.rs` — keep for back-compat; switch trait method `env_var()` to return `""`

**Modified files in `crates/vmux_desktop/src/`:**
- `settings.rs` — replace `register_app_agents_from_settings` body with built-ins + overrides; default `app_providers` becomes empty
- `agent.rs` — handle `AgentUrl::AppDefault` (resolve + reformat as `App`, or spawn "no provider" placeholder)
- `command_bar.rs` — replace per-kind "New chat" actions with "New chat" (default) + three explicit-provider actions

**Modified files in `crates/vmux_desktop/Cargo.toml`:**
- no new deps (uses `uuid` already)

---

## Task 1: Add `env_var()` to AppAgentStrategy trait

**Files:**
- Modify: `crates/vmux_agent/src/app.rs`
- Modify: `crates/vmux_agent/src/echo.rs:42-44` (the `EchoAppStrategy` impl block)

Changed crates: `vmux_agent`

- [ ] **Step 1: Add the trait method**

Edit `crates/vmux_agent/src/app.rs`:

```rust
use crate::message::Message;
use crate::strategy::AgentStrategy;
use crate::stream::{StreamEvent, ToolDef};

pub trait AppAgentStrategy: AgentStrategy {
    fn provider(&self) -> &str;
    fn model(&self) -> &str;
    fn endpoint(&self) -> &str;
    fn env_var(&self) -> &'static str;

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

- [ ] **Step 2: Implement on EchoAppStrategy**

In `crates/vmux_agent/src/echo.rs`, inside `impl AppAgentStrategy for EchoAppStrategy { ... }`, add (place after `endpoint`):

```rust
    fn env_var(&self) -> &'static str {
        ""
    }
```

- [ ] **Step 3: Update inline trait impls in `strategy.rs` tests**

In `crates/vmux_agent/src/strategy.rs` find the three `impl crate::app::AppAgentStrategy for StubApp/App { ... }` blocks inside `#[cfg(test)] mod tests`. Each must add:

```rust
    fn env_var(&self) -> &'static str {
        ""
    }
```

There are two such impls (`StubApp` and `App`). Add the method to both.

- [ ] **Step 4: Run vmux_agent tests**

```bash
env -u CEF_PATH cargo test -p vmux_agent
```
Expected: all existing tests still pass.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_agent/src/app.rs crates/vmux_agent/src/echo.rs crates/vmux_agent/src/strategy.rs
git commit -m "feat(vmux_agent): add env_var() to AppAgentStrategy"
```

---

## Task 2: Change registry storage from Box to Arc

**Files:**
- Modify: `crates/vmux_agent/src/strategy.rs`

Changed crates: `vmux_agent`

Reason: `drive_sse` runs in `IoTaskPool::spawn` (`'static` async block). Strategy references must be `Arc<dyn AppAgentStrategy>` to hand into the task.

- [ ] **Step 1: Replace Box with Arc in AgentStrategies**

In `crates/vmux_agent/src/strategy.rs`, update the imports and struct fields:

```rust
use std::collections::HashMap;
use std::sync::Arc;

use bevy::prelude::Resource;

use crate::AgentKind;
use crate::AgentVariant;
use crate::app::AppAgentStrategy;
use crate::cli_trait::CliAgentStrategy;

pub trait AgentStrategy: Send + Sync + 'static {
    fn kind(&self) -> AgentKind;
    fn variant(&self) -> AgentVariant;
}

#[derive(Resource, Default)]
pub struct AgentStrategies {
    cli: HashMap<AgentKind, Box<dyn CliAgentStrategy>>,
    app: HashMap<(String, String), Arc<dyn AppAgentStrategy>>,
}

impl AgentStrategies {
    pub fn register_cli(&mut self, strategy: Box<dyn CliAgentStrategy>) {
        self.cli.insert(strategy.kind(), strategy);
    }

    pub fn get_cli(&self, kind: AgentKind) -> Option<&dyn CliAgentStrategy> {
        self.cli.get(&kind).map(|b| b.as_ref())
    }

    pub fn register_app(&mut self, strategy: Arc<dyn AppAgentStrategy>) {
        let key = (
            strategy.provider().to_string(),
            strategy.model().to_string(),
        );
        self.app.insert(key, strategy);
    }

    pub fn get_app_by_provider_model(
        &self,
        provider: &str,
        model: &str,
    ) -> Option<Arc<dyn AppAgentStrategy>> {
        self.app
            .get(&(provider.to_string(), model.to_string()))
            .cloned()
    }

    pub fn app_strategies(&self) -> impl Iterator<Item = &Arc<dyn AppAgentStrategy>> {
        self.app.values()
    }

    pub fn cli_strategies(&self) -> impl Iterator<Item = &dyn CliAgentStrategy> {
        self.cli.values().map(|b| b.as_ref())
    }
}
```

- [ ] **Step 2: Update test call sites in same file**

The three test functions in `strategy.rs` already call `register_app(Box::new(...))`. Change each `Box::new(...)` → `Arc::new(...)` (three sites). Also at the top of the test module add `use std::sync::Arc;`.

- [ ] **Step 3: Update register_app call site in settings.rs**

In `crates/vmux_desktop/src/settings.rs`, find `strategies.register_app(Box::new(vmux_agent::EchoAppStrategy::new(...)))` and change `Box::new` → `std::sync::Arc::new`.

- [ ] **Step 4: Build and test the changed crates**

```bash
env -u CEF_PATH cargo test -p vmux_agent
env -u CEF_PATH cargo build -p vmux_desktop
```
Expected: both succeed.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_agent/src/strategy.rs crates/vmux_desktop/src/settings.rs
git commit -m "refactor(vmux_agent): store AppAgentStrategy as Arc for cross-task use"
```

---

## Task 3: Add `AgentRunStateKind` discriminant mirror

**Files:**
- Create: `crates/vmux_agent/src/run_state_kind.rs`
- Modify: `crates/vmux_agent/src/lib.rs`

Changed crates: `vmux_agent`

- [ ] **Step 1: Write the kind mirror with tests**

Create `crates/vmux_agent/src/run_state_kind.rs`:

```rust
use bevy::prelude::Component;

use crate::run_state::AgentRunState;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum AgentRunStateKind {
    Idle,
    Streaming,
    RunningTool,
    AwaitingApproval,
    Errored,
}

impl From<&AgentRunState> for AgentRunStateKind {
    fn from(state: &AgentRunState) -> Self {
        match state {
            AgentRunState::Idle => AgentRunStateKind::Idle,
            AgentRunState::Streaming { .. } => AgentRunStateKind::Streaming,
            AgentRunState::RunningTool { .. } => AgentRunStateKind::RunningTool,
            AgentRunState::AwaitingApproval { .. } => AgentRunStateKind::AwaitingApproval,
            AgentRunState::Errored(_) => AgentRunStateKind::Errored,
        }
    }
}

#[derive(Component, Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct LastRunStateKind(pub AgentRunStateKind);

impl Default for LastRunStateKind {
    fn default() -> Self {
        Self(AgentRunStateKind::Idle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_state_idle() {
        let s = AgentRunState::Idle;
        assert_eq!(AgentRunStateKind::from(&s), AgentRunStateKind::Idle);
    }

    #[test]
    fn from_state_errored() {
        let s = AgentRunState::Errored("oops".into());
        assert_eq!(AgentRunStateKind::from(&s), AgentRunStateKind::Errored);
    }

    #[test]
    fn last_run_state_kind_default_is_idle() {
        assert_eq!(LastRunStateKind::default().0, AgentRunStateKind::Idle);
    }
}
```

- [ ] **Step 2: Wire into lib.rs**

In `crates/vmux_agent/src/lib.rs`:

After the existing `pub mod run_state;` line, add:

```rust
pub mod run_state_kind;
```

After the existing `pub use run_state::...` line, add:

```rust
pub use run_state_kind::{AgentRunStateKind, LastRunStateKind};
```

- [ ] **Step 3: Run tests**

```bash
env -u CEF_PATH cargo test -p vmux_agent run_state_kind
```
Expected: 3 tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/vmux_agent/src/run_state_kind.rs crates/vmux_agent/src/lib.rs
git commit -m "feat(vmux_agent): add AgentRunStateKind discriminant + LastRunStateKind component"
```

---

## Task 4: Add AgentToast event + ToastLevel

**Files:**
- Create: `crates/vmux_agent/src/toast.rs`
- Modify: `crates/vmux_agent/src/lib.rs`

Changed crates: `vmux_agent`

- [ ] **Step 1: Write the event + tests**

Create `crates/vmux_agent/src/toast.rs`:

```rust
use bevy::prelude::Message;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum ToastLevel {
    Info,
    Warning,
    Error,
}

#[derive(Message, Clone, Debug, Serialize, Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct AgentToast {
    pub session_sid: String,
    pub level: ToastLevel,
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rkyv_roundtrip() {
        let t = AgentToast {
            session_sid: "abc".into(),
            level: ToastLevel::Error,
            message: "boom".into(),
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&t).expect("ser");
        let back: AgentToast =
            rkyv::from_bytes::<AgentToast, rkyv::rancor::Error>(&bytes).expect("de");
        assert_eq!(back.session_sid, "abc");
        assert_eq!(back.level, ToastLevel::Error);
        assert_eq!(back.message, "boom");
    }
}
```

- [ ] **Step 2: Add rkyv to Cargo.toml**

Edit `crates/vmux_agent/Cargo.toml`. In `[dependencies]`, add (alphabetical order):

```toml
rkyv = { workspace = true }
```

If `rkyv` is not in workspace, instead use `rkyv = "0.8"`. Check `Cargo.toml` workspace `[workspace.dependencies]` first.

- [ ] **Step 3: Wire into lib.rs**

In `crates/vmux_agent/src/lib.rs` add module + re-export:

```rust
pub mod toast;
```

```rust
pub use toast::{AgentToast, ToastLevel};
```

- [ ] **Step 4: Run tests**

```bash
env -u CEF_PATH cargo test -p vmux_agent toast
```
Expected: 1 test passes.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_agent/src/toast.rs crates/vmux_agent/src/lib.rs crates/vmux_agent/Cargo.toml
git commit -m "feat(vmux_agent): add AgentToast event + ToastLevel"
```

---

## Task 5: Add mcp_tool_defs() bridge

**Files:**
- Create: `crates/vmux_agent/src/tools.rs`
- Modify: `crates/vmux_agent/src/lib.rs`
- Modify: `crates/vmux_agent/Cargo.toml`

Changed crates: `vmux_agent`

- [ ] **Step 1: Add vmux_mcp dependency**

Edit `crates/vmux_agent/Cargo.toml`, in `[dependencies]` (alphabetical):

```toml
vmux_mcp = { path = "../vmux_mcp" }
```

- [ ] **Step 2: Write the bridge + test**

Create `crates/vmux_agent/src/tools.rs`:

```rust
use crate::stream::ToolDef;

pub fn mcp_tool_defs() -> Vec<ToolDef> {
    vmux_mcp::tools::tool_definitions()
        .into_iter()
        .map(|d| ToolDef {
            name: Box::leak(d.name.into_boxed_str()),
            description: Box::leak(d.description.into_boxed_str()),
            input_schema: d.input_schema,
            read_only: false,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_at_least_one_tool() {
        let defs = mcp_tool_defs();
        assert!(!defs.is_empty(), "vmux_mcp must expose at least one tool");
        for d in &defs {
            assert!(!d.name.is_empty(), "tool name must not be empty");
            assert!(
                d.input_schema.is_object(),
                "tool schema must be a JSON object"
            );
        }
    }
}
```

Note: `Box::leak` is intentional. `ToolDef::name` and `description` are `&'static str` per the existing definition; leaking once per process is acceptable since the tool list is built at most a few times per session and never freed. If clippy flags `clippy::leaking_str` (it doesn't by default), add `#[allow(clippy::...)]` on the function with a one-line rationale comment.

- [ ] **Step 3: Wire into lib.rs**

In `crates/vmux_agent/src/lib.rs`:

```rust
pub mod tools;
```

```rust
pub use tools::mcp_tool_defs;
```

- [ ] **Step 4: Run tests**

```bash
env -u CEF_PATH cargo test -p vmux_agent tools::tests
```
Expected: 1 test passes.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_agent/Cargo.toml crates/vmux_agent/src/tools.rs crates/vmux_agent/src/lib.rs
git commit -m "feat(vmux_agent): mcp_tool_defs() bridges vmux_mcp::tools to ToolDef"
```

---

## Task 6: Extend Cargo.toml with reqwest stream + futures-util + mockito

**Files:**
- Modify: `crates/vmux_agent/Cargo.toml`

Changed crates: `vmux_agent`

- [ ] **Step 1: Update reqwest features + add futures-util + dev-deps**

Edit `crates/vmux_agent/Cargo.toml`:

Find the existing `reqwest` line and replace with:

```toml
reqwest = { version = "0.12", default-features = false, features = ["rustls-tls", "stream", "json"] }
```

Add to `[dependencies]` (alphabetical):

```toml
futures-util = "0.3"
```

Add a new section at the bottom:

```toml
[dev-dependencies]
mockito = "1"
```

- [ ] **Step 2: Build to verify deps resolve**

```bash
env -u CEF_PATH cargo build -p vmux_agent
```
Expected: success.

- [ ] **Step 3: Commit**

```bash
git add crates/vmux_agent/Cargo.toml
git commit -m "build(vmux_agent): add reqwest stream feature, futures-util, mockito dev-dep"
```

---

## Task 7: Implement SSE driver (`http::drive_sse`)

**Files:**
- Create: `crates/vmux_agent/src/http.rs`
- Modify: `crates/vmux_agent/src/lib.rs`

Changed crates: `vmux_agent`

- [ ] **Step 1: Write the driver + an inline unit test using mockito**

Create `crates/vmux_agent/src/http.rs`:

```rust
use std::sync::Arc;

use crossbeam_channel::Sender;
use futures_util::StreamExt;

use crate::app::AppAgentStrategy;
use crate::stream::StreamEvent;

pub async fn drive_sse(
    request: reqwest::Request,
    strategy: Arc<dyn AppAgentStrategy>,
    tx: Sender<StreamEvent>,
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
        let snippet: String = body.chars().take(500).collect();
        let _ = tx.send(StreamEvent::Error(format!("HTTP {status}: {snippet}")));
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
            let frame = frame.trim_end_matches('\n');
            if frame.is_empty() {
                continue;
            }
            if let Some(event) = strategy.parse_sse_event(frame) {
                if tx.send(event).is_err() {
                    return;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::strategy::AgentStrategy;
    use crate::stream::ToolDef;
    use crate::{AgentKind, AgentVariant};
    use crossbeam_channel::unbounded;

    struct EchoTextStrategy;
    impl AgentStrategy for EchoTextStrategy {
        fn kind(&self) -> AgentKind {
            AgentKind::Vibe
        }
        fn variant(&self) -> AgentVariant {
            AgentVariant::App
        }
    }
    impl AppAgentStrategy for EchoTextStrategy {
        fn provider(&self) -> &str {
            "echo"
        }
        fn model(&self) -> &str {
            "echo"
        }
        fn endpoint(&self) -> &str {
            "stub://"
        }
        fn env_var(&self) -> &'static str {
            ""
        }
        fn build_request(
            &self,
            _: &str,
            _: &[crate::message::Message],
            _: &[ToolDef],
            _: &str,
        ) -> reqwest::Request {
            unreachable!("test builds request manually")
        }
        fn parse_sse_event(&self, payload: &str) -> Option<StreamEvent> {
            payload
                .strip_prefix("data: ")
                .map(|s| StreamEvent::TextDelta(s.to_string()))
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn drives_two_text_deltas_from_mock_server() {
        let mut server = mockito::Server::new_async().await;
        let body = "data: hello\n\ndata: world\n\n";
        let _m = server
            .mock("POST", "/test")
            .with_status(200)
            .with_header("content-type", "text/event-stream")
            .with_body(body)
            .create_async()
            .await;
        let req = reqwest::Client::new()
            .post(format!("{}/test", server.url()))
            .build()
            .unwrap();
        let (tx, rx) = unbounded::<StreamEvent>();
        drive_sse(req, Arc::new(EchoTextStrategy), tx).await;
        let collected: Vec<StreamEvent> = rx.try_iter().collect();
        assert_eq!(
            collected,
            vec![
                StreamEvent::TextDelta("hello".into()),
                StreamEvent::TextDelta("world".into())
            ]
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn http_error_status_emits_error_event() {
        let mut server = mockito::Server::new_async().await;
        let _m = server
            .mock("POST", "/fail")
            .with_status(401)
            .with_body("unauthorized")
            .create_async()
            .await;
        let req = reqwest::Client::new()
            .post(format!("{}/fail", server.url()))
            .build()
            .unwrap();
        let (tx, rx) = unbounded::<StreamEvent>();
        drive_sse(req, Arc::new(EchoTextStrategy), tx).await;
        let collected: Vec<StreamEvent> = rx.try_iter().collect();
        assert_eq!(collected.len(), 1);
        match &collected[0] {
            StreamEvent::Error(msg) => {
                assert!(msg.contains("401"));
                assert!(msg.contains("unauthorized"));
            }
            other => panic!("expected Error, got {other:?}"),
        }
    }
}
```

- [ ] **Step 2: Add tokio dev-dep for the async tests**

Edit `crates/vmux_agent/Cargo.toml`. In `[dev-dependencies]`, add:

```toml
tokio = { version = "1", features = ["rt", "macros"] }
```

(Workspace `tokio` may already be configured; if so use `tokio = { workspace = true, features = ["rt", "macros"] }`.)

- [ ] **Step 3: Wire into lib.rs**

In `crates/vmux_agent/src/lib.rs`:

```rust
pub mod http;
```

- [ ] **Step 4: Run tests**

```bash
env -u CEF_PATH cargo test -p vmux_agent http::tests
```
Expected: 2 tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_agent/src/http.rs crates/vmux_agent/src/lib.rs crates/vmux_agent/Cargo.toml
git commit -m "feat(vmux_agent): drive_sse async function pumps SSE frames through strategy"
```

---

## Task 8: SSE fixtures (Mistral, OpenAI Responses, Anthropic)

**Files:**
- Create: `crates/vmux_agent/tests/fixtures/mistral/text.sse`
- Create: `crates/vmux_agent/tests/fixtures/mistral/tools.sse`
- Create: `crates/vmux_agent/tests/fixtures/openai/text.sse`
- Create: `crates/vmux_agent/tests/fixtures/openai/tools.sse`
- Create: `crates/vmux_agent/tests/fixtures/anthropic/text.sse`
- Create: `crates/vmux_agent/tests/fixtures/anthropic/tools.sse`

Changed crates: `vmux_agent`

Each fixture is a short raw SSE stream representing one realistic turn. Each `\n\n` separates frames. Trailing blank line at EOF intentional.

- [ ] **Step 1: Write Mistral text fixture**

Create `crates/vmux_agent/tests/fixtures/mistral/text.sse`:

```
data: {"id":"c1","object":"chat.completion.chunk","choices":[{"index":0,"delta":{"role":"assistant","content":"hello"},"finish_reason":null}]}

data: {"id":"c1","object":"chat.completion.chunk","choices":[{"index":0,"delta":{"content":" world"},"finish_reason":null}]}

data: {"id":"c1","object":"chat.completion.chunk","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}

data: [DONE]

```

- [ ] **Step 2: Write Mistral tools fixture**

Create `crates/vmux_agent/tests/fixtures/mistral/tools.sse`:

```
data: {"id":"c1","object":"chat.completion.chunk","choices":[{"index":0,"delta":{"role":"assistant","content":null,"tool_calls":[{"index":0,"id":"call_1","type":"function","function":{"name":"list_spaces","arguments":""}}]},"finish_reason":null}]}

data: {"id":"c1","object":"chat.completion.chunk","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"filter\":\"all\"}"}}]},"finish_reason":null}]}

data: {"id":"c1","object":"chat.completion.chunk","choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}]}

data: [DONE]

```

- [ ] **Step 3: Write OpenAI Responses text fixture**

Create `crates/vmux_agent/tests/fixtures/openai/text.sse`:

```
event: response.created
data: {"type":"response.created","response":{"id":"resp_1","status":"in_progress"}}

event: response.output_text.delta
data: {"type":"response.output_text.delta","delta":"hello"}

event: response.output_text.delta
data: {"type":"response.output_text.delta","delta":" world"}

event: response.completed
data: {"type":"response.completed","response":{"id":"resp_1","status":"completed"}}

```

- [ ] **Step 4: Write OpenAI Responses tools fixture**

Create `crates/vmux_agent/tests/fixtures/openai/tools.sse`:

```
event: response.output_item.added
data: {"type":"response.output_item.added","output_index":0,"item":{"type":"function_call","id":"call_1","name":"list_spaces"}}

event: response.function_call_arguments.delta
data: {"type":"response.function_call_arguments.delta","item_id":"call_1","delta":"{\"filter\":\"all\"}"}

event: response.output_item.done
data: {"type":"response.output_item.done","output_index":0,"item":{"type":"function_call","id":"call_1","name":"list_spaces","arguments":"{\"filter\":\"all\"}"}}

event: response.completed
data: {"type":"response.completed","response":{"id":"resp_1","status":"completed","stop_reason":"tool_use"}}

```

- [ ] **Step 5: Write Anthropic text fixture**

Create `crates/vmux_agent/tests/fixtures/anthropic/text.sse`:

```
event: message_start
data: {"type":"message_start","message":{"id":"m1","role":"assistant","content":[],"model":"claude-sonnet-4-6","stop_reason":null,"usage":{"input_tokens":10,"output_tokens":0}}}

event: content_block_start
data: {"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}

event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"hello"}}

event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":" world"}}

event: content_block_stop
data: {"type":"content_block_stop","index":0}

event: message_delta
data: {"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":2}}

event: message_stop
data: {"type":"message_stop"}

```

- [ ] **Step 6: Write Anthropic tools fixture**

Create `crates/vmux_agent/tests/fixtures/anthropic/tools.sse`:

```
event: message_start
data: {"type":"message_start","message":{"id":"m1","role":"assistant","content":[],"model":"claude-sonnet-4-6","stop_reason":null,"usage":{"input_tokens":10,"output_tokens":0}}}

event: content_block_start
data: {"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"tool_1","name":"list_spaces","input":{}}}

event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{\"filter\":\"all\"}"}}

event: content_block_stop
data: {"type":"content_block_stop","index":0}

event: message_delta
data: {"type":"message_delta","delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":5}}

event: message_stop
data: {"type":"message_stop"}

```

- [ ] **Step 7: Commit**

```bash
git add crates/vmux_agent/tests/fixtures
git commit -m "test(vmux_agent): add SSE fixtures for mistral/openai-responses/anthropic"
```

---

## Task 9: OpenAI chat-completions shared parser (used by Mistral)

**Files:**
- Create: `crates/vmux_agent/src/providers.rs`
- Create: `crates/vmux_agent/src/providers/openai_shared.rs`
- Modify: `crates/vmux_agent/src/lib.rs`

Changed crates: `vmux_agent`

- [ ] **Step 1: Module root**

Create `crates/vmux_agent/src/providers.rs`:

```rust
pub mod openai_shared;
```

- [ ] **Step 2: Write the parser + helpers + tests**

Create `crates/vmux_agent/src/providers/openai_shared.rs`:

```rust
use serde::Deserialize;
use serde_json::{Value, json};

use crate::message::{AssistantBlock, Message};
use crate::stream::{StopReason, StreamEvent, ToolDef};

#[derive(Deserialize)]
struct ChunkRoot<'a> {
    #[serde(borrow)]
    choices: Vec<Choice<'a>>,
}

#[derive(Deserialize)]
struct Choice<'a> {
    #[serde(borrow, default)]
    delta: Delta<'a>,
    #[serde(borrow, default)]
    finish_reason: Option<&'a str>,
}

#[derive(Deserialize, Default)]
struct Delta<'a> {
    #[serde(borrow, default)]
    content: Option<&'a str>,
    #[serde(borrow, default)]
    tool_calls: Option<Vec<ToolCallDelta<'a>>>,
}

#[derive(Deserialize)]
struct ToolCallDelta<'a> {
    #[serde(default)]
    index: usize,
    #[serde(borrow, default)]
    id: Option<&'a str>,
    #[serde(borrow, default)]
    function: Option<FunctionDelta<'a>>,
}

#[derive(Deserialize)]
struct FunctionDelta<'a> {
    #[serde(borrow, default)]
    name: Option<&'a str>,
    #[serde(borrow, default)]
    arguments: Option<&'a str>,
}

pub fn parse_chat_completions_sse(frame: &str) -> Option<StreamEvent> {
    let payload = frame.lines().find_map(|line| line.strip_prefix("data: "))?;
    if payload.trim() == "[DONE]" {
        return None;
    }
    let chunk: ChunkRoot = serde_json::from_str(payload).ok()?;
    let choice = chunk.choices.into_iter().next()?;
    if let Some(reason) = choice.finish_reason {
        return Some(StreamEvent::StopTurn {
            reason: match reason {
                "stop" => StopReason::EndTurn,
                "tool_calls" => StopReason::ToolUse,
                "length" => StopReason::MaxTokens,
                _ => StopReason::Other,
            },
        });
    }
    if let Some(text) = choice.delta.content {
        if !text.is_empty() {
            return Some(StreamEvent::TextDelta(text.to_string()));
        }
    }
    if let Some(calls) = choice.delta.tool_calls {
        let call = calls.into_iter().next()?;
        if let Some(id) = call.id {
            if let Some(func) = &call.function {
                if let Some(name) = func.name {
                    return Some(StreamEvent::ToolUseStart {
                        call_id: id.to_string(),
                        name: name.to_string(),
                    });
                }
            }
            return Some(StreamEvent::ToolUseStart {
                call_id: id.to_string(),
                name: String::new(),
            });
        }
        if let Some(func) = call.function {
            if let Some(args) = func.arguments {
                return Some(StreamEvent::ToolUseArgsDelta {
                    call_id: String::new(),
                    json_chunk: args.to_string(),
                });
            }
        }
    }
    None
}

pub fn messages_to_chat_completions(messages: &[Message]) -> Vec<Value> {
    let mut out = Vec::new();
    for msg in messages {
        match msg {
            Message::User { text } => out.push(json!({"role":"user","content":text})),
            Message::Assistant { blocks } => {
                let mut content = String::new();
                let mut tool_calls = Vec::new();
                for b in blocks {
                    match b {
                        AssistantBlock::Text(t) => content.push_str(t),
                        AssistantBlock::ToolUse {
                            call_id,
                            name,
                            args,
                        } => tool_calls.push(json!({
                            "id": call_id,
                            "type":"function",
                            "function": {"name": name, "arguments": args}
                        })),
                    }
                }
                let mut obj = json!({"role":"assistant","content": content});
                if !tool_calls.is_empty() {
                    obj["tool_calls"] = json!(tool_calls);
                }
                out.push(obj);
            }
            Message::ToolResult {
                call_id,
                content,
                is_error,
            } => out.push(json!({
                "role":"tool",
                "tool_call_id": call_id,
                "content": if *is_error { format!("ERROR: {content}") } else { content.clone() }
            })),
        }
    }
    out
}

pub fn tools_to_function_specs(tools: &[ToolDef]) -> Vec<Value> {
    tools
        .iter()
        .map(|t| {
            json!({
                "type": "function",
                "function": {
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.input_schema,
                }
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const MISTRAL_TEXT: &str = include_str!("../../tests/fixtures/mistral/text.sse");
    const MISTRAL_TOOLS: &str = include_str!("../../tests/fixtures/mistral/tools.sse");

    fn frames(raw: &str) -> Vec<&str> {
        raw.split("\n\n").filter(|s| !s.trim().is_empty()).collect()
    }

    #[test]
    fn parses_text_then_stop() {
        let events: Vec<StreamEvent> = frames(MISTRAL_TEXT)
            .into_iter()
            .filter_map(parse_chat_completions_sse)
            .collect();
        assert_eq!(events.len(), 3);
        assert_eq!(events[0], StreamEvent::TextDelta("hello".into()));
        assert_eq!(events[1], StreamEvent::TextDelta(" world".into()));
        assert!(matches!(
            events[2],
            StreamEvent::StopTurn {
                reason: StopReason::EndTurn
            }
        ));
    }

    #[test]
    fn parses_tool_call_sequence() {
        let events: Vec<StreamEvent> = frames(MISTRAL_TOOLS)
            .into_iter()
            .filter_map(parse_chat_completions_sse)
            .collect();
        match &events[0] {
            StreamEvent::ToolUseStart { call_id, name } => {
                assert_eq!(call_id, "call_1");
                assert_eq!(name, "list_spaces");
            }
            other => panic!("expected ToolUseStart, got {other:?}"),
        }
        match &events[1] {
            StreamEvent::ToolUseArgsDelta { json_chunk, .. } => {
                assert_eq!(json_chunk, "{\"filter\":\"all\"}");
            }
            other => panic!("expected ToolUseArgsDelta, got {other:?}"),
        }
        assert!(matches!(
            events[2],
            StreamEvent::StopTurn {
                reason: StopReason::ToolUse
            }
        ));
    }

    #[test]
    fn messages_to_chat_completions_roundtrip() {
        let msgs = vec![
            Message::User { text: "hi".into() },
            Message::Assistant {
                blocks: vec![AssistantBlock::Text("hello".into())],
            },
            Message::ToolResult {
                call_id: "c1".into(),
                content: "ok".into(),
                is_error: false,
            },
        ];
        let out = messages_to_chat_completions(&msgs);
        assert_eq!(out[0]["role"], "user");
        assert_eq!(out[1]["role"], "assistant");
        assert_eq!(out[2]["role"], "tool");
        assert_eq!(out[2]["tool_call_id"], "c1");
    }

    #[test]
    fn tools_to_function_specs_shape() {
        let tools = vec![ToolDef {
            name: "list_spaces",
            description: "desc",
            input_schema: json!({"type":"object"}),
            read_only: true,
        }];
        let out = tools_to_function_specs(&tools);
        assert_eq!(out[0]["type"], "function");
        assert_eq!(out[0]["function"]["name"], "list_spaces");
    }
}
```

Note: `StreamEvent::ToolUseArgsDelta` is emitted with an empty `call_id` because chat-completions chunks only repeat `index`, not `id`, on continuation deltas. `drain_stream` correlates by relying on the in-progress `PartialToolUse` in `AgentRunState::Streaming.partial`.

- [ ] **Step 3: Wire into lib.rs**

In `crates/vmux_agent/src/lib.rs`:

```rust
pub mod providers;
```

- [ ] **Step 4: Run tests**

```bash
env -u CEF_PATH cargo test -p vmux_agent providers::openai_shared
```
Expected: 4 tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_agent/src/providers.rs crates/vmux_agent/src/providers/openai_shared.rs crates/vmux_agent/src/lib.rs
git commit -m "feat(vmux_agent): chat-completions SSE parser + message/tool helpers"
```

---

## Task 10: MistralStrategy

**Files:**
- Create: `crates/vmux_agent/src/providers/mistral.rs`
- Modify: `crates/vmux_agent/src/providers.rs`

Changed crates: `vmux_agent`

- [ ] **Step 1: Write strategy + tests**

Create `crates/vmux_agent/src/providers/mistral.rs`:

```rust
use serde_json::json;

use crate::app::AppAgentStrategy;
use crate::message::Message;
use crate::providers::openai_shared::{
    messages_to_chat_completions, parse_chat_completions_sse, tools_to_function_specs,
};
use crate::strategy::AgentStrategy;
use crate::stream::{StreamEvent, ToolDef};
use crate::{AgentKind, AgentVariant};

pub struct MistralStrategy {
    provider: String,
    model: String,
}

impl MistralStrategy {
    pub fn new(provider: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
            model: model.into(),
        }
    }
}

impl AgentStrategy for MistralStrategy {
    fn kind(&self) -> AgentKind {
        AgentKind::Vibe
    }
    fn variant(&self) -> AgentVariant {
        AgentVariant::App
    }
}

impl AppAgentStrategy for MistralStrategy {
    fn provider(&self) -> &str {
        &self.provider
    }
    fn model(&self) -> &str {
        &self.model
    }
    fn endpoint(&self) -> &str {
        "https://api.mistral.ai/v1/chat/completions"
    }
    fn env_var(&self) -> &'static str {
        "MISTRAL_API_KEY"
    }

    fn build_request(
        &self,
        model: &str,
        messages: &[Message],
        tools: &[ToolDef],
        api_key: &str,
    ) -> reqwest::Request {
        let mut body = json!({
            "model": model,
            "messages": messages_to_chat_completions(messages),
            "stream": true,
        });
        if !tools.is_empty() {
            body["tools"] = json!(tools_to_function_specs(tools));
            body["tool_choice"] = json!("auto");
        }
        reqwest::Client::new()
            .post(self.endpoint())
            .bearer_auth(api_key)
            .header("Accept", "text/event-stream")
            .header("Content-Type", "application/json")
            .json(&body)
            .build()
            .expect("MistralStrategy: build_request")
    }

    fn parse_sse_event(&self, payload: &str) -> Option<StreamEvent> {
        parse_chat_completions_sse(payload)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_request_sets_headers_and_url() {
        let s = MistralStrategy::new("mistral", "devstral-2");
        let msgs = vec![Message::User {
            text: "hi".into(),
        }];
        let req = s.build_request("devstral-2", &msgs, &[], "test-key");
        assert_eq!(req.url().as_str(), s.endpoint());
        let auth = req
            .headers()
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default();
        assert_eq!(auth, "Bearer test-key");
        let body = req.body().unwrap().as_bytes().unwrap();
        let parsed: serde_json::Value = serde_json::from_slice(body).unwrap();
        assert_eq!(parsed["model"], "devstral-2");
        assert_eq!(parsed["stream"], true);
        assert_eq!(parsed["messages"][0]["role"], "user");
    }

    #[test]
    fn parse_sse_event_delegates_to_shared_parser() {
        let s = MistralStrategy::new("mistral", "devstral-2");
        let frame = r#"data: {"id":"c1","choices":[{"index":0,"delta":{"content":"hi"},"finish_reason":null}]}"#;
        assert_eq!(
            s.parse_sse_event(frame),
            Some(StreamEvent::TextDelta("hi".into()))
        );
    }
}
```

- [ ] **Step 2: Register in providers.rs**

Edit `crates/vmux_agent/src/providers.rs`:

```rust
pub mod mistral;
pub mod openai_shared;

pub use mistral::MistralStrategy;
```

- [ ] **Step 3: Run tests**

```bash
env -u CEF_PATH cargo test -p vmux_agent providers::mistral
```
Expected: 2 tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/vmux_agent/src/providers/mistral.rs crates/vmux_agent/src/providers.rs
git commit -m "feat(vmux_agent): MistralStrategy hits chat completions with shared parser"
```

---

## Task 11: OpenAiResponsesStrategy

**Files:**
- Create: `crates/vmux_agent/src/providers/openai.rs`
- Modify: `crates/vmux_agent/src/providers.rs`

Changed crates: `vmux_agent`

- [ ] **Step 1: Write strategy + parser + tests**

Create `crates/vmux_agent/src/providers/openai.rs`:

```rust
use serde::Deserialize;
use serde_json::{Value, json};

use crate::app::AppAgentStrategy;
use crate::message::{AssistantBlock, Message};
use crate::providers::openai_shared::tools_to_function_specs;
use crate::strategy::AgentStrategy;
use crate::stream::{StopReason, StreamEvent, ToolDef};
use crate::{AgentKind, AgentVariant};

pub struct OpenAiResponsesStrategy {
    provider: String,
    model: String,
}

impl OpenAiResponsesStrategy {
    pub fn new(provider: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
            model: model.into(),
        }
    }
}

impl AgentStrategy for OpenAiResponsesStrategy {
    fn kind(&self) -> AgentKind {
        AgentKind::Codex
    }
    fn variant(&self) -> AgentVariant {
        AgentVariant::App
    }
}

impl AppAgentStrategy for OpenAiResponsesStrategy {
    fn provider(&self) -> &str {
        &self.provider
    }
    fn model(&self) -> &str {
        &self.model
    }
    fn endpoint(&self) -> &str {
        "https://api.openai.com/v1/responses"
    }
    fn env_var(&self) -> &'static str {
        "OPENAI_API_KEY"
    }

    fn build_request(
        &self,
        model: &str,
        messages: &[Message],
        tools: &[ToolDef],
        api_key: &str,
    ) -> reqwest::Request {
        let mut body = json!({
            "model": model,
            "input": messages_to_responses_input(messages),
            "stream": true,
        });
        if !tools.is_empty() {
            body["tools"] = json!(tools_to_responses_tools(tools));
        }
        reqwest::Client::new()
            .post(self.endpoint())
            .bearer_auth(api_key)
            .header("Accept", "text/event-stream")
            .header("Content-Type", "application/json")
            .json(&body)
            .build()
            .expect("OpenAiResponsesStrategy: build_request")
    }

    fn parse_sse_event(&self, payload: &str) -> Option<StreamEvent> {
        parse_responses_sse(payload)
    }
}

fn messages_to_responses_input(messages: &[Message]) -> Vec<Value> {
    let mut out = Vec::new();
    for msg in messages {
        match msg {
            Message::User { text } => out.push(json!({
                "type":"message","role":"user","content":[{"type":"input_text","text":text}]
            })),
            Message::Assistant { blocks } => {
                let mut content_parts = Vec::new();
                for b in blocks {
                    match b {
                        AssistantBlock::Text(t) => content_parts.push(json!({"type":"output_text","text":t})),
                        AssistantBlock::ToolUse {
                            call_id,
                            name,
                            args,
                        } => out.push(json!({
                            "type":"function_call","call_id":call_id,"name":name,"arguments":args
                        })),
                    }
                }
                if !content_parts.is_empty() {
                    out.push(json!({"type":"message","role":"assistant","content":content_parts}));
                }
            }
            Message::ToolResult {
                call_id,
                content,
                is_error,
            } => out.push(json!({
                "type":"function_call_output","call_id":call_id,
                "output": if *is_error { format!("ERROR: {content}") } else { content.clone() }
            })),
        }
    }
    out
}

fn tools_to_responses_tools(tools: &[ToolDef]) -> Vec<Value> {
    tools_to_function_specs(tools)
        .into_iter()
        .map(|spec| {
            json!({
                "type":"function",
                "name": spec["function"]["name"],
                "description": spec["function"]["description"],
                "parameters": spec["function"]["parameters"],
            })
        })
        .collect()
}

#[derive(Deserialize)]
#[serde(tag = "type")]
enum ResponsesEvent {
    #[serde(rename = "response.output_text.delta")]
    TextDelta { delta: String },
    #[serde(rename = "response.output_item.added")]
    ItemAdded { item: ItemAdded },
    #[serde(rename = "response.function_call_arguments.delta")]
    ArgsDelta { item_id: String, delta: String },
    #[serde(rename = "response.output_item.done")]
    ItemDone {
        #[serde(default)]
        item: ItemDone,
    },
    #[serde(rename = "response.completed")]
    Completed {
        #[serde(default)]
        response: CompletedResponse,
    },
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
struct ItemAdded {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: String,
}

#[derive(Deserialize, Default)]
struct ItemDone {
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(default)]
    id: String,
}

#[derive(Deserialize, Default)]
struct CompletedResponse {
    #[serde(default)]
    stop_reason: String,
}

pub fn parse_responses_sse(frame: &str) -> Option<StreamEvent> {
    let data = frame.lines().find_map(|line| line.strip_prefix("data: "))?;
    let evt: ResponsesEvent = serde_json::from_str(data).ok()?;
    match evt {
        ResponsesEvent::TextDelta { delta } => Some(StreamEvent::TextDelta(delta)),
        ResponsesEvent::ItemAdded { kind, id, name } if kind == "function_call" => {
            Some(StreamEvent::ToolUseStart {
                call_id: id,
                name,
            })
        }
        ResponsesEvent::ArgsDelta { item_id, delta } => Some(StreamEvent::ToolUseArgsDelta {
            call_id: item_id,
            json_chunk: delta,
        }),
        ResponsesEvent::ItemDone { kind, id } if kind == "function_call" => {
            Some(StreamEvent::ToolUseEnd { call_id: id })
        }
        ResponsesEvent::Completed { response } => {
            let reason = match response.stop_reason.as_str() {
                "tool_use" => StopReason::ToolUse,
                "length" => StopReason::MaxTokens,
                _ => StopReason::EndTurn,
            };
            Some(StreamEvent::StopTurn { reason })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT: &str = include_str!("../../tests/fixtures/openai/text.sse");
    const TOOLS: &str = include_str!("../../tests/fixtures/openai/tools.sse");

    fn frames(raw: &str) -> Vec<&str> {
        raw.split("\n\n").filter(|s| !s.trim().is_empty()).collect()
    }

    #[test]
    fn parses_text_then_completed_end_turn() {
        let events: Vec<StreamEvent> = frames(TEXT)
            .into_iter()
            .filter_map(parse_responses_sse)
            .collect();
        assert!(events.contains(&StreamEvent::TextDelta("hello".into())));
        assert!(events.contains(&StreamEvent::TextDelta(" world".into())));
        assert!(events.iter().any(|e| matches!(
            e,
            StreamEvent::StopTurn {
                reason: StopReason::EndTurn
            }
        )));
    }

    #[test]
    fn parses_tool_call_start_args_end_completed_tool_use() {
        let events: Vec<StreamEvent> = frames(TOOLS)
            .into_iter()
            .filter_map(parse_responses_sse)
            .collect();
        let has_start = events.iter().any(|e| {
            matches!(e, StreamEvent::ToolUseStart{call_id, name} if call_id == "call_1" && name == "list_spaces")
        });
        let has_args = events.iter().any(|e| {
            matches!(e, StreamEvent::ToolUseArgsDelta{json_chunk, ..} if json_chunk == "{\"filter\":\"all\"}")
        });
        let has_end = events
            .iter()
            .any(|e| matches!(e, StreamEvent::ToolUseEnd { call_id } if call_id == "call_1"));
        let has_stop = events.iter().any(|e| {
            matches!(
                e,
                StreamEvent::StopTurn {
                    reason: StopReason::ToolUse
                }
            )
        });
        assert!(has_start && has_args && has_end && has_stop, "{events:?}");
    }

    #[test]
    fn build_request_uses_responses_endpoint_and_bearer_auth() {
        let s = OpenAiResponsesStrategy::new("openai", "gpt-5");
        let msgs = vec![Message::User {
            text: "hi".into(),
        }];
        let req = s.build_request("gpt-5", &msgs, &[], "test-key");
        assert_eq!(req.url().as_str(), s.endpoint());
        assert_eq!(
            req.headers().get("authorization").unwrap(),
            "Bearer test-key"
        );
        let body: serde_json::Value =
            serde_json::from_slice(req.body().unwrap().as_bytes().unwrap()).unwrap();
        assert_eq!(body["model"], "gpt-5");
        assert_eq!(body["stream"], true);
        assert_eq!(body["input"][0]["type"], "message");
    }
}
```

- [ ] **Step 2: Register in providers.rs**

Edit `crates/vmux_agent/src/providers.rs`:

```rust
pub mod mistral;
pub mod openai;
pub mod openai_shared;

pub use mistral::MistralStrategy;
pub use openai::OpenAiResponsesStrategy;
```

- [ ] **Step 3: Run tests**

```bash
env -u CEF_PATH cargo test -p vmux_agent providers::openai
```
Expected: 3 tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/vmux_agent/src/providers/openai.rs crates/vmux_agent/src/providers.rs
git commit -m "feat(vmux_agent): OpenAiResponsesStrategy with Responses-API SSE parser"
```

---

## Task 12: AnthropicStrategy

**Files:**
- Create: `crates/vmux_agent/src/providers/anthropic.rs`
- Modify: `crates/vmux_agent/src/providers.rs`

Changed crates: `vmux_agent`

- [ ] **Step 1: Write strategy + helpers + tests**

Create `crates/vmux_agent/src/providers/anthropic.rs`:

```rust
use serde::Deserialize;
use serde_json::{Value, json};

use crate::app::AppAgentStrategy;
use crate::message::{AssistantBlock, Message};
use crate::strategy::AgentStrategy;
use crate::stream::{StopReason, StreamEvent, ToolDef};
use crate::{AgentKind, AgentVariant};

pub struct AnthropicStrategy {
    provider: String,
    model: String,
}

impl AnthropicStrategy {
    pub fn new(provider: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
            model: model.into(),
        }
    }
}

impl AgentStrategy for AnthropicStrategy {
    fn kind(&self) -> AgentKind {
        AgentKind::Claude
    }
    fn variant(&self) -> AgentVariant {
        AgentVariant::App
    }
}

impl AppAgentStrategy for AnthropicStrategy {
    fn provider(&self) -> &str {
        &self.provider
    }
    fn model(&self) -> &str {
        &self.model
    }
    fn endpoint(&self) -> &str {
        "https://api.anthropic.com/v1/messages"
    }
    fn env_var(&self) -> &'static str {
        "ANTHROPIC_API_KEY"
    }

    fn build_request(
        &self,
        model: &str,
        messages: &[Message],
        tools: &[ToolDef],
        api_key: &str,
    ) -> reqwest::Request {
        let mut body = json!({
            "model": model,
            "max_tokens": 8192,
            "messages": messages_to_anthropic_blocks(messages),
            "stream": true,
        });
        if !tools.is_empty() {
            body["tools"] = json!(tools_to_anthropic(tools));
        }
        reqwest::Client::new()
            .post(self.endpoint())
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .header("Accept", "text/event-stream")
            .header("Content-Type", "application/json")
            .json(&body)
            .build()
            .expect("AnthropicStrategy: build_request")
    }

    fn parse_sse_event(&self, payload: &str) -> Option<StreamEvent> {
        parse_messages_sse(payload)
    }
}

fn messages_to_anthropic_blocks(messages: &[Message]) -> Vec<Value> {
    let mut out = Vec::new();
    for msg in messages {
        match msg {
            Message::User { text } => out.push(json!({
                "role":"user",
                "content":[{"type":"text","text":text}]
            })),
            Message::Assistant { blocks } => {
                let content: Vec<Value> = blocks
                    .iter()
                    .map(|b| match b {
                        AssistantBlock::Text(t) => json!({"type":"text","text":t}),
                        AssistantBlock::ToolUse {
                            call_id,
                            name,
                            args,
                        } => json!({
                            "type":"tool_use",
                            "id":call_id,
                            "name":name,
                            "input": serde_json::from_str::<Value>(args).unwrap_or(json!({}))
                        }),
                    })
                    .collect();
                out.push(json!({"role":"assistant","content": content}));
            }
            Message::ToolResult {
                call_id,
                content,
                is_error,
            } => out.push(json!({
                "role":"user",
                "content":[{
                    "type":"tool_result",
                    "tool_use_id":call_id,
                    "content":content,
                    "is_error":*is_error
                }]
            })),
        }
    }
    out
}

fn tools_to_anthropic(tools: &[ToolDef]) -> Vec<Value> {
    tools
        .iter()
        .map(|t| {
            json!({
                "name": t.name,
                "description": t.description,
                "input_schema": t.input_schema,
            })
        })
        .collect()
}

#[derive(Deserialize)]
#[serde(tag = "type")]
enum MessagesEvent {
    #[serde(rename = "content_block_start")]
    BlockStart {
        index: usize,
        content_block: BlockStart,
    },
    #[serde(rename = "content_block_delta")]
    BlockDelta {
        index: usize,
        delta: BlockDelta,
    },
    #[serde(rename = "content_block_stop")]
    BlockStop {
        index: usize,
    },
    #[serde(rename = "message_delta")]
    MessageDelta {
        delta: MessageStopDelta,
    },
    #[serde(rename = "message_stop")]
    MessageStop,
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
#[serde(tag = "type")]
enum BlockStart {
    #[serde(rename = "text")]
    Text {
        #[serde(default)]
        text: String,
    },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
    },
}

#[derive(Deserialize)]
#[serde(tag = "type")]
enum BlockDelta {
    #[serde(rename = "text_delta")]
    Text { text: String },
    #[serde(rename = "input_json_delta")]
    JsonDelta { partial_json: String },
}

#[derive(Deserialize, Default)]
struct MessageStopDelta {
    #[serde(default)]
    stop_reason: Option<String>,
}

pub fn parse_messages_sse(frame: &str) -> Option<StreamEvent> {
    let data = frame.lines().find_map(|line| line.strip_prefix("data: "))?;
    let evt: MessagesEvent = serde_json::from_str(data).ok()?;
    match evt {
        MessagesEvent::BlockStart { content_block, .. } => match content_block {
            BlockStart::Text { text } if !text.is_empty() => Some(StreamEvent::TextDelta(text)),
            BlockStart::Text { .. } => None,
            BlockStart::ToolUse { id, name } => Some(StreamEvent::ToolUseStart {
                call_id: id,
                name,
            }),
        },
        MessagesEvent::BlockDelta { delta, .. } => match delta {
            BlockDelta::Text { text } => Some(StreamEvent::TextDelta(text)),
            BlockDelta::JsonDelta { partial_json } => Some(StreamEvent::ToolUseArgsDelta {
                call_id: String::new(),
                json_chunk: partial_json,
            }),
        },
        MessagesEvent::BlockStop { .. } => Some(StreamEvent::ToolUseEnd {
            call_id: String::new(),
        }),
        MessagesEvent::MessageDelta { delta } => {
            let reason = match delta.stop_reason.as_deref() {
                Some("tool_use") => StopReason::ToolUse,
                Some("max_tokens") => StopReason::MaxTokens,
                Some(_) => StopReason::EndTurn,
                None => return None,
            };
            Some(StreamEvent::StopTurn { reason })
        }
        MessagesEvent::MessageStop | MessagesEvent::Other => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT: &str = include_str!("../../tests/fixtures/anthropic/text.sse");
    const TOOLS: &str = include_str!("../../tests/fixtures/anthropic/tools.sse");

    fn frames(raw: &str) -> Vec<&str> {
        raw.split("\n\n").filter(|s| !s.trim().is_empty()).collect()
    }

    #[test]
    fn parses_text_block_into_deltas_then_end_turn() {
        let events: Vec<StreamEvent> = frames(TEXT)
            .into_iter()
            .filter_map(parse_messages_sse)
            .collect();
        assert!(events.contains(&StreamEvent::TextDelta("hello".into())));
        assert!(events.contains(&StreamEvent::TextDelta(" world".into())));
        assert!(events.iter().any(|e| matches!(
            e,
            StreamEvent::StopTurn {
                reason: StopReason::EndTurn
            }
        )));
    }

    #[test]
    fn parses_tool_use_block() {
        let events: Vec<StreamEvent> = frames(TOOLS)
            .into_iter()
            .filter_map(parse_messages_sse)
            .collect();
        assert!(events.iter().any(|e| matches!(
            e,
            StreamEvent::ToolUseStart { call_id, name } if call_id == "tool_1" && name == "list_spaces"
        )));
        assert!(events.iter().any(|e| matches!(
            e,
            StreamEvent::ToolUseArgsDelta { json_chunk, .. } if json_chunk == "{\"filter\":\"all\"}"
        )));
        assert!(events.iter().any(|e| matches!(e, StreamEvent::ToolUseEnd { .. })));
        assert!(events.iter().any(|e| matches!(
            e,
            StreamEvent::StopTurn {
                reason: StopReason::ToolUse
            }
        )));
    }

    #[test]
    fn build_request_sets_x_api_key_and_version_header() {
        let s = AnthropicStrategy::new("anthropic", "claude-sonnet-4-6");
        let msgs = vec![Message::User { text: "hi".into() }];
        let req = s.build_request("claude-sonnet-4-6", &msgs, &[], "test-key");
        assert_eq!(req.url().as_str(), s.endpoint());
        assert_eq!(req.headers().get("x-api-key").unwrap(), "test-key");
        assert_eq!(
            req.headers().get("anthropic-version").unwrap(),
            "2023-06-01"
        );
    }
}
```

Note: Anthropic emits `ToolUseStart`/`ToolUseEnd` with empty `call_id` for `content_block_*` events because the `id` lives on `BlockStart::ToolUse` only. `drain_stream` tracks the in-progress tool via `partial: PartialToolUse` (already exists), so the empty call_id on Args/End is fine — `drain_stream` uses the partial's stored id when finalizing.

- [ ] **Step 2: Register in providers.rs**

Edit `crates/vmux_agent/src/providers.rs`:

```rust
pub mod anthropic;
pub mod mistral;
pub mod openai;
pub mod openai_shared;

pub use anthropic::AnthropicStrategy;
pub use mistral::MistralStrategy;
pub use openai::OpenAiResponsesStrategy;
```

- [ ] **Step 3: Run tests**

```bash
env -u CEF_PATH cargo test -p vmux_agent providers::anthropic
```
Expected: 3 tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/vmux_agent/src/providers/anthropic.rs crates/vmux_agent/src/providers.rs
git commit -m "feat(vmux_agent): AnthropicStrategy with Messages SSE parser"
```

---

## Task 13: BUILTIN_PROVIDERS registry + default resolver

**Files:**
- Create: `crates/vmux_agent/src/providers/builtin.rs`
- Modify: `crates/vmux_agent/src/providers.rs`
- Modify: `crates/vmux_agent/src/lib.rs`

Changed crates: `vmux_agent`

- [ ] **Step 1: Write the builtin registry + tests**

Create `crates/vmux_agent/src/providers/builtin.rs`:

```rust
use std::sync::Arc;

use crate::app::AppAgentStrategy;
use crate::providers::anthropic::AnthropicStrategy;
use crate::providers::mistral::MistralStrategy;
use crate::providers::openai::OpenAiResponsesStrategy;
use crate::AgentKind;

#[derive(Copy, Clone, Debug)]
pub struct BuiltinProvider {
    pub provider: &'static str,
    pub kind: AgentKind,
    pub default_model: &'static str,
    pub env_var: &'static str,
}

pub const BUILTIN_PROVIDERS: &[BuiltinProvider] = &[
    BuiltinProvider {
        provider: "mistral",
        kind: AgentKind::Vibe,
        default_model: "devstral-2",
        env_var: "MISTRAL_API_KEY",
    },
    BuiltinProvider {
        provider: "anthropic",
        kind: AgentKind::Claude,
        default_model: "claude-sonnet-4-6",
        env_var: "ANTHROPIC_API_KEY",
    },
    BuiltinProvider {
        provider: "openai",
        kind: AgentKind::Codex,
        default_model: "gpt-5",
        env_var: "OPENAI_API_KEY",
    },
];

pub fn resolve_default_app_provider() -> Option<&'static BuiltinProvider> {
    BUILTIN_PROVIDERS
        .iter()
        .find(|p| std::env::var(p.env_var).is_ok())
}

pub fn instantiate_builtin(p: &BuiltinProvider, model: &str) -> Arc<dyn AppAgentStrategy> {
    match p.provider {
        "mistral" => Arc::new(MistralStrategy::new(p.provider, model.to_string())),
        "anthropic" => Arc::new(AnthropicStrategy::new(p.provider, model.to_string())),
        "openai" => Arc::new(OpenAiResponsesStrategy::new(p.provider, model.to_string())),
        other => panic!("instantiate_builtin: unknown provider '{other}'"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clear_all_keys() {
        for p in BUILTIN_PROVIDERS {
            // SAFETY: tests are gated single-threaded by the `#[serial]` attribute below.
            unsafe { std::env::remove_var(p.env_var) };
        }
    }

    #[test]
    fn priority_is_mistral_then_anthropic_then_openai() {
        clear_all_keys();
        unsafe { std::env::set_var("MISTRAL_API_KEY", "x") };
        unsafe { std::env::set_var("ANTHROPIC_API_KEY", "y") };
        unsafe { std::env::set_var("OPENAI_API_KEY", "z") };
        let p = resolve_default_app_provider().unwrap();
        assert_eq!(p.provider, "mistral");
        clear_all_keys();
    }

    #[test]
    fn anthropic_wins_when_mistral_absent() {
        clear_all_keys();
        unsafe { std::env::set_var("ANTHROPIC_API_KEY", "y") };
        unsafe { std::env::set_var("OPENAI_API_KEY", "z") };
        let p = resolve_default_app_provider().unwrap();
        assert_eq!(p.provider, "anthropic");
        clear_all_keys();
    }

    #[test]
    fn no_keys_returns_none() {
        clear_all_keys();
        assert!(resolve_default_app_provider().is_none());
    }

    #[test]
    fn instantiate_returns_correct_strategy_type() {
        let bp = &BUILTIN_PROVIDERS[0];
        let s = instantiate_builtin(bp, "devstral-2");
        assert_eq!(s.provider(), "mistral");
        assert_eq!(s.model(), "devstral-2");
    }
}
```

Note: `std::env::set_var/remove_var` are `unsafe` in Rust 2024 because env mutation isn't thread-safe. To avoid flakes across parallel tests, mark the three env-mutating tests with a serial guard. Easiest: depend on `serial_test = "3"` in `[dev-dependencies]` and annotate each with `#[serial_test::serial]`. Add the dep at the end of Cargo.toml's `[dev-dependencies]`:

```toml
serial_test = "3"
```

Then add `use serial_test::serial;` at the top of `mod tests` and `#[serial]` on each env-mutating test (`priority_is_*`, `anthropic_wins_*`, `no_keys_*`).

- [ ] **Step 2: Wire into providers.rs and lib.rs**

Edit `crates/vmux_agent/src/providers.rs`:

```rust
pub mod anthropic;
pub mod builtin;
pub mod mistral;
pub mod openai;
pub mod openai_shared;

pub use anthropic::AnthropicStrategy;
pub use builtin::{BUILTIN_PROVIDERS, BuiltinProvider, instantiate_builtin, resolve_default_app_provider};
pub use mistral::MistralStrategy;
pub use openai::OpenAiResponsesStrategy;
```

In `crates/vmux_agent/src/lib.rs`, re-export at top level:

```rust
pub use providers::{
    AnthropicStrategy, BUILTIN_PROVIDERS, BuiltinProvider, MistralStrategy,
    OpenAiResponsesStrategy, instantiate_builtin, resolve_default_app_provider,
};
```

- [ ] **Step 3: Run tests**

```bash
env -u CEF_PATH cargo test -p vmux_agent providers::builtin
```
Expected: 4 tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/vmux_agent/src/providers/builtin.rs crates/vmux_agent/src/providers.rs crates/vmux_agent/src/lib.rs crates/vmux_agent/Cargo.toml
git commit -m "feat(vmux_agent): BUILTIN_PROVIDERS + resolve_default_app_provider"
```

---

## Task 14: AgentUrl::AppDefault parsing

**Files:**
- Modify: `crates/vmux_agent/src/kind.rs`

Changed crates: `vmux_agent`

- [ ] **Step 1: Add the variant + parser branch + test**

Edit `crates/vmux_agent/src/kind.rs`. Update the `AgentUrl` enum (currently with `Cli` and `App` variants) to add `AppDefault`:

```rust
pub enum AgentUrl {
    Cli { kind: AgentKind, sid: String },
    App { provider: String, model: String, sid: String },
    AppDefault,
}
```

Update `AgentUrl::parse` to handle the empty-segments case. The existing `match segs.as_slice()` block: add an `[] => Some(AgentUrl::AppDefault),` arm before the existing arms.

Update `format`, `variant`, and `sid` methods to handle the new variant:

```rust
    pub fn variant(&self) -> AgentVariant {
        match self {
            AgentUrl::Cli { .. } => AgentVariant::Cli,
            AgentUrl::App { .. } | AgentUrl::AppDefault => AgentVariant::App,
        }
    }

    pub fn sid(&self) -> &str {
        match self {
            AgentUrl::Cli { sid, .. } => sid,
            AgentUrl::App { sid, .. } => sid,
            AgentUrl::AppDefault => "",
        }
    }

    pub fn format(&self) -> String {
        match self {
            AgentUrl::Cli { kind, sid } => format!("{}{sid}", kind.cli_url_prefix()),
            AgentUrl::App { provider, model, sid } => format!("{}{sid}", app_url_prefix(provider, model)),
            AgentUrl::AppDefault => "vmux://agent/".to_string(),
        }
    }
```

Add tests inside the existing `#[cfg(test)] mod tests`:

```rust
    #[test]
    fn bare_agent_url_parses_to_app_default() {
        assert_eq!(AgentUrl::parse("vmux://agent/"), Some(AgentUrl::AppDefault));
    }

    #[test]
    fn app_default_formats_back_to_bare() {
        assert_eq!(AgentUrl::AppDefault.format(), "vmux://agent/");
    }

    #[test]
    fn app_default_round_trip() {
        let url = AgentUrl::AppDefault.format();
        assert_eq!(AgentUrl::parse(&url), Some(AgentUrl::AppDefault));
    }
```

If `AgentUrl` doesn't currently derive `PartialEq, Eq`, add those derives so the tests compile.

- [ ] **Step 2: Run tests**

```bash
env -u CEF_PATH cargo test -p vmux_agent kind
```
Expected: all kind tests pass including the 3 new ones.

- [ ] **Step 3: Commit**

```bash
git add crates/vmux_agent/src/kind.rs
git commit -m "feat(vmux_agent): AgentUrl::AppDefault for bare vmux://agent/"
```

---

## Task 15: Rewire `process_user_input` to use strategy + drive_sse

**Files:**
- Modify: `crates/vmux_agent/src/systems/process_input.rs`

Changed crates: `vmux_agent`

- [ ] **Step 1: Replace the system body**

Replace the entire contents of `crates/vmux_agent/src/systems/process_input.rs` with:

```rust
use bevy::prelude::*;
use bevy::tasks::IoTaskPool;
use crossbeam_channel::unbounded;

use crate::components::{AgentMessages, AgentSession, PendingUserInput};
use crate::http::drive_sse;
use crate::message::Message;
use crate::run_state::AgentRunState;
use crate::stream::StreamEvent;
use crate::strategy::AgentStrategies;
use crate::tools::mcp_tool_defs;

pub fn process_user_input(
    mut commands: Commands,
    strategies: Res<AgentStrategies>,
    mut q: Query<(
        Entity,
        &PendingUserInput,
        &mut AgentMessages,
        &mut AgentRunState,
        &AgentSession,
    )>,
) {
    for (entity, pending, mut messages, mut state, session) in &mut q {
        if !matches!(
            *state,
            AgentRunState::Idle | AgentRunState::Errored(_)
        ) {
            continue;
        }
        messages.0.push(Message::User {
            text: pending.0.clone(),
        });

        let Some(strategy) =
            strategies.get_app_by_provider_model(&session.provider, &session.model)
        else {
            *state = AgentRunState::Errored(format!(
                "No registered App strategy for {}/{}",
                session.provider, session.model
            ));
            commands.entity(entity).remove::<PendingUserInput>();
            continue;
        };

        let env_var = strategy.env_var();
        let api_key = if env_var.is_empty() {
            String::new()
        } else {
            match std::env::var(env_var) {
                Ok(k) => k,
                Err(_) => {
                    *state = AgentRunState::Errored(format!("Missing {env_var}"));
                    commands.entity(entity).remove::<PendingUserInput>();
                    continue;
                }
            }
        };

        let tools = mcp_tool_defs();
        let request = strategy.build_request(&session.model, &messages.0, &tools, &api_key);
        let (tx, rx) = unbounded::<StreamEvent>();
        let strat_arc = strategy.clone();
        let task = IoTaskPool::get().spawn(async move {
            drive_sse(request, strat_arc, tx).await;
        });

        *state = AgentRunState::Streaming {
            rx,
            _task: task,
            partial: None,
        };
        commands.entity(entity).remove::<PendingUserInput>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::strategy::AgentStrategy;
    use crate::stream::ToolDef;
    use crate::{AgentKind, AgentVariant};
    use std::sync::Arc;

    struct MockAppStrategy;
    impl AgentStrategy for MockAppStrategy {
        fn kind(&self) -> AgentKind {
            AgentKind::Vibe
        }
        fn variant(&self) -> AgentVariant {
            AgentVariant::App
        }
    }
    impl crate::app::AppAgentStrategy for MockAppStrategy {
        fn provider(&self) -> &str {
            "mock"
        }
        fn model(&self) -> &str {
            "m"
        }
        fn endpoint(&self) -> &str {
            "http://127.0.0.1:9/never"
        }
        fn env_var(&self) -> &'static str {
            ""
        }
        fn build_request(
            &self,
            _: &str,
            _: &[Message],
            _: &[ToolDef],
            _: &str,
        ) -> reqwest::Request {
            reqwest::Client::new()
                .get("http://127.0.0.1:9/never")
                .build()
                .unwrap()
        }
        fn parse_sse_event(&self, _: &str) -> Option<StreamEvent> {
            None
        }
    }

    fn make_app() -> App {
        let mut app = App::new();
        app.add_plugins(bevy::app::TaskPoolPlugin::default());
        let mut s = AgentStrategies::default();
        s.register_app(Arc::new(MockAppStrategy));
        app.insert_resource(s);
        app.add_systems(Update, process_user_input);
        app
    }

    #[test]
    fn transitions_idle_to_streaming_when_strategy_present() {
        let mut app = make_app();
        let entity = app
            .world_mut()
            .spawn((
                AgentSession {
                    kind: AgentKind::Vibe,
                    variant: AgentVariant::App,
                    sid: "t".into(),
                    provider: "mock".into(),
                    model: "m".into(),
                },
                AgentMessages::default(),
                AgentRunState::Idle,
                PendingUserInput("hi".into()),
            ))
            .id();
        app.update();
        let world = app.world();
        let state = world.get::<AgentRunState>(entity).unwrap();
        assert!(matches!(state, AgentRunState::Streaming { .. }));
        assert!(world.get::<PendingUserInput>(entity).is_none());
    }

    #[test]
    fn errors_when_no_strategy_registered() {
        let mut app = App::new();
        app.add_plugins(bevy::app::TaskPoolPlugin::default());
        app.insert_resource(AgentStrategies::default());
        app.add_systems(Update, process_user_input);
        let entity = app
            .world_mut()
            .spawn((
                AgentSession {
                    kind: AgentKind::Vibe,
                    variant: AgentVariant::App,
                    sid: "t".into(),
                    provider: "missing".into(),
                    model: "m".into(),
                },
                AgentMessages::default(),
                AgentRunState::Idle,
                PendingUserInput("hi".into()),
            ))
            .id();
        app.update();
        let state = app.world().get::<AgentRunState>(entity).unwrap();
        match state {
            AgentRunState::Errored(msg) => assert!(msg.contains("missing/m")),
            other => panic!("expected Errored, got {other:?}"),
        }
    }
}
```

- [ ] **Step 2: Run tests**

```bash
env -u CEF_PATH cargo test -p vmux_agent systems::process_input
```
Expected: 2 tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/vmux_agent/src/systems/process_input.rs
git commit -m "feat(vmux_agent): process_user_input drives real SSE via strategy + IoTaskPool"
```

---

## Task 16: Extend `drain_stream` for tool-use + Error transitions

**Files:**
- Modify: `crates/vmux_agent/src/systems/drain_stream.rs`

Changed crates: `vmux_agent`

- [ ] **Step 1: Replace the system body**

Replace the entire contents of `crates/vmux_agent/src/systems/drain_stream.rs` with:

```rust
use bevy::prelude::*;
use bevy::tasks::IoTaskPool;

use crate::components::{AgentApprovalPolicy, AgentMessages, AgentSession};
use crate::events::{AgentApprovalRequest, AgentDelta, AgentToolStatus, ToolStatus};
use crate::message::{AssistantBlock, Message};
use crate::run_state::{AgentRunState, ToolDispatchOutput};
use crate::stream::{PartialToolUse, StopReason, StreamEvent};

pub fn drain_stream(
    mut commands: Commands,
    mut q: Query<(
        Entity,
        &mut AgentRunState,
        &mut AgentMessages,
        &AgentApprovalPolicy,
        &AgentSession,
    )>,
) {
    for (entity, mut state, mut messages, policy, _session) in &mut q {
        let mut drained: Vec<StreamEvent> = Vec::new();
        match &*state {
            AgentRunState::Streaming { rx, .. } => drained.extend(rx.try_iter()),
            _ => continue,
        }
        if drained.is_empty() {
            continue;
        }

        ensure_assistant_tail(&mut messages);

        let mut next_state: Option<AgentRunState> = None;
        for event in drained {
            match event {
                StreamEvent::TextDelta(text) => {
                    append_text_delta(&mut messages, &text);
                    commands.trigger(AgentDelta {
                        session: entity,
                        text,
                    });
                }
                StreamEvent::ToolUseStart { call_id, name } => {
                    if let AgentRunState::Streaming { partial, .. } = &mut *state {
                        *partial = Some(PartialToolUse {
                            call_id,
                            name,
                            args_buf: String::new(),
                        });
                    }
                }
                StreamEvent::ToolUseArgsDelta { call_id, json_chunk } => {
                    if let AgentRunState::Streaming { partial, .. } = &mut *state {
                        if let Some(p) = partial {
                            if !call_id.is_empty() && p.call_id.is_empty() {
                                p.call_id = call_id;
                            }
                            p.args_buf.push_str(&json_chunk);
                        }
                    }
                }
                StreamEvent::ToolUseEnd { call_id: streamed_id } => {
                    let p = match &mut *state {
                        AgentRunState::Streaming { partial, .. } => partial.take(),
                        _ => None,
                    };
                    if let Some(mut p) = p {
                        if p.call_id.is_empty() && !streamed_id.is_empty() {
                            p.call_id = streamed_id;
                        }
                        push_tool_use_block(&mut messages, &p);
                        let args_value: serde_json::Value = serde_json::from_str(&p.args_buf)
                            .unwrap_or_else(|_| serde_json::Value::String(p.args_buf.clone()));
                        commands.trigger(AgentToolStatus {
                            session: entity,
                            call_id: p.call_id.clone(),
                            status: ToolStatus::Pending,
                        });
                        if policy.auto.contains(&p.name) {
                            next_state = Some(spawn_running_tool(&p, args_value));
                        } else {
                            commands.trigger(AgentApprovalRequest {
                                session: entity,
                                call_id: p.call_id.clone(),
                                name: p.name.clone(),
                                args: args_value.clone(),
                            });
                            next_state = Some(AgentRunState::AwaitingApproval {
                                call_id: p.call_id,
                                name: p.name,
                                args: args_value,
                            });
                        }
                    }
                }
                StreamEvent::StopTurn {
                    reason: StopReason::EndTurn,
                } => {
                    if next_state.is_none() {
                        next_state = Some(AgentRunState::Idle);
                    }
                }
                StreamEvent::StopTurn {
                    reason: StopReason::ToolUse,
                } => {
                    // do nothing — tool dispatch path already chose the next state
                }
                StreamEvent::StopTurn {
                    reason: StopReason::MaxTokens | StopReason::Other,
                } => {
                    next_state = Some(AgentRunState::Idle);
                }
                StreamEvent::Error(msg) => {
                    next_state = Some(AgentRunState::Errored(msg));
                }
            }
        }

        if let Some(new_state) = next_state {
            *state = new_state;
        }
    }
}

fn ensure_assistant_tail(messages: &mut AgentMessages) {
    if !matches!(messages.0.last(), Some(Message::Assistant { .. })) {
        messages.0.push(Message::Assistant { blocks: Vec::new() });
    }
}

fn append_text_delta(messages: &mut AgentMessages, text: &str) {
    let Some(Message::Assistant { blocks }) = messages.0.last_mut() else {
        return;
    };
    if let Some(AssistantBlock::Text(buf)) = blocks.last_mut() {
        buf.push_str(text);
    } else {
        blocks.push(AssistantBlock::Text(text.to_string()));
    }
}

fn push_tool_use_block(messages: &mut AgentMessages, p: &PartialToolUse) {
    let Some(Message::Assistant { blocks }) = messages.0.last_mut() else {
        return;
    };
    blocks.push(AssistantBlock::ToolUse {
        call_id: p.call_id.clone(),
        name: p.name.clone(),
        args: p.args_buf.clone(),
    });
}

fn spawn_running_tool(p: &PartialToolUse, _args: serde_json::Value) -> AgentRunState {
    let call_id = p.call_id.clone();
    let call_id_for_task = call_id.clone();
    let task = IoTaskPool::get().spawn(async move {
        ToolDispatchOutput {
            call_id: call_id_for_task,
            content: "tool dispatch not yet wired".to_string(),
            is_error: true,
        }
    });
    AgentRunState::RunningTool { call_id, task }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stream::PartialToolUse;
    use crate::{AgentKind, AgentVariant};
    use bevy::tasks::IoTaskPool;
    use crossbeam_channel::unbounded;

    fn make_app() -> App {
        let mut app = App::new();
        app.add_plugins(bevy::app::TaskPoolPlugin::default());
        app.add_systems(Update, drain_stream);
        app
    }

    fn make_session() -> AgentSession {
        AgentSession {
            kind: AgentKind::Vibe,
            variant: AgentVariant::App,
            sid: "t".into(),
            provider: "mock".into(),
            model: "m".into(),
        }
    }

    #[test]
    fn text_delta_then_end_turn_goes_idle() {
        let mut app = make_app();
        let (tx, rx) = unbounded::<StreamEvent>();
        tx.send(StreamEvent::TextDelta("hi".into())).unwrap();
        tx.send(StreamEvent::StopTurn {
            reason: StopReason::EndTurn,
        })
        .unwrap();
        drop(tx);
        let task = IoTaskPool::get().spawn(async {});
        let entity = app
            .world_mut()
            .spawn((
                make_session(),
                AgentMessages::default(),
                AgentApprovalPolicy::default(),
                AgentRunState::Streaming {
                    rx,
                    _task: task,
                    partial: None,
                },
            ))
            .id();
        app.update();
        assert!(matches!(
            app.world().get::<AgentRunState>(entity),
            Some(AgentRunState::Idle)
        ));
    }

    #[test]
    fn tool_use_without_policy_transitions_to_awaiting_approval() {
        let mut app = make_app();
        let (tx, rx) = unbounded::<StreamEvent>();
        tx.send(StreamEvent::ToolUseStart {
            call_id: "c1".into(),
            name: "list_spaces".into(),
        })
        .unwrap();
        tx.send(StreamEvent::ToolUseArgsDelta {
            call_id: String::new(),
            json_chunk: "{\"x\":1}".into(),
        })
        .unwrap();
        tx.send(StreamEvent::ToolUseEnd {
            call_id: String::new(),
        })
        .unwrap();
        tx.send(StreamEvent::StopTurn {
            reason: StopReason::ToolUse,
        })
        .unwrap();
        drop(tx);
        let task = IoTaskPool::get().spawn(async {});
        let entity = app
            .world_mut()
            .spawn((
                make_session(),
                AgentMessages::default(),
                AgentApprovalPolicy::default(),
                AgentRunState::Streaming {
                    rx,
                    _task: task,
                    partial: Some(PartialToolUse::default()),
                },
            ))
            .id();
        app.update();
        match app.world().get::<AgentRunState>(entity).unwrap() {
            AgentRunState::AwaitingApproval { call_id, name, .. } => {
                assert_eq!(call_id, "c1");
                assert_eq!(name, "list_spaces");
            }
            other => panic!("expected AwaitingApproval, got {other:?}"),
        }
    }

    #[test]
    fn error_event_transitions_to_errored() {
        let mut app = make_app();
        let (tx, rx) = unbounded::<StreamEvent>();
        tx.send(StreamEvent::Error("HTTP 500: boom".into())).unwrap();
        drop(tx);
        let task = IoTaskPool::get().spawn(async {});
        let entity = app
            .world_mut()
            .spawn((
                make_session(),
                AgentMessages::default(),
                AgentApprovalPolicy::default(),
                AgentRunState::Streaming {
                    rx,
                    _task: task,
                    partial: None,
                },
            ))
            .id();
        app.update();
        match app.world().get::<AgentRunState>(entity).unwrap() {
            AgentRunState::Errored(msg) => assert!(msg.contains("HTTP 500")),
            other => panic!("expected Errored, got {other:?}"),
        }
    }
}
```

Note on the auto-approve path: `spawn_running_tool` returns a stub task that fails ("tool dispatch not yet wired"). Real MCP dispatch is added in Task 17. The stub is sufficient for state-transition tests now.

- [ ] **Step 2: Run tests**

```bash
env -u CEF_PATH cargo test -p vmux_agent systems::drain_stream
```
Expected: 3 tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/vmux_agent/src/systems/drain_stream.rs
git commit -m "feat(vmux_agent): drain_stream handles tool-use + Error transitions"
```

---

## Task 17: In-process MCP tool dispatch bridge

**Files:**
- Create: `crates/vmux_agent/src/tool_dispatch.rs`
- Modify: `crates/vmux_agent/src/lib.rs`
- Modify: `crates/vmux_agent/src/systems/drain_stream.rs` (replace stub `spawn_running_tool`)
- Modify: `crates/vmux_agent/src/systems/approval.rs` (Allow path spawns the same task)
- Modify: `crates/vmux_desktop/src/agent.rs` (`handle_agent_commands` writes results back via a Bevy `Messages<AgentCommandResultMessage>` channel for in-process callers)

Changed crates: `vmux_agent`, `vmux_desktop`

**Architectural context (discovered):** `vmux_mcp::tools::dispatch_from_tool_call(name, args)` returns a `DispatchTarget::Command(AgentCommand)` or `DispatchTarget::Query(AgentQuery)`. The desktop process executes these via its existing `handle_agent_commands` system, which reads `AgentCommandRequest` messages from the Bevy message bus and writes results back over a `vmux_service::ServiceClient` to whichever out-of-process CLI agent issued them.

The new App agent runs **in-process** in the desktop app, so it can write `AgentCommandRequest` to the same bus, but needs an in-process callback path for results (no service client). Two-step bridge:

1. `spawn_tool_task` writes an `AgentCommandRequest` (or `AgentQueryRequest`) with a fresh `AgentRequestId`, then awaits the matching `AgentCommandResultMessage` from a new in-process channel.
2. `handle_agent_commands` is extended to additionally publish results on a new `AgentCommandResultMessage` Bevy message stream so in-process callers can pick them up. The existing out-of-process service-client write is unchanged.

For v1 we wire this for `AppCommand` and `Query` tool kinds (the simplest dispatch paths). Other `ServiceAgentCommand` variants (`NewTerminalTab`, `RunShell`, etc.) return an "unsupported in App agent v1" error and the agent loop continues with an error tool result.

- [ ] **Step 1: Add the in-process result message**

In `crates/vmux_desktop/src/agent.rs`, near the existing `AgentCommandRequest` definition (~line 28), add:

```rust
#[derive(Message, Clone, Debug)]
pub(crate) struct AgentCommandResultMessage {
    pub(crate) request_id: AgentRequestId,
    pub(crate) result: vmux_service::protocol::AgentCommandResult,
}

#[derive(Message, Clone, Debug)]
pub(crate) struct AgentQueryResultMessage {
    pub(crate) request_id: AgentRequestId,
    pub(crate) result: vmux_service::protocol::AgentQueryResult,
}
```

Register them in the agent plugin's `build` alongside the existing `add_message::<AgentCommandRequest>()` calls:

```rust
            .add_message::<AgentCommandResultMessage>()
            .add_message::<AgentQueryResultMessage>()
```

- [ ] **Step 2: Have `handle_agent_commands` publish results in-process**

In `crates/vmux_desktop/src/agent.rs` `handle_agent_commands` (around line 849), after each branch computes its `AgentCommandResult` (currently stored in `result`), and before / alongside the existing `service.send_to_client(...)` write, also publish to the new message stream. Add a `MessageWriter<AgentCommandResultMessage>` parameter to the system signature and `writer.write(AgentCommandResultMessage { request_id: request.request_id.clone(), result: result.clone() })` after each result is produced. Do the same for `handle_agent_queries` (find it by `grep -n 'fn handle_agent_queries' crates/vmux_desktop/src/agent.rs`).

- [ ] **Step 3: Write the dispatcher helper**

Create `crates/vmux_agent/src/tool_dispatch.rs`:

```rust
use bevy::tasks::{IoTaskPool, Task};
use crossbeam_channel::{Receiver, Sender, unbounded};
use std::collections::HashMap;
use std::sync::Mutex;

use crate::run_state::ToolDispatchOutput;

/// Type-erased result a dispatch waiter receives.
#[derive(Clone, Debug)]
pub struct DispatchResult {
    pub content: String,
    pub is_error: bool,
}

/// In-process registry mapping a request id (String) to a one-shot sender.
/// The dispatcher writes a `Sender` here before emitting the Bevy message;
/// a Bevy system in vmux_desktop reads `AgentCommandResultMessage` and pops
/// the matching sender to deliver the result.
static PENDING: once_cell::sync::Lazy<Mutex<HashMap<String, Sender<DispatchResult>>>> =
    once_cell::sync::Lazy::new(|| Mutex::new(HashMap::new()));

pub fn register_pending(request_id: String) -> Receiver<DispatchResult> {
    let (tx, rx) = unbounded::<DispatchResult>();
    PENDING.lock().unwrap().insert(request_id, tx);
    rx
}

pub fn deliver(request_id: &str, result: DispatchResult) {
    if let Some(tx) = PENDING.lock().unwrap().remove(request_id) {
        let _ = tx.send(result);
    }
}

pub fn spawn_tool_task(
    call_id: String,
    name: String,
    args: serde_json::Value,
) -> Task<ToolDispatchOutput> {
    IoTaskPool::get().spawn(async move {
        // Step 1: classify tool via vmux_mcp.
        let target = match vmux_mcp::tools::dispatch_from_tool_call(&name, args) {
            Ok(t) => t,
            Err(e) => {
                return ToolDispatchOutput {
                    call_id,
                    content: e,
                    is_error: true,
                };
            }
        };

        // Step 2: mint a request id and register a waiter.
        let request_id = format!("app-agent-{}", uuid::Uuid::new_v4());
        let rx = register_pending(request_id.clone());

        // Step 3: publish AgentCommandRequest / AgentQueryRequest via the
        // global Bevy world. We don't have direct world access from a spawn
        // task, so use a static channel (see below: `EMIT_CHANNEL`).
        let emit = EmitDispatch {
            request_id: request_id.clone(),
            target,
        };
        let _ = EMIT_CHANNEL.0.send(emit);

        // Step 4: await the result (with a 60s timeout).
        let result = match rx.recv_timeout(std::time::Duration::from_secs(60)) {
            Ok(r) => r,
            Err(_) => DispatchResult {
                content: "tool dispatch timed out (60s)".to_string(),
                is_error: true,
            },
        };

        ToolDispatchOutput {
            call_id,
            content: result.content,
            is_error: result.is_error,
        }
    })
}

#[derive(Clone, Debug)]
pub struct EmitDispatch {
    pub request_id: String,
    pub target: vmux_mcp::tools::DispatchTarget,
}

pub static EMIT_CHANNEL: once_cell::sync::Lazy<(Sender<EmitDispatch>, Receiver<EmitDispatch>)> =
    once_cell::sync::Lazy::new(unbounded);
```

This file requires adding `once_cell = "1"` and `uuid = { workspace = true }` to `crates/vmux_agent/Cargo.toml` `[dependencies]`. `uuid` was added in Task 13; add `once_cell` here.

- [ ] **Step 4: Add the desktop-side bridge system**

In `crates/vmux_desktop/src/agent.rs`, register a new system that:
1. Drains `vmux_agent::tool_dispatch::EMIT_CHANNEL.1.try_iter()` each frame.
2. For each `EmitDispatch { request_id, target }`, writes a matching `AgentCommandRequest` or `AgentQueryRequest`.
3. In a parallel system, reads `AgentCommandResultMessage` / `AgentQueryResultMessage` and calls `vmux_agent::tool_dispatch::deliver(&request_id, DispatchResult { ... })`.

```rust
fn drain_app_agent_dispatches(
    mut cmd_writer: MessageWriter<AgentCommandRequest>,
    mut query_writer: MessageWriter<AgentQueryRequest>,
) {
    use vmux_agent::tool_dispatch::EMIT_CHANNEL;
    use vmux_mcp::tools::DispatchTarget;
    for emit in EMIT_CHANNEL.1.try_iter() {
        let request_id = AgentRequestId::from(emit.request_id.clone());
        match emit.target {
            DispatchTarget::Command(command) => {
                cmd_writer.write(AgentCommandRequest { request_id, command });
            }
            DispatchTarget::Query(query) => {
                query_writer.write(AgentQueryRequest { request_id, query });
            }
        }
    }
}

fn relay_command_results_to_app_agent(
    mut reader: MessageReader<AgentCommandResultMessage>,
) {
    use vmux_agent::tool_dispatch::{DispatchResult, deliver};
    use vmux_service::protocol::AgentCommandResult;
    for msg in reader.read() {
        let (content, is_error) = match &msg.result {
            AgentCommandResult::Ok => ("ok".to_string(), false),
            AgentCommandResult::Error(e) => (e.clone(), true),
        };
        deliver(msg.request_id.as_str(), DispatchResult { content, is_error });
    }
}

fn relay_query_results_to_app_agent(
    mut reader: MessageReader<AgentQueryResultMessage>,
) {
    use vmux_agent::tool_dispatch::{DispatchResult, deliver};
    for msg in reader.read() {
        let (content, is_error) = match serde_json::to_string(&msg.result) {
            Ok(s) => (s, false),
            Err(e) => (format!("serde error: {e}"), true),
        };
        deliver(msg.request_id.as_str(), DispatchResult { content, is_error });
    }
}
```

Register all three in the `Plugin::build` of agent.rs (find it by `grep -n 'impl Plugin for' crates/vmux_desktop/src/agent.rs`), in `add_systems(Update, ...)`. Adapt `AgentRequestId::from` / `.as_str()` to whatever the actual constructor/accessor is — check `vmux_service::protocol::AgentRequestId` definition with `grep -n 'pub struct AgentRequestId\|impl AgentRequestId' crates/vmux_service/src/protocol.rs`.

- [ ] **Step 5: Replace the stub in drain_stream**

In `crates/vmux_agent/src/systems/drain_stream.rs`, replace `spawn_running_tool`:

```rust
fn spawn_running_tool(p: &PartialToolUse, args: serde_json::Value) -> AgentRunState {
    let call_id = p.call_id.clone();
    let task = crate::tool_dispatch::spawn_tool_task(call_id.clone(), p.name.clone(), args);
    AgentRunState::RunningTool { call_id, task }
}
```

Remove the now-unused `IoTaskPool` import if clippy warns.

- [ ] **Step 6: Rewrite the Allow path in approval.rs**

Replace `crates/vmux_agent/src/systems/approval.rs::handle_approval_reply` entirely:

```rust
use bevy::prelude::*;

use crate::components::{AgentApprovalPolicy, AgentMessages};
use crate::events::{AgentApprovalReply, ApprovalDecision};
use crate::message::Message;
use crate::run_state::AgentRunState;

pub fn handle_approval_reply(
    trigger: On<AgentApprovalReply>,
    mut q: Query<(
        &mut AgentRunState,
        &mut AgentMessages,
        &mut AgentApprovalPolicy,
    )>,
) {
    let reply = trigger.event();
    let Ok((mut state, mut messages, mut policy)) = q.get_mut(reply.session) else {
        return;
    };
    let matches_call = matches!(
        &*state,
        AgentRunState::AwaitingApproval { call_id, .. } if call_id == &reply.call_id
    );
    if !matches_call {
        return;
    }
    match reply.decision {
        ApprovalDecision::Allow | ApprovalDecision::AllowAlways => {
            let AgentRunState::AwaitingApproval { call_id, name, args } =
                std::mem::replace(&mut *state, AgentRunState::Idle)
            else {
                return;
            };
            if reply.decision == ApprovalDecision::AllowAlways {
                policy.auto.insert(name.clone());
            }
            let task = crate::tool_dispatch::spawn_tool_task(call_id.clone(), name, args);
            *state = AgentRunState::RunningTool { call_id, task };
        }
        ApprovalDecision::Deny => {
            messages.0.push(Message::ToolResult {
                call_id: reply.call_id.clone(),
                content: "denied by user".into(),
                is_error: true,
            });
            *state = AgentRunState::Idle;
        }
    }
}
```

The two existing tests in `approval.rs` (`deny_appends_error_result_and_idles`, `allow_always_records_in_policy`) still pass because they don't assert on `RunningTool` state — only on `Idle` and policy mutation. The `Allow` test now transitions to `RunningTool` instead of `Idle`; check the existing test asserts. If it asserts `AgentRunState::Idle` after Allow, change the expectation to `matches!(_, AgentRunState::RunningTool { .. })` and update the test description.

- [ ] **Step 7: Wire into lib.rs**

In `crates/vmux_agent/src/lib.rs`:

```rust
pub mod tool_dispatch;
```

- [ ] **Step 8: Add Cargo deps**

In `crates/vmux_agent/Cargo.toml` `[dependencies]`:

```toml
once_cell = "1"
vmux_mcp = { path = "../vmux_mcp" }   # already added in Task 5
uuid = { workspace = true }            # if not already present
```

- [ ] **Step 9: Build + test**

```bash
env -u CEF_PATH cargo build -p vmux_agent
env -u CEF_PATH cargo build -p vmux_desktop
env -u CEF_PATH cargo test -p vmux_agent
env -u CEF_PATH cargo test -p vmux_desktop
```
Expected: all succeed. Pre-existing tests stay green; `approval` Allow test now expects `RunningTool`.

- [ ] **Step 10: Commit**

```bash
git add crates/vmux_agent/src/tool_dispatch.rs crates/vmux_agent/src/lib.rs crates/vmux_agent/src/systems/drain_stream.rs crates/vmux_agent/src/systems/approval.rs crates/vmux_agent/Cargo.toml crates/vmux_desktop/src/agent.rs
git commit -m "feat(vmux_agent): in-process MCP tool dispatch via static channel bridge"
```

---

## Task 18: `continue_after_tool` system

**Files:**
- Create: `crates/vmux_agent/src/systems/continue_after_tool.rs`
- Modify: `crates/vmux_agent/src/lib.rs` (add to `systems` module)
- Modify: `crates/vmux_agent/src/app_plugin.rs` (register system)

Changed crates: `vmux_agent`

- [ ] **Step 1: Write the system + tests**

Create `crates/vmux_agent/src/systems/continue_after_tool.rs`:

```rust
use bevy::prelude::*;
use bevy::tasks::IoTaskPool;
use crossbeam_channel::unbounded;

use crate::components::{AgentMessages, AgentSession};
use crate::http::drive_sse;
use crate::message::Message;
use crate::run_state::AgentRunState;
use crate::stream::StreamEvent;
use crate::strategy::AgentStrategies;
use crate::tools::mcp_tool_defs;

pub fn continue_after_tool(
    strategies: Res<AgentStrategies>,
    mut q: Query<(&mut AgentRunState, &AgentMessages, &AgentSession)>,
) {
    for (mut state, messages, session) in &mut q {
        if !matches!(*state, AgentRunState::Idle) {
            continue;
        }
        if !matches!(messages.0.last(), Some(Message::ToolResult { .. })) {
            continue;
        }
        let Some(strategy) =
            strategies.get_app_by_provider_model(&session.provider, &session.model)
        else {
            *state = AgentRunState::Errored(format!(
                "No registered App strategy for {}/{}",
                session.provider, session.model
            ));
            continue;
        };
        let env_var = strategy.env_var();
        let api_key = if env_var.is_empty() {
            String::new()
        } else {
            match std::env::var(env_var) {
                Ok(k) => k,
                Err(_) => {
                    *state = AgentRunState::Errored(format!("Missing {env_var}"));
                    continue;
                }
            }
        };
        let tools = mcp_tool_defs();
        let request = strategy.build_request(&session.model, &messages.0, &tools, &api_key);
        let (tx, rx) = unbounded::<StreamEvent>();
        let strat_arc = strategy.clone();
        let task = IoTaskPool::get().spawn(async move {
            drive_sse(request, strat_arc, tx).await;
        });
        *state = AgentRunState::Streaming {
            rx,
            _task: task,
            partial: None,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::AppAgentStrategy;
    use crate::strategy::AgentStrategy;
    use crate::stream::ToolDef;
    use crate::{AgentKind, AgentVariant};
    use std::sync::Arc;

    struct MockAppStrategy;
    impl AgentStrategy for MockAppStrategy {
        fn kind(&self) -> AgentKind {
            AgentKind::Vibe
        }
        fn variant(&self) -> AgentVariant {
            AgentVariant::App
        }
    }
    impl AppAgentStrategy for MockAppStrategy {
        fn provider(&self) -> &str {
            "mock"
        }
        fn model(&self) -> &str {
            "m"
        }
        fn endpoint(&self) -> &str {
            "http://127.0.0.1:9/never"
        }
        fn env_var(&self) -> &'static str {
            ""
        }
        fn build_request(
            &self,
            _: &str,
            _: &[Message],
            _: &[ToolDef],
            _: &str,
        ) -> reqwest::Request {
            reqwest::Client::new()
                .get("http://127.0.0.1:9/never")
                .build()
                .unwrap()
        }
        fn parse_sse_event(&self, _: &str) -> Option<StreamEvent> {
            None
        }
    }

    #[test]
    fn idle_with_tool_result_tail_transitions_to_streaming() {
        let mut app = App::new();
        app.add_plugins(bevy::app::TaskPoolPlugin::default());
        let mut s = AgentStrategies::default();
        s.register_app(Arc::new(MockAppStrategy));
        app.insert_resource(s);
        app.add_systems(Update, continue_after_tool);
        let entity = app
            .world_mut()
            .spawn((
                AgentSession {
                    kind: AgentKind::Vibe,
                    variant: AgentVariant::App,
                    sid: "t".into(),
                    provider: "mock".into(),
                    model: "m".into(),
                },
                AgentMessages(vec![Message::ToolResult {
                    call_id: "c1".into(),
                    content: "ok".into(),
                    is_error: false,
                }]),
                AgentRunState::Idle,
            ))
            .id();
        app.update();
        assert!(matches!(
            app.world().get::<AgentRunState>(entity),
            Some(AgentRunState::Streaming { .. })
        ));
    }

    #[test]
    fn idle_without_tool_result_tail_stays_idle() {
        let mut app = App::new();
        app.add_plugins(bevy::app::TaskPoolPlugin::default());
        let mut s = AgentStrategies::default();
        s.register_app(Arc::new(MockAppStrategy));
        app.insert_resource(s);
        app.add_systems(Update, continue_after_tool);
        let entity = app
            .world_mut()
            .spawn((
                AgentSession {
                    kind: AgentKind::Vibe,
                    variant: AgentVariant::App,
                    sid: "t".into(),
                    provider: "mock".into(),
                    model: "m".into(),
                },
                AgentMessages(vec![Message::User { text: "hi".into() }]),
                AgentRunState::Idle,
            ))
            .id();
        app.update();
        assert!(matches!(
            app.world().get::<AgentRunState>(entity),
            Some(AgentRunState::Idle)
        ));
    }
}
```

- [ ] **Step 2: Wire into lib.rs**

In `crates/vmux_agent/src/lib.rs`, find:

```rust
pub mod systems {
    pub mod approval;
    pub mod dispatch_tool;
    pub mod drain_stream;
    pub mod process_input;
}
```

Add `continue_after_tool` (alphabetical):

```rust
pub mod systems {
    pub mod approval;
    pub mod continue_after_tool;
    pub mod dispatch_tool;
    pub mod drain_stream;
    pub mod process_input;
}
```

- [ ] **Step 3: Register in app_plugin.rs**

In `crates/vmux_agent/src/app_plugin.rs`, add `continue_after_tool` to the imports and the `add_systems`:

```rust
use crate::systems::{approval, continue_after_tool, dispatch_tool, drain_stream, process_input};
```

```rust
            .add_systems(
                Update,
                (
                    process_input::process_user_input,
                    drain_stream::drain_stream,
                    dispatch_tool::dispatch_tool,
                    continue_after_tool::continue_after_tool,
                ),
            );
```

- [ ] **Step 4: Run tests**

```bash
env -u CEF_PATH cargo test -p vmux_agent systems::continue_after_tool
```
Expected: 2 tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_agent/src/systems/continue_after_tool.rs crates/vmux_agent/src/lib.rs crates/vmux_agent/src/app_plugin.rs
git commit -m "feat(vmux_agent): continue_after_tool re-streams when last msg is ToolResult"
```

---

## Task 19: `surface_errors` system + toast event registration

**Files:**
- Create: `crates/vmux_agent/src/systems/surface_errors.rs`
- Modify: `crates/vmux_agent/src/lib.rs`
- Modify: `crates/vmux_agent/src/app_plugin.rs`

Changed crates: `vmux_agent`

- [ ] **Step 1: Write the system + tests**

Create `crates/vmux_agent/src/systems/surface_errors.rs`:

```rust
use bevy::prelude::*;

use crate::components::{AgentMessages, AgentSession};
use crate::message::{AssistantBlock, Message};
use crate::run_state::AgentRunState;
use crate::run_state_kind::{AgentRunStateKind, LastRunStateKind};
use crate::toast::{AgentToast, ToastLevel};

pub fn surface_errors(
    mut writer: MessageWriter<AgentToast>,
    mut q: Query<(
        &AgentRunState,
        &mut LastRunStateKind,
        &mut AgentMessages,
        &AgentSession,
    )>,
) {
    for (state, mut last, mut messages, session) in &mut q {
        let cur = AgentRunStateKind::from(state);
        if last.0 == cur {
            continue;
        }
        last.0 = cur;
        if cur != AgentRunStateKind::Errored {
            continue;
        }
        let AgentRunState::Errored(msg) = state else {
            continue;
        };
        messages.0.push(Message::Assistant {
            blocks: vec![AssistantBlock::Text(format!("⚠ {msg}"))],
        });
        writer.write(AgentToast {
            session_sid: session.sid.clone(),
            level: ToastLevel::Error,
            message: msg.clone(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AgentKind, AgentVariant};

    fn make_app() -> App {
        let mut app = App::new();
        app.add_plugins(bevy::app::TaskPoolPlugin::default());
        app.add_message::<AgentToast>();
        app.add_systems(Update, surface_errors);
        app
    }

    fn make_session() -> AgentSession {
        AgentSession {
            kind: AgentKind::Vibe,
            variant: AgentVariant::App,
            sid: "abc".into(),
            provider: "mock".into(),
            model: "m".into(),
        }
    }

    #[test]
    fn errored_transition_appends_inline_and_fires_toast() {
        let mut app = make_app();
        let entity = app
            .world_mut()
            .spawn((
                make_session(),
                AgentMessages::default(),
                LastRunStateKind::default(),
                AgentRunState::Errored("boom".into()),
            ))
            .id();
        app.update();
        let msgs = app.world().get::<AgentMessages>(entity).unwrap();
        assert_eq!(msgs.0.len(), 1);
        match &msgs.0[0] {
            Message::Assistant { blocks } => match &blocks[0] {
                AssistantBlock::Text(t) => assert!(t.contains("boom")),
                _ => panic!("expected text block"),
            },
            _ => panic!("expected assistant message"),
        }
        let events: Vec<AgentToast> = app
            .world_mut()
            .resource_mut::<bevy::ecs::message::Messages<AgentToast>>()
            .drain()
            .collect();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].session_sid, "abc");
        assert_eq!(events[0].level, ToastLevel::Error);
        assert!(events[0].message.contains("boom"));
    }

    #[test]
    fn no_op_when_state_kind_unchanged() {
        let mut app = make_app();
        let entity = app
            .world_mut()
            .spawn((
                make_session(),
                AgentMessages::default(),
                LastRunStateKind(AgentRunStateKind::Errored),
                AgentRunState::Errored("old".into()),
            ))
            .id();
        app.update();
        let msgs = app.world().get::<AgentMessages>(entity).unwrap();
        assert!(msgs.0.is_empty());
        let events: Vec<AgentToast> = app
            .world_mut()
            .resource_mut::<bevy::ecs::message::Messages<AgentToast>>()
            .drain()
            .collect();
        assert!(events.is_empty());
    }
}
```

- [ ] **Step 2: Wire into lib.rs**

In the `systems` module block:

```rust
pub mod systems {
    pub mod approval;
    pub mod continue_after_tool;
    pub mod dispatch_tool;
    pub mod drain_stream;
    pub mod process_input;
    pub mod surface_errors;
}
```

- [ ] **Step 3: Register event + system + auto-attach LastRunStateKind**

In `crates/vmux_agent/src/app_plugin.rs`:

```rust
use bevy::prelude::*;

use crate::components::{AgentApprovalPolicy, AgentMessages, AgentSession};
use crate::run_state_kind::LastRunStateKind;
use crate::strategy::AgentStrategies;
use crate::systems::{approval, continue_after_tool, dispatch_tool, drain_stream, process_input, surface_errors};
use crate::toast::AgentToast;

pub struct AppAgentPlugin;

impl Plugin for AppAgentPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<AgentSession>()
            .register_type::<AgentMessages>()
            .register_type::<AgentApprovalPolicy>()
            .add_message::<AgentToast>()
            .add_observer(approval::handle_approval_reply)
            .add_systems(
                Update,
                (
                    process_input::process_user_input,
                    drain_stream::drain_stream,
                    dispatch_tool::dispatch_tool,
                    continue_after_tool::continue_after_tool,
                    surface_errors::surface_errors,
                    attach_last_run_state_kind,
                ),
            );

        if app.world().get_resource::<AgentStrategies>().is_none() {
            app.insert_resource(AgentStrategies::default());
        }
    }
}

fn attach_last_run_state_kind(
    mut commands: Commands,
    q: Query<Entity, (With<AgentSession>, Without<LastRunStateKind>)>,
) {
    for entity in &q {
        commands.entity(entity).insert(LastRunStateKind::default());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_builds_without_panic() {
        let mut app = App::new();
        app.add_plugins(bevy::app::TaskPoolPlugin::default());
        app.add_plugins(AppAgentPlugin);
        app.update();
    }
}
```

- [ ] **Step 4: Run tests**

```bash
env -u CEF_PATH cargo test -p vmux_agent systems::surface_errors
env -u CEF_PATH cargo test -p vmux_agent app_plugin
```
Expected: 2 + 1 tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_agent/src/systems/surface_errors.rs crates/vmux_agent/src/lib.rs crates/vmux_agent/src/app_plugin.rs
git commit -m "feat(vmux_agent): surface_errors emits inline message + AgentToast on Errored"
```

---

## Task 20: Replace `register_app_agents_from_settings` with built-ins + overrides

**Files:**
- Modify: `crates/vmux_desktop/src/settings.rs`

Changed crates: `vmux_desktop`, `vmux_agent` (consumed)

- [ ] **Step 1: Replace the registration body**

In `crates/vmux_desktop/src/settings.rs`, replace `register_app_agents_from_settings` (lines 43–72) and the helper `default_agent_settings` to:

```rust
fn register_app_agents_from_settings(
    settings: Option<Res<AppSettings>>,
    strategies: Option<ResMut<vmux_agent::strategy::AgentStrategies>>,
) {
    let Some(mut strategies) = strategies else {
        return;
    };

    // 1. Always register built-ins (default model per provider).
    for builtin in vmux_agent::BUILTIN_PROVIDERS {
        let strat = vmux_agent::instantiate_builtin(builtin, builtin.default_model);
        strategies.register_app(strat);
    }

    // 2. Apply user overrides on top.
    let Some(settings) = settings else { return };
    for provider_settings in &settings.agent.app_providers {
        for model in &provider_settings.models {
            let strat: std::sync::Arc<dyn vmux_agent::AppAgentStrategy> =
                match provider_settings.provider.as_str() {
                    "mistral" => std::sync::Arc::new(vmux_agent::MistralStrategy::new(
                        provider_settings.provider.clone(),
                        model.clone(),
                    )),
                    "anthropic" => std::sync::Arc::new(vmux_agent::AnthropicStrategy::new(
                        provider_settings.provider.clone(),
                        model.clone(),
                    )),
                    "openai" => std::sync::Arc::new(vmux_agent::OpenAiResponsesStrategy::new(
                        provider_settings.provider.clone(),
                        model.clone(),
                    )),
                    "stub" => std::sync::Arc::new(vmux_agent::EchoAppStrategy::new(
                        provider_settings.provider.clone(),
                        model.clone(),
                        vmux_agent::AgentKind::Vibe,
                    )),
                    other => {
                        bevy::log::warn!(
                            "agent.app_providers: unknown provider '{other}' (model '{model}')"
                        );
                        continue;
                    }
                };
            strategies.register_app(strat);
        }
    }
}
```

Replace `default_agent_settings` body to:

```rust
fn default_agent_settings() -> AgentSettings {
    AgentSettings { app_providers: vec![] }
}
```

- [ ] **Step 2: Build and test the changed crates**

```bash
env -u CEF_PATH cargo test -p vmux_desktop
env -u CEF_PATH cargo build -p vmux_desktop
```
Expected: success.

- [ ] **Step 3: Commit**

```bash
git add crates/vmux_desktop/src/settings.rs
git commit -m "feat(vmux_desktop): register built-in providers, settings becomes overrides only"
```

---

## Task 21: Handle `AgentUrl::AppDefault` in spawn pipeline

**Files:**
- Modify: `crates/vmux_desktop/src/agent.rs` (the `match agent_url { ... }` block around lines 706–786)

Changed crates: `vmux_desktop`

Context: `crates/vmux_desktop/src/agent.rs` contains one `match agent_url` block (around line 706) inside the `"agent" =>` arm of the URL scheme dispatch. The arms are `Some(App)`, `Some(Cli)`, `None`. After Task 14 adds `AppDefault` to `AgentUrl::parse`, we need a new `Some(AppDefault)` arm.

- [ ] **Step 1: Add the AppDefault arm**

In `crates/vmux_desktop/src/agent.rs`, find the `match agent_url {` block (currently around line 706). Add a new arm immediately after `Some(vmux_agent::AgentUrl::App { ... }) => { ... Ok(()) }` and before `Some(vmux_agent::AgentUrl::Cli { ... }) => { ... }`:

```rust
                Some(vmux_agent::AgentUrl::AppDefault) => {
                    match vmux_agent::resolve_default_app_provider() {
                        Some(p) => {
                            let sid = uuid::Uuid::new_v4().to_string();
                            if spawn_app_agent_tab(
                                p.provider,
                                p.default_model,
                                pane,
                                &sid,
                                commands,
                                meshes,
                                webview_mt,
                                strategies,
                            )
                            .is_none()
                            {
                                return Err(format!(
                                    "no App agent strategy registered for {}/{}",
                                    p.provider, p.default_model
                                ));
                            }
                            Ok(())
                        }
                        None => {
                            bevy::log::warn!(
                                "vmux://agent/ requested but no provider API key is set; falling back to terminal"
                            );
                            spawn_terminal_tab(
                                pane, None, None, commands, meshes, webview_mt, settings,
                            );
                            Ok(())
                        }
                    }
                }
```

Reasoning:
- We reuse `spawn_app_agent_tab` (defined at line 462) — same path the explicit `App` arm uses.
- The no-key branch falls back to `spawn_terminal_tab` (visible elsewhere in the same `match`) so the user sees a shell instead of an empty pane. Better polish (a Dioxus "set your API key" page) is deferred to a follow-up — leaving a `warn!` log + functional terminal is the minimal correct behavior for v1.
- `p.provider` and `p.default_model` are `&'static str`; pass directly (no `.to_string()` — `spawn_app_agent_tab` takes `&str`).

- [ ] **Step 2: Build the crate**

```bash
env -u CEF_PATH cargo build -p vmux_desktop
```
Expected: success.

- [ ] **Step 3: Add a focused regression test**

In `crates/vmux_desktop/src/agent.rs`, locate the existing `#[cfg(test)] mod tests` block. Add (the file likely already has URL-routing tests for `App` and `Cli` variants — model the new test after one of those):

```rust
    #[test]
    fn app_default_url_parses() {
        let parsed = vmux_agent::AgentUrl::parse("vmux://agent/");
        assert!(matches!(parsed, Some(vmux_agent::AgentUrl::AppDefault)));
    }
```

If a richer dispatch test exists (one that actually calls the URL handler and asserts a pane was spawned), add an analogous one — set `MISTRAL_API_KEY=x` for the test, call the handler with `vmux://agent/`, assert `spawn_app_agent_tab` was invoked with `("mistral", "devstral-2", ...)`. Skip this if no such test pattern exists; the unit test above is sufficient.

- [ ] **Step 4: Run the test**

```bash
env -u CEF_PATH cargo test -p vmux_desktop agent::tests::app_default_url_parses
```
Expected: 1 test passes.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_desktop/src/agent.rs
git commit -m "feat(vmux_desktop): dispatch vmux://agent/ via resolve_default_app_provider"
```

---

## Task 22: Command bar — add "New chat" default entry

**Files:**
- Modify: `crates/vmux_desktop/src/command_bar.rs` (around line 419 — the `app_agent_entries` builder)

Changed crates: `vmux_desktop`

Context: the command bar already iterates `AgentStrategies::app_strategies()` (line 421) to generate one entry per registered `(provider, model)`. After Task 20 registers all three built-ins, those entries appear automatically — **no code change is needed for the per-provider actions**. What's missing is a single top-of-list "New chat" entry that opens `vmux://agent/` (resolves to default provider at spawn time).

- [ ] **Step 1: Prepend a default entry to `app_agent_entries`**

In `crates/vmux_desktop/src/command_bar.rs`, find the `let app_agent_entries = space_params.p3().map(...)` builder around line 421. After the `.collect()` produces the per-strategy `Vec<AppAgentEntry>`, prepend a "default" entry. Replace the whole `app_agent_entries` binding with:

```rust
    let app_agent_entries = {
        let mut entries: Vec<AppAgentEntry> = space_params
            .p3()
            .map(|strategies| {
                strategies
                    .app_strategies()
                    .map(|s| AppAgentEntry {
                        id: app_agent_id(s.provider(), s.model()),
                        name: format!("New {}/{} chat (App)", s.provider(), s.model()),
                        provider: s.provider().to_string(),
                        model: s.model().to_string(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        if let Some(default) = vmux_agent::resolve_default_app_provider() {
            entries.insert(
                0,
                AppAgentEntry {
                    id: app_agent_id(default.provider, default.default_model),
                    name: "New chat".to_string(),
                    provider: default.provider.to_string(),
                    model: default.default_model.to_string(),
                },
            );
        }
        entries
    };
```

Rationale: by reusing the same `id` (`app_agent_id(provider, model)`) and the existing `parse_app_agent_id` handler (line 1433), the default entry rides the existing dispatch path — no new handler needed. The user sees "New chat" as the first action; selecting it opens whichever (provider, model) is the current default.

- [ ] **Step 2: Build**

```bash
env -u CEF_PATH cargo build -p vmux_desktop
```
Expected: success.

- [ ] **Step 3: Sanity check (no new unit test required)**

`app_agent_entries` is local data; behavioral correctness is verified by manual smoke (Task 25 step 2). If a `command_list` or `app_agent_entries` test exists in this file (`grep -n 'fn .*command_list\|fn .*app_agent_entries' crates/vmux_desktop/src/command_bar.rs`), extend it; otherwise skip.

- [ ] **Step 4: Commit**

```bash
git add crates/vmux_desktop/src/command_bar.rs
git commit -m "feat(vmux_desktop): command bar 'New chat' default entry uses resolved provider"
```

---

## Task 23: AgentToast JS bridge (BinJsEmitEventPlugin)

**Files:**
- Modify: `crates/vmux_agent/src/app_plugin.rs`
- Modify: `crates/vmux_agent/Cargo.toml`

Changed crates: `vmux_agent`, downstream `vmux_desktop` (rebuilds)

- [ ] **Step 1: Add bevy_cef dep**

In `crates/vmux_agent/Cargo.toml`, in `[dependencies]` (alphabetical) add:

```toml
bevy_cef = { workspace = true }
```

If `bevy_cef` isn't already a workspace dep, check how other crates depend on it (e.g. `crates/vmux_webview_app/Cargo.toml`) and copy that line literally. If it's path-based (`{ path = "../bevy_cef" }`), use the same form.

This is acceptable: `vmux_agent` already depends on Bevy and rkyv; adding `bevy_cef` only for the JS-bridge plugin is in scope. If the dependency adds significant build time or it pulls CEF into non-CEF builds, alternative: move the bridge into a new small crate `vmux_agent_webview_bridge` — but for v1 add it to `vmux_agent` for simplicity.

- [ ] **Step 2: Register the plugin**

In `crates/vmux_agent/src/app_plugin.rs`, update the imports and `build`:

```rust
use bevy_cef::prelude::BinJsEmitEventPlugin;
```

In `Plugin for AppAgentPlugin::build`, add right after the `add_event::<AgentToast>()` call:

```rust
            .add_plugins(BinJsEmitEventPlugin::<AgentToast>::with_id(
                "vmux-agent-toast",
            ))
```

If `BinJsEmitEventPlugin::with_id` requires the event to implement `BinReceive`/`BinEmit`/etc, check the existing `JsEmitUiReadyPlugin` in `crates/vmux_webview_app/src/lib.rs` for the exact plugin name and trait requirements. The pattern there uses `BinJsEmitEventPlugin::<UiReady>::with_id(...)`. Match that pattern; the `AgentToast` struct already derives `rkyv::Archive + Serialize + Deserialize` (Task 4).

- [ ] **Step 3: Build**

```bash
env -u CEF_PATH cargo build -p vmux_agent
env -u CEF_PATH cargo test -p vmux_agent app_plugin
```
Expected: success.

- [ ] **Step 4: Commit**

```bash
git add crates/vmux_agent/Cargo.toml crates/vmux_agent/src/app_plugin.rs
git commit -m "feat(vmux_agent): emit AgentToast to JS via BinJsEmitEventPlugin"
```

---

## Task 24: Streaming smoke test (end-to-end mocked transport)

**Files:**
- Create: `crates/vmux_agent/tests/streaming_smoke.rs`

Changed crates: `vmux_agent`

- [ ] **Step 1: Write the smoke test**

Create `crates/vmux_agent/tests/streaming_smoke.rs`:

```rust
use std::sync::Arc;
use std::time::Duration;

use bevy::prelude::*;
use bevy::tasks::IoTaskPool;
use crossbeam_channel::unbounded;
use vmux_agent::{
    AgentApprovalPolicy, AgentKind, AgentMessages, AgentRunState, AgentSession, AgentVariant,
    AppAgentStrategy, AppAgentPlugin, LastRunStateKind, Message, PendingUserInput,
    providers::openai_shared::parse_chat_completions_sse,
    strategy::{AgentStrategies, AgentStrategy},
    stream::{StreamEvent, ToolDef},
};

struct MockMistral {
    url: String,
}

impl AgentStrategy for MockMistral {
    fn kind(&self) -> AgentKind {
        AgentKind::Vibe
    }
    fn variant(&self) -> AgentVariant {
        AgentVariant::App
    }
}

impl AppAgentStrategy for MockMistral {
    fn provider(&self) -> &str {
        "mistral"
    }
    fn model(&self) -> &str {
        "devstral-2"
    }
    fn endpoint(&self) -> &str {
        &self.url
    }
    fn env_var(&self) -> &'static str {
        ""
    }
    fn build_request(
        &self,
        _: &str,
        _: &[Message],
        _: &[ToolDef],
        _: &str,
    ) -> reqwest::Request {
        reqwest::Client::new()
            .post(&self.url)
            .body("{}")
            .build()
            .unwrap()
    }
    fn parse_sse_event(&self, payload: &str) -> Option<StreamEvent> {
        parse_chat_completions_sse(payload)
    }
}

#[test]
fn single_text_turn_streams_into_assistant_message() {
    let mut server = mockito::Server::new();
    let body = include_str!("fixtures/mistral/text.sse");
    let _m = server
        .mock("POST", "/chat")
        .with_status(200)
        .with_header("content-type", "text/event-stream")
        .with_body(body)
        .create();

    let mut app = App::new();
    app.add_plugins(bevy::app::TaskPoolPlugin::default());
    app.add_plugins(AppAgentPlugin);
    let mut strategies = AgentStrategies::default();
    strategies.register_app(Arc::new(MockMistral {
        url: format!("{}/chat", server.url()),
    }));
    app.insert_resource(strategies);

    let entity = app
        .world_mut()
        .spawn((
            AgentSession {
                kind: AgentKind::Vibe,
                variant: AgentVariant::App,
                sid: "smoke".into(),
                provider: "mistral".into(),
                model: "devstral-2".into(),
            },
            AgentMessages::default(),
            AgentApprovalPolicy::default(),
            AgentRunState::Idle,
            LastRunStateKind::default(),
            PendingUserInput("hi".into()),
        ))
        .id();

    // Spin until state becomes Idle (after streaming finishes).
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while std::time::Instant::now() < deadline {
        app.update();
        if matches!(
            app.world().get::<AgentRunState>(entity),
            Some(AgentRunState::Idle)
        ) {
            let msgs = app.world().get::<AgentMessages>(entity).unwrap();
            let last = msgs.0.last().unwrap();
            if let Message::Assistant { blocks } = last {
                let text: String = blocks
                    .iter()
                    .filter_map(|b| match b {
                        vmux_agent::AssistantBlock::Text(t) => Some(t.clone()),
                        _ => None,
                    })
                    .collect();
                if text == "hello world" {
                    return;
                }
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("did not reach Idle with assistant 'hello world' within 5s");
}
```

- [ ] **Step 2: Run the smoke test**

```bash
env -u CEF_PATH cargo test -p vmux_agent --test streaming_smoke
```
Expected: 1 test passes.

- [ ] **Step 3: Commit**

```bash
git add crates/vmux_agent/tests/streaming_smoke.rs
git commit -m "test(vmux_agent): end-to-end streaming smoke with mockito"
```

---

## Task 25: Full crate sweep + manual smoke checklist

**Files:** none (verification)

Changed crates: `vmux_agent`, `vmux_desktop`

- [ ] **Step 1: Run fmt + clippy + test on changed crates**

```bash
PKGS=$(BASE=origin/main ./scripts/changed-crates.sh)
echo "Changed packages: $PKGS"
for pkg in $PKGS; do cargo fmt -p "$pkg" -- --check; done
for pkg in $PKGS; do env -u CEF_PATH cargo clippy -p "$pkg" --all-targets -- -D warnings; done
for pkg in $PKGS; do env -u CEF_PATH cargo test -p "$pkg"; done
```
Expected: all three loops pass. If fmt fails, run `cargo fmt -p "$pkg"` and re-stage with a fixup commit (do not amend a published commit).

- [ ] **Step 2: Manual smoke — Mistral**

```bash
export MISTRAL_API_KEY=<your-key>
env -u CEF_PATH cargo run -p vmux_desktop
```
In the app, run the command-bar "New chat" action. Verify the chat opens, the URL bar shows `vmux://agent/mistral/devstral-2/<uuid>`, type "hello — list_spaces" and press send. Expected: text streams in. The model should call `list_spaces`, you see an approval prompt (or auto-approval if you opted in), the tool result appears, then a final text response.

- [ ] **Step 3: Manual smoke — Anthropic**

```bash
unset MISTRAL_API_KEY
export ANTHROPIC_API_KEY=<your-key>
env -u CEF_PATH cargo run -p vmux_desktop
```
In the app, run "New chat". URL should be `vmux://agent/anthropic/claude-sonnet-4-6/<uuid>`. Repeat the text + tool test.

- [ ] **Step 4: Manual smoke — OpenAI**

```bash
unset ANTHROPIC_API_KEY
export OPENAI_API_KEY=<your-key>
env -u CEF_PATH cargo run -p vmux_desktop
```
In the app, run "New chat". URL should be `vmux://agent/openai/gpt-5/<uuid>`. Repeat.

- [ ] **Step 5: Manual smoke — no keys (error path)**

```bash
unset MISTRAL_API_KEY ANTHROPIC_API_KEY OPENAI_API_KEY
env -u CEF_PATH cargo run -p vmux_desktop
```
"New chat" should open a placeholder pane (or log a warning, depending on Task 21's choice). The default URL fallback should not panic. Toast should appear with "Missing ..." or similar.

- [ ] **Step 6: Manual smoke — error toast on invalid key**

```bash
export ANTHROPIC_API_KEY=invalid-key
env -u CEF_PATH cargo run -p vmux_desktop
```
New chat → type "hi". Expected: toast appears with "HTTP 401: ..." text and an inline ⚠ message in the chat.

- [ ] **Step 7: Open the PR**

After all smokes pass, follow `superpowers:open-new-pr`:

```bash
git push -u origin vmx-gui-agent
gh pr create --title "feat(vmux_agent): real App agent providers (mistral/anthropic/openai)" --body "$(cat <<'EOF'
## Summary
- Replace EchoAppStrategy with real MistralStrategy / OpenAiResponsesStrategy / AnthropicStrategy
- Add SSE driver, built-in defaults, default-URL routing via `vmux://agent/`
- Full multi-turn MCP tool loop with approval + continue-after-tool system
- Inline error + toast feedback in Dioxus

## Test plan
- [x] Unit tests pass (per-provider SSE parsing + request building, builtin precedence, AgentUrl::AppDefault)
- [x] System tests pass (process_user_input, drain_stream, continue_after_tool, surface_errors)
- [x] End-to-end smoke (mockito) passes
- [ ] Manual: Mistral chat + tool call
- [ ] Manual: Anthropic chat + tool call
- [ ] Manual: OpenAI chat + tool call
- [ ] Manual: missing-keys placeholder + error toast
EOF
)"
```

- [ ] **Step 8: Delete the plan file**

```bash
git rm docs/plans/2026-05-16-real-app-agent-providers.md
git commit -m "chore: remove implemented plan"
```

(Per `AGENTS.md`: delete the plan file once fully implemented.)

---

## Out of scope (follow-up specs)

- Token usage display in toolbar (per-turn input/output tokens)
- Streaming reasoning blocks (Anthropic `thinking`, OpenAI o-series)
- Provider model lists fetched at runtime (currently hard-coded built-ins)
- Cost tracking, conversation export/import
- Per-tool approval modal polish
- Keychain-based key storage (env vars only for v1)
- vmux_webview_app Dioxus toast subscriber wiring (this plan emits the JS event; the Dioxus listener side is a follow-up if a chat UI doesn't exist yet)
