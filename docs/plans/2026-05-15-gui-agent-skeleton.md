# GUI Agent Skeleton Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add GUI variant of agent panes alongside existing CLI wrappers. End-to-end plumbing through stub `EchoGuiStrategy` (no real provider yet). Step 2 of `docs/specs/2026-05-15-gui-agent-pane-design.md`.

**Architecture:** ECS-native state machine. `AgentVariant::Gui` registered into the existing `AgentStrategies` registry alongside `Cli`. Per-session components hold conversation + run state; Bevy systems advance the state machine each frame. Provider strategies are pure data + parsing; generic systems do all I/O via `IoTaskPool::spawn` returning `Task<T>` polled per frame.

**Tech Stack:** Bevy 0.18 ECS, `crossbeam_channel` for stream piping, `reqwest` for future provider HTTP (this plan stubs it), `moonshine-save` for persistence, Dioxus 0.7 for the (stub) chat UI.

**Depends on:** Step 1 (URL migration) landed in `main`. Specifically: `vmux_agent::AgentKind` enum exists, `AgentStrategy` trait + registry exist, URL scheme is `vmux://agent/<kind>/[cli/]<sid>`, CLI wrappers serve under the `cli/` segment. If step 1 is not yet merged, rebase this branch onto post-merge `main` before starting.

---

## File Structure

**New files** (all under `crates/vmux_agent/src/` unless noted):
- `variant.rs` — `AgentVariant` enum
- `message.rs` — `Message`, `AssistantBlock`, `ToolUse`, `ToolResult`
- `stream.rs` — `StreamEvent`, `ToolDef`, `PartialToolUse`
- `components.rs` — durable session components (`AgentSession`, `AgentMessages`, `AgentApprovalPolicy`, `PendingUserInput`)
- `run_state.rs` — `AgentRunState` (non-serializable runtime state)
- `events.rs` — `AgentInput`, `AgentDelta`, `AgentToolStatus`, `AgentApprovalRequest`, `AgentApprovalReply` Bevy events
- `gui.rs` — `GuiAgentStrategy` trait
- `cli_trait.rs` — moves the existing CLI-only methods out of `AgentStrategy` into `CliAgentStrategy` sub-trait (rename current `strategy.rs` content)
- `echo.rs` — stub `EchoGuiStrategy` for one provider (`vibe`)
- `systems/process_input.rs` — `process_user_input` system
- `systems/drain_stream.rs` — `drain_stream` system
- `systems/dispatch_tool.rs` — `dispatch_tool` system
- `systems/approval.rs` — `handle_approval_reply` system
- `gui_plugin.rs` — `GuiAgentPlugin` wiring components, events, systems

**Modified files:**
- `crates/vmux_agent/src/lib.rs` — re-export new modules
- `crates/vmux_agent/src/strategy.rs` — slim down to core trait + registry change
- `crates/vmux_agent/src/{vibe,claude,codex}.rs` — implement new core trait + `CliAgentStrategy` sub-trait
- `crates/vmux_agent/src/plugin.rs` — register `AgentStrategies` keyed by `(AgentKind, AgentVariant)`
- `crates/vmux_desktop/src/...` — URL routing recognises bare `vmux://agent/<kind>/<sid>` as Gui variant; spawn pane wires Dioxus stub page
- `crates/vmux_desktop/src/command_bar.rs` — three new actions ("New Vibe chat", "New Claude chat", "New Codex chat") that mint a fresh UUID and open `vmux://agent/<kind>/<uuid>`
- `crates/vmux_webview_app/src/lib.rs` — register stub Dioxus page for `agent/<provider>/<sid>` route

**Test files** (mirror module structure):
- `crates/vmux_agent/src/variant.rs` — inline `#[cfg(test)] mod tests`
- `crates/vmux_agent/src/message.rs` — inline tests
- `crates/vmux_agent/src/stream.rs` — inline tests
- `crates/vmux_agent/src/echo.rs` — inline tests
- `crates/vmux_agent/src/systems/*.rs` — each system has inline integration test using a minimal `App`
- `crates/vmux_agent/tests/echo_smoke.rs` — end-to-end smoke

---

## Task 1: AgentVariant enum

**Files:**
- Create: `crates/vmux_agent/src/variant.rs`
- Modify: `crates/vmux_agent/src/lib.rs`

- [ ] **Step 1: Write the failing test**

Create `crates/vmux_agent/src/variant.rs`:

```rust
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum AgentVariant {
    Gui,
    Cli,
}

impl AgentVariant {
    pub fn as_url_segment(self) -> Option<&'static str> {
        match self {
            AgentVariant::Gui => None,
            AgentVariant::Cli => Some("cli"),
        }
    }

    pub fn from_url_segment(segment: Option<&str>) -> Option<Self> {
        match segment {
            None | Some("") => Some(AgentVariant::Gui),
            Some("cli") => Some(AgentVariant::Cli),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_segment_round_trips() {
        for v in [AgentVariant::Gui, AgentVariant::Cli] {
            assert_eq!(AgentVariant::from_url_segment(v.as_url_segment()), Some(v));
        }
    }

    #[test]
    fn empty_segment_resolves_to_gui() {
        assert_eq!(AgentVariant::from_url_segment(Some("")), Some(AgentVariant::Gui));
        assert_eq!(AgentVariant::from_url_segment(None), Some(AgentVariant::Gui));
    }

    #[test]
    fn unknown_segment_returns_none() {
        assert_eq!(AgentVariant::from_url_segment(Some("nope")), None);
    }
}
```

Add to `crates/vmux_agent/src/lib.rs`:

```rust
mod variant;
pub use variant::AgentVariant;
```

- [ ] **Step 2: Run test to verify it passes**

```
env -u CEF_PATH cargo test -p vmux_agent variant::tests
```
Expected: 3 tests pass.

- [ ] **Step 3: Commit**

```
git add crates/vmux_agent/src/variant.rs crates/vmux_agent/src/lib.rs
git commit -m "feat(vmux_agent): add AgentVariant enum"
```

---

## Task 2: Split AgentStrategy into core + CliAgentStrategy sub-trait

**Files:**
- Modify: `crates/vmux_agent/src/strategy.rs`
- Create: `crates/vmux_agent/src/cli_trait.rs`
- Modify: `crates/vmux_agent/src/{vibe,claude,codex}.rs`
- Modify: `crates/vmux_agent/src/lib.rs`

- [ ] **Step 1: Slim down `strategy.rs` to core trait only**

Replace the trait definition in `crates/vmux_agent/src/strategy.rs` with:

```rust
use crate::{AgentKind, AgentVariant};

pub trait AgentStrategy: Send + Sync + 'static {
    fn kind(&self) -> AgentKind;
    fn variant(&self) -> AgentVariant;
}
```

(Registry stays — it gets reworked in Task 3.)

- [ ] **Step 2: Move CLI-only methods to new `cli_trait.rs`**

Create `crates/vmux_agent/src/cli_trait.rs`:

```rust
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::strategy::AgentStrategy;
use crate::McpServerConfig;

pub trait CliAgentStrategy: AgentStrategy {
    fn sessions_root(&self) -> PathBuf;
    fn build_args(&self, mcp: &McpServerConfig, session_id: Option<&str>) -> Vec<String>;
    fn build_env(&self, mcp: &McpServerConfig) -> Vec<(String, String)>;
    fn discover_session(
        &self,
        cwd: &Path,
        spawn_time: SystemTime,
        claimed: &HashSet<String>,
    ) -> Option<String>;
    fn detect_end_time(&self, session_id: &str) -> bool;
}
```

Add to `crates/vmux_agent/src/lib.rs`:

```rust
mod cli_trait;
pub use cli_trait::CliAgentStrategy;
```

- [ ] **Step 3: Update VibeStrategy to implement both traits**

In `crates/vmux_agent/src/vibe.rs`, change:

```rust
impl AgentStrategy for VibeStrategy {
    fn kind(&self) -> AgentKind { AgentKind::Vibe }
    fn variant(&self) -> AgentVariant { AgentVariant::Cli }
}

impl CliAgentStrategy for VibeStrategy {
    // (move existing sessions_root, build_args, build_env, discover_session, detect_end_time here)
}
```

Repeat for `claude.rs` and `codex.rs`.

- [ ] **Step 4: Run tests to verify CLI strategies still work**

```
env -u CEF_PATH cargo test -p vmux_agent
```
Expected: all existing CLI strategy tests still pass.

- [ ] **Step 5: Commit**

```
git add crates/vmux_agent/src/strategy.rs crates/vmux_agent/src/cli_trait.rs crates/vmux_agent/src/{vibe,claude,codex}.rs crates/vmux_agent/src/lib.rs
git commit -m "refactor(vmux_agent): split AgentStrategy into core + CliAgentStrategy sub-trait"
```

---

## Task 3: Registry keyed by (AgentKind, AgentVariant)

**Files:**
- Modify: `crates/vmux_agent/src/strategy.rs`
- Modify: `crates/vmux_agent/src/plugin.rs`

- [ ] **Step 1: Write the failing test**

Append to `crates/vmux_agent/src/strategy.rs`:

```rust
#[cfg(test)]
mod registry_variant_tests {
    use super::*;
    use crate::AgentVariant;
    use std::path::PathBuf;

    struct StubCli;
    impl AgentStrategy for StubCli {
        fn kind(&self) -> AgentKind { AgentKind::Vibe }
        fn variant(&self) -> AgentVariant { AgentVariant::Cli }
    }

    struct StubGui;
    impl AgentStrategy for StubGui {
        fn kind(&self) -> AgentKind { AgentKind::Vibe }
        fn variant(&self) -> AgentVariant { AgentVariant::Gui }
    }

    #[test]
    fn registers_cli_and_gui_for_same_kind_independently() {
        let mut s = AgentStrategies::default();
        s.register(Box::new(StubCli));
        s.register(Box::new(StubGui));
        assert!(s.get(AgentKind::Vibe, AgentVariant::Cli).is_some());
        assert!(s.get(AgentKind::Vibe, AgentVariant::Gui).is_some());
        assert!(s.get(AgentKind::Claude, AgentVariant::Gui).is_none());
    }
}
```

- [ ] **Step 2: Run test, see it fail (signature mismatch)**

```
env -u CEF_PATH cargo test -p vmux_agent registry_variant_tests
```
Expected: compile error — `get()` takes one arg, not two.

- [ ] **Step 3: Rework registry**

Replace `AgentStrategies` body in `crates/vmux_agent/src/strategy.rs`:

```rust
use crate::{AgentKind, AgentVariant};
use bevy::prelude::Resource;
use std::collections::HashMap;

#[derive(Resource, Default)]
pub struct AgentStrategies {
    inner: HashMap<(AgentKind, AgentVariant), Box<dyn AgentStrategy>>,
}

impl AgentStrategies {
    pub fn register(&mut self, strategy: Box<dyn AgentStrategy>) {
        let key = (strategy.kind(), strategy.variant());
        self.inner.insert(key, strategy);
    }

    pub fn get(&self, kind: AgentKind, variant: AgentVariant) -> Option<&dyn AgentStrategy> {
        self.inner.get(&(kind, variant)).map(|b| b.as_ref())
    }

    pub fn iter(&self) -> impl Iterator<Item = (&(AgentKind, AgentVariant), &dyn AgentStrategy)> {
        self.inner.iter().map(|(k, v)| (k, v.as_ref()))
    }
}
```

Update existing call sites in `crates/vmux_agent/src/plugin.rs` and any consumer to pass `AgentVariant::Cli` for the existing CLI strategies.

- [ ] **Step 4: Run all vmux_agent tests**

```
env -u CEF_PATH cargo test -p vmux_agent
```
Expected: all pass (existing tests + new variant test).

- [ ] **Step 5: Commit**

```
git add crates/vmux_agent/src/strategy.rs crates/vmux_agent/src/plugin.rs
git commit -m "feat(vmux_agent): registry keyed by (AgentKind, AgentVariant)"
```

---

## Task 4: GuiAgentStrategy sub-trait

**Files:**
- Create: `crates/vmux_agent/src/gui.rs`
- Modify: `crates/vmux_agent/src/lib.rs`

- [ ] **Step 1: Write the trait file**

Create `crates/vmux_agent/src/gui.rs`:

```rust
use crate::message::Message;
use crate::stream::{StreamEvent, ToolDef};
use crate::strategy::AgentStrategy;

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

Add to `crates/vmux_agent/src/lib.rs`:

```rust
mod gui;
pub use gui::GuiAgentStrategy;
```

(Note: `message` and `stream` modules don't exist yet — Tasks 5 and 6 add them. Mark this task as compile-failing until those land; that's fine in TDD order. Run the build after Task 6.)

- [ ] **Step 2: Commit**

```
git add crates/vmux_agent/src/gui.rs crates/vmux_agent/src/lib.rs
git commit -m "feat(vmux_agent): GuiAgentStrategy trait stub"
```

---

## Task 5: StreamEvent + ToolDef

**Files:**
- Create: `crates/vmux_agent/src/stream.rs`
- Modify: `crates/vmux_agent/src/lib.rs`

- [ ] **Step 1: Write the types and tests**

Create `crates/vmux_agent/src/stream.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum StreamEvent {
    TextDelta(String),
    ToolUseStart { call_id: String, name: String },
    ToolUseArgsDelta { call_id: String, json_chunk: String },
    ToolUseEnd { call_id: String },
    StopTurn { reason: StopReason },
    Error(String),
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum StopReason {
    EndTurn,
    ToolUse,
    MaxTokens,
    Other,
}

#[derive(Clone, Debug)]
pub struct ToolDef {
    pub name: &'static str,
    pub description: &'static str,
    pub input_schema: serde_json::Value,
    pub read_only: bool,
}

#[derive(Clone, Debug, Default)]
pub struct PartialToolUse {
    pub call_id: String,
    pub name: String,
    pub args_buf: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde_roundtrip_text_delta() {
        let e = StreamEvent::TextDelta("hi".into());
        let json = serde_json::to_string(&e).unwrap();
        let back: StreamEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(e, back);
    }

    #[test]
    fn stop_reason_serializes_as_variant_name() {
        let json = serde_json::to_string(&StopReason::EndTurn).unwrap();
        assert_eq!(json, "\"EndTurn\"");
    }
}
```

Add to `crates/vmux_agent/src/lib.rs`:

```rust
mod stream;
pub use stream::{PartialToolUse, StopReason, StreamEvent, ToolDef};
```

- [ ] **Step 2: Run tests**

```
env -u CEF_PATH cargo test -p vmux_agent stream::tests
```
Expected: 2 tests pass.

- [ ] **Step 3: Commit**

```
git add crates/vmux_agent/src/stream.rs crates/vmux_agent/src/lib.rs
git commit -m "feat(vmux_agent): StreamEvent, StopReason, ToolDef, PartialToolUse"
```

---

## Task 6: Message types

**Files:**
- Create: `crates/vmux_agent/src/message.rs`
- Modify: `crates/vmux_agent/src/lib.rs`

- [ ] **Step 1: Write types and tests**

Create `crates/vmux_agent/src/message.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum Message {
    User { text: String },
    Assistant { blocks: Vec<AssistantBlock> },
    ToolResult { call_id: String, content: String, is_error: bool },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum AssistantBlock {
    Text(String),
    ToolUse { call_id: String, name: String, args: serde_json::Value },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_roundtrip() {
        let m = Message::User { text: "hi".into() };
        let json = serde_json::to_string(&m).unwrap();
        let back: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(m, back);
    }

    #[test]
    fn assistant_blocks_roundtrip() {
        let m = Message::Assistant {
            blocks: vec![
                AssistantBlock::Text("hello".into()),
                AssistantBlock::ToolUse {
                    call_id: "abc".into(),
                    name: "list_spaces".into(),
                    args: serde_json::json!({}),
                },
            ],
        };
        let json = serde_json::to_string(&m).unwrap();
        let back: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(m, back);
    }

    #[test]
    fn tool_result_roundtrip() {
        let m = Message::ToolResult {
            call_id: "abc".into(),
            content: "ok".into(),
            is_error: false,
        };
        let json = serde_json::to_string(&m).unwrap();
        let back: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(m, back);
    }
}
```

Add to `crates/vmux_agent/src/lib.rs`:

```rust
mod message;
pub use message::{AssistantBlock, Message};
```

- [ ] **Step 2: Run tests**

```
env -u CEF_PATH cargo test -p vmux_agent message::tests
```
Expected: 3 tests pass. Also confirms Task 4's `gui.rs` now compiles.

- [ ] **Step 3: Commit**

```
git add crates/vmux_agent/src/message.rs crates/vmux_agent/src/lib.rs
git commit -m "feat(vmux_agent): Message + AssistantBlock"
```

---

## Task 7: Session components

**Files:**
- Create: `crates/vmux_agent/src/components.rs`
- Modify: `crates/vmux_agent/src/lib.rs`

- [ ] **Step 1: Write components and tests**

Create `crates/vmux_agent/src/components.rs`:

```rust
use std::collections::HashSet;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::message::Message;
use crate::{AgentKind, AgentVariant};

#[derive(Component, Clone, Debug, Serialize, Deserialize, Reflect)]
#[reflect(Component)]
pub struct AgentSession {
    pub kind: AgentKind,
    pub variant: AgentVariant,
    pub sid: String,
    pub provider: String,
    pub model: String,
}

#[derive(Component, Clone, Debug, Default, Serialize, Deserialize, Reflect)]
#[reflect(Component)]
pub struct AgentMessages(pub Vec<Message>);

#[derive(Component, Clone, Debug, Default, Serialize, Deserialize, Reflect)]
#[reflect(Component)]
pub struct AgentApprovalPolicy {
    pub auto: HashSet<String>,
}

#[derive(Component, Clone, Debug)]
pub struct PendingUserInput(pub String);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_components_default_constructible() {
        let _ = AgentMessages::default();
        let _ = AgentApprovalPolicy::default();
    }
}
```

Note: `AgentKind` and `AgentVariant` need `#[derive(Serialize, Deserialize, Reflect)]` for these components to roundtrip via moonshine-save. Add those derives if missing.

Add to `crates/vmux_agent/src/lib.rs`:

```rust
mod components;
pub use components::{AgentApprovalPolicy, AgentMessages, AgentSession, PendingUserInput};
```

- [ ] **Step 2: Run tests**

```
env -u CEF_PATH cargo test -p vmux_agent components::tests
```
Expected: 1 test passes.

- [ ] **Step 3: Commit**

```
git add crates/vmux_agent/src/components.rs crates/vmux_agent/src/lib.rs crates/vmux_agent/src/{kind,variant}.rs
git commit -m "feat(vmux_agent): durable session components"
```

---

## Task 8: AgentRunState component

**Files:**
- Create: `crates/vmux_agent/src/run_state.rs`
- Modify: `crates/vmux_agent/src/lib.rs`

- [ ] **Step 1: Define the component**

Create `crates/vmux_agent/src/run_state.rs`:

```rust
use bevy::prelude::*;
use bevy::tasks::Task;
use crossbeam_channel::Receiver;

use crate::stream::{PartialToolUse, StreamEvent};

#[derive(Component)]
pub enum AgentRunState {
    Idle,
    Streaming {
        rx: Receiver<StreamEvent>,
        _task: Task<()>,
        partial: Option<PartialToolUse>,
    },
    RunningTool {
        call_id: String,
        task: Task<ToolDispatchOutput>,
    },
    AwaitingApproval {
        call_id: String,
        name: String,
        args: serde_json::Value,
    },
    Errored(String),
}

#[derive(Clone, Debug)]
pub struct ToolDispatchOutput {
    pub call_id: String,
    pub content: String,
    pub is_error: bool,
}

impl Default for AgentRunState {
    fn default() -> Self { Self::Idle }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_idle() {
        assert!(matches!(AgentRunState::default(), AgentRunState::Idle));
    }

    #[test]
    fn errored_holds_message() {
        let s = AgentRunState::Errored("oops".into());
        match s {
            AgentRunState::Errored(m) => assert_eq!(m, "oops"),
            _ => panic!("wrong variant"),
        }
    }
}
```

Add to `crates/vmux_agent/src/lib.rs`:

```rust
mod run_state;
pub use run_state::{AgentRunState, ToolDispatchOutput};
```

Add `crossbeam-channel = "0.5"` to `crates/vmux_agent/Cargo.toml` `[dependencies]` if not present.

- [ ] **Step 2: Run tests**

```
env -u CEF_PATH cargo test -p vmux_agent run_state::tests
```
Expected: 2 tests pass.

- [ ] **Step 3: Commit**

```
git add crates/vmux_agent/src/run_state.rs crates/vmux_agent/src/lib.rs crates/vmux_agent/Cargo.toml
git commit -m "feat(vmux_agent): AgentRunState component"
```

---

## Task 9: Bevy events

**Files:**
- Create: `crates/vmux_agent/src/events.rs`
- Modify: `crates/vmux_agent/src/lib.rs`

- [ ] **Step 1: Write events**

Create `crates/vmux_agent/src/events.rs`:

```rust
use bevy::prelude::*;
use serde_json::Value;

#[derive(Event, Clone, Debug)]
pub struct AgentInput {
    pub session: Entity,
    pub text: String,
}

#[derive(Event, Clone, Debug)]
pub struct AgentDelta {
    pub session: Entity,
    pub text: String,
}

#[derive(Event, Clone, Debug)]
pub struct AgentToolStatus {
    pub session: Entity,
    pub call_id: String,
    pub status: ToolStatus,
}

#[derive(Clone, Debug)]
pub enum ToolStatus {
    Pending,
    Running,
    Result { content: String, is_error: bool },
}

#[derive(Event, Clone, Debug)]
pub struct AgentApprovalRequest {
    pub session: Entity,
    pub call_id: String,
    pub name: String,
    pub args: Value,
}

#[derive(Event, Clone, Debug)]
pub struct AgentApprovalReply {
    pub session: Entity,
    pub call_id: String,
    pub decision: ApprovalDecision,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApprovalDecision {
    Allow,
    AllowAlways,
    Deny,
}
```

Add to `crates/vmux_agent/src/lib.rs`:

```rust
mod events;
pub use events::{
    AgentApprovalReply, AgentApprovalRequest, AgentDelta, AgentInput, AgentToolStatus,
    ApprovalDecision, ToolStatus,
};
```

- [ ] **Step 2: Build to verify it compiles**

```
env -u CEF_PATH cargo build -p vmux_agent
```
Expected: clean build.

- [ ] **Step 3: Commit**

```
git add crates/vmux_agent/src/events.rs crates/vmux_agent/src/lib.rs
git commit -m "feat(vmux_agent): GUI agent Bevy events"
```

---

## Task 10: EchoGuiStrategy stub

**Files:**
- Create: `crates/vmux_agent/src/echo.rs`
- Modify: `crates/vmux_agent/src/lib.rs`

- [ ] **Step 1: Write the stub strategy**

Create `crates/vmux_agent/src/echo.rs`:

```rust
use crate::gui::GuiAgentStrategy;
use crate::message::Message;
use crate::stream::{StopReason, StreamEvent, ToolDef};
use crate::strategy::AgentStrategy;
use crate::{AgentKind, AgentVariant};

pub struct EchoGuiStrategy;

impl AgentStrategy for EchoGuiStrategy {
    fn kind(&self) -> AgentKind { AgentKind::Vibe }
    fn variant(&self) -> AgentVariant { AgentVariant::Gui }
}

impl GuiAgentStrategy for EchoGuiStrategy {
    fn models(&self) -> &'static [&'static str] { &["echo-stub"] }
    fn default_model(&self) -> &'static str { "echo-stub" }
    fn endpoint(&self) -> &'static str { "stub://echo" }

    fn build_request(
        &self,
        _model: &str,
        _messages: &[Message],
        _tools: &[ToolDef],
        _api_key: &str,
    ) -> reqwest::Request {
        reqwest::Client::new()
            .get("http://localhost/echo-stub-unused")
            .build()
            .unwrap()
    }

    fn parse_sse_event(&self, _payload: &str) -> Option<StreamEvent> {
        None
    }
}

pub fn synthetic_echo_stream(text: &str) -> Vec<StreamEvent> {
    vec![
        StreamEvent::TextDelta(format!("echo: {text}")),
        StreamEvent::StopTurn { reason: StopReason::EndTurn },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn echo_stream_returns_text_then_stop() {
        let events = synthetic_echo_stream("hi");
        assert_eq!(events.len(), 2);
        match &events[0] {
            StreamEvent::TextDelta(t) => assert_eq!(t, "echo: hi"),
            _ => panic!("expected text delta"),
        }
        assert!(matches!(events[1], StreamEvent::StopTurn { .. }));
    }
}
```

Add `reqwest = { version = "0.12", default-features = false, features = ["rustls-tls"] }` to `crates/vmux_agent/Cargo.toml` if not already there.

Add to `crates/vmux_agent/src/lib.rs`:

```rust
mod echo;
pub use echo::{EchoGuiStrategy, synthetic_echo_stream};
```

- [ ] **Step 2: Run tests**

```
env -u CEF_PATH cargo test -p vmux_agent echo::tests
```
Expected: 1 test passes.

- [ ] **Step 3: Commit**

```
git add crates/vmux_agent/src/echo.rs crates/vmux_agent/src/lib.rs crates/vmux_agent/Cargo.toml
git commit -m "feat(vmux_agent): EchoGuiStrategy stub for end-to-end smoke"
```

---

## Task 11: process_user_input system

**Files:**
- Create: `crates/vmux_agent/src/systems/process_input.rs`
- Modify: `crates/vmux_agent/src/lib.rs`

- [ ] **Step 1: Write the system + test**

Create `crates/vmux_agent/src/systems/process_input.rs`:

```rust
use bevy::prelude::*;
use bevy::tasks::IoTaskPool;
use crossbeam_channel::unbounded;

use crate::components::{AgentMessages, AgentSession, PendingUserInput};
use crate::echo::synthetic_echo_stream;
use crate::message::Message;
use crate::run_state::AgentRunState;
use crate::stream::StreamEvent;

pub fn process_user_input(
    mut commands: Commands,
    mut q: Query<
        (Entity, &PendingUserInput, &mut AgentMessages, &mut AgentRunState, &AgentSession),
        With<PendingUserInput>,
    >,
) {
    for (entity, pending, mut messages, mut state, _session) in &mut q {
        if !matches!(*state, AgentRunState::Idle) {
            continue;
        }
        messages.0.push(Message::User { text: pending.0.clone() });

        let (tx, rx) = unbounded::<StreamEvent>();
        let text = pending.0.clone();
        let task = IoTaskPool::get().spawn(async move {
            for event in synthetic_echo_stream(&text) {
                let _ = tx.send(event);
            }
        });

        *state = AgentRunState::Streaming { rx, _task: task, partial: None };
        commands.entity(entity).remove::<PendingUserInput>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AgentKind, AgentVariant};

    fn make_app() -> App {
        let mut app = App::new();
        app.add_plugins(bevy::tasks::TaskPoolPlugin::default());
        app.add_systems(Update, process_user_input);
        app
    }

    #[test]
    fn pending_input_transitions_to_streaming() {
        let mut app = make_app();
        let entity = app.world_mut().spawn((
            AgentSession {
                kind: AgentKind::Vibe,
                variant: AgentVariant::Gui,
                sid: "test".into(),
                provider: "vibe".into(),
                model: "echo-stub".into(),
            },
            AgentMessages::default(),
            AgentRunState::Idle,
            PendingUserInput("hi".into()),
        )).id();

        app.update();

        let world = app.world();
        let state = world.get::<AgentRunState>(entity).unwrap();
        assert!(matches!(state, AgentRunState::Streaming { .. }));
        assert!(world.get::<PendingUserInput>(entity).is_none());
        let msgs = world.get::<AgentMessages>(entity).unwrap();
        assert_eq!(msgs.0.len(), 1);
        match &msgs.0[0] {
            Message::User { text } => assert_eq!(text, "hi"),
            _ => panic!("expected user message"),
        }
    }
}
```

Add to `crates/vmux_agent/src/lib.rs`:

```rust
pub mod systems {
    pub mod process_input;
}
```

- [ ] **Step 2: Run tests**

```
env -u CEF_PATH cargo test -p vmux_agent systems::process_input::tests
```
Expected: 1 test passes.

- [ ] **Step 3: Commit**

```
git add crates/vmux_agent/src/systems/process_input.rs crates/vmux_agent/src/lib.rs
git commit -m "feat(vmux_agent): process_user_input system"
```

---

## Task 12: drain_stream system

**Files:**
- Create: `crates/vmux_agent/src/systems/drain_stream.rs`
- Modify: `crates/vmux_agent/src/lib.rs`

- [ ] **Step 1: Write the system + test**

Create `crates/vmux_agent/src/systems/drain_stream.rs`:

```rust
use bevy::prelude::*;

use crate::components::AgentMessages;
use crate::events::{AgentDelta, AgentToolStatus, ToolStatus};
use crate::message::{AssistantBlock, Message};
use crate::run_state::AgentRunState;
use crate::stream::{StopReason, StreamEvent};

pub fn drain_stream(
    mut q: Query<(Entity, &mut AgentRunState, &mut AgentMessages)>,
    mut delta_events: EventWriter<AgentDelta>,
    mut tool_events: EventWriter<AgentToolStatus>,
) {
    for (entity, mut state, mut messages) in &mut q {
        let drained: Vec<StreamEvent> = match &*state {
            AgentRunState::Streaming { rx, .. } => rx.try_iter().collect(),
            _ => continue,
        };
        if drained.is_empty() { continue; }

        ensure_assistant_tail(&mut messages);

        let mut should_idle = false;
        for event in drained {
            match event {
                StreamEvent::TextDelta(text) => {
                    append_text_delta(&mut messages, &text);
                    delta_events.write(AgentDelta { session: entity, text });
                }
                StreamEvent::ToolUseEnd { call_id } => {
                    tool_events.write(AgentToolStatus {
                        session: entity,
                        call_id,
                        status: ToolStatus::Pending,
                    });
                }
                StreamEvent::StopTurn { reason: StopReason::EndTurn } => {
                    should_idle = true;
                }
                _ => {}
            }
        }

        if should_idle {
            *state = AgentRunState::Idle;
        }
    }
}

fn ensure_assistant_tail(messages: &mut AgentMessages) {
    if !matches!(messages.0.last(), Some(Message::Assistant { .. })) {
        messages.0.push(Message::Assistant { blocks: Vec::new() });
    }
}

fn append_text_delta(messages: &mut AgentMessages, text: &str) {
    let Some(Message::Assistant { blocks }) = messages.0.last_mut() else { return };
    if let Some(AssistantBlock::Text(buf)) = blocks.last_mut() {
        buf.push_str(text);
    } else {
        blocks.push(AssistantBlock::Text(text.to_string()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::echo::synthetic_echo_stream;
    use crate::events::{AgentDelta, AgentToolStatus};
    use crossbeam_channel::unbounded;

    fn make_app() -> App {
        let mut app = App::new();
        app.add_plugins(bevy::tasks::TaskPoolPlugin::default());
        app.add_event::<AgentDelta>();
        app.add_event::<AgentToolStatus>();
        app.add_systems(Update, drain_stream);
        app
    }

    #[test]
    fn echo_stream_appends_text_and_idles() {
        let mut app = make_app();
        let (tx, rx) = unbounded::<StreamEvent>();
        for e in synthetic_echo_stream("hi") {
            tx.send(e).unwrap();
        }
        drop(tx);

        let task = bevy::tasks::IoTaskPool::get().spawn(async {});
        let entity = app.world_mut().spawn((
            AgentMessages::default(),
            AgentRunState::Streaming { rx, _task: task, partial: None },
        )).id();

        app.update();

        let world = app.world();
        let state = world.get::<AgentRunState>(entity).unwrap();
        assert!(matches!(state, AgentRunState::Idle));
        let msgs = world.get::<AgentMessages>(entity).unwrap();
        assert_eq!(msgs.0.len(), 1);
        match &msgs.0[0] {
            Message::Assistant { blocks } => {
                assert_eq!(blocks.len(), 1);
                match &blocks[0] {
                    AssistantBlock::Text(t) => assert_eq!(t, "echo: hi"),
                    _ => panic!("expected text block"),
                }
            }
            _ => panic!("expected assistant message"),
        }
    }
}
```

Add to `crates/vmux_agent/src/lib.rs`:

```rust
pub mod systems {
    pub mod drain_stream;
    pub mod process_input;
}
```

- [ ] **Step 2: Run tests**

```
env -u CEF_PATH cargo test -p vmux_agent systems::drain_stream::tests
```
Expected: 1 test passes.

- [ ] **Step 3: Commit**

```
git add crates/vmux_agent/src/systems/drain_stream.rs crates/vmux_agent/src/lib.rs
git commit -m "feat(vmux_agent): drain_stream system"
```

---

## Task 13: dispatch_tool system

**Files:**
- Create: `crates/vmux_agent/src/systems/dispatch_tool.rs`
- Modify: `crates/vmux_agent/src/lib.rs`

- [ ] **Step 1: Write the system + test**

Create `crates/vmux_agent/src/systems/dispatch_tool.rs`:

```rust
use bevy::prelude::*;
use futures_lite::future;

use crate::components::AgentMessages;
use crate::events::{AgentToolStatus, ToolStatus};
use crate::message::Message;
use crate::run_state::{AgentRunState, ToolDispatchOutput};

pub fn dispatch_tool(
    mut q: Query<(Entity, &mut AgentRunState, &mut AgentMessages)>,
    mut tool_events: EventWriter<AgentToolStatus>,
) {
    for (entity, mut state, mut messages) in &mut q {
        let output_opt = match &mut *state {
            AgentRunState::RunningTool { task, .. } => future::block_on(future::poll_once(task)),
            _ => continue,
        };
        let Some(output) = output_opt else { continue };
        let ToolDispatchOutput { call_id, content, is_error } = output;
        messages.0.push(Message::ToolResult {
            call_id: call_id.clone(),
            content: content.clone(),
            is_error,
        });
        tool_events.write(AgentToolStatus {
            session: entity,
            call_id,
            status: ToolStatus::Result { content, is_error },
        });
        *state = AgentRunState::Idle;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::tasks::IoTaskPool;

    fn make_app() -> App {
        let mut app = App::new();
        app.add_plugins(bevy::tasks::TaskPoolPlugin::default());
        app.add_event::<AgentToolStatus>();
        app.add_systems(Update, dispatch_tool);
        app
    }

    #[test]
    fn completed_tool_appends_result_and_idles() {
        let mut app = make_app();
        let task = IoTaskPool::get().spawn(async {
            ToolDispatchOutput {
                call_id: "abc".into(),
                content: "ok".into(),
                is_error: false,
            }
        });
        let entity = app.world_mut().spawn((
            AgentMessages::default(),
            AgentRunState::RunningTool { call_id: "abc".into(), task },
        )).id();

        for _ in 0..10 {
            app.update();
            if matches!(app.world().get::<AgentRunState>(entity), Some(AgentRunState::Idle)) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }

        let world = app.world();
        let msgs = world.get::<AgentMessages>(entity).unwrap();
        assert_eq!(msgs.0.len(), 1);
        match &msgs.0[0] {
            Message::ToolResult { call_id, content, is_error } => {
                assert_eq!(call_id, "abc");
                assert_eq!(content, "ok");
                assert!(!is_error);
            }
            _ => panic!("expected tool result"),
        }
    }
}
```

Add to `crates/vmux_agent/src/lib.rs`:

```rust
pub mod systems {
    pub mod dispatch_tool;
    pub mod drain_stream;
    pub mod process_input;
}
```

Add `futures-lite = "2"` to `crates/vmux_agent/Cargo.toml` if not present.

- [ ] **Step 2: Run tests**

```
env -u CEF_PATH cargo test -p vmux_agent systems::dispatch_tool::tests
```
Expected: 1 test passes.

- [ ] **Step 3: Commit**

```
git add crates/vmux_agent/src/systems/dispatch_tool.rs crates/vmux_agent/src/lib.rs crates/vmux_agent/Cargo.toml
git commit -m "feat(vmux_agent): dispatch_tool system"
```

---

## Task 14: handle_approval_reply system

**Files:**
- Create: `crates/vmux_agent/src/systems/approval.rs`
- Modify: `crates/vmux_agent/src/lib.rs`

- [ ] **Step 1: Write the system + test**

Create `crates/vmux_agent/src/systems/approval.rs`:

```rust
use bevy::prelude::*;

use crate::components::{AgentApprovalPolicy, AgentMessages};
use crate::events::{AgentApprovalReply, ApprovalDecision};
use crate::message::Message;
use crate::run_state::AgentRunState;

pub fn handle_approval_reply(
    mut events: EventReader<AgentApprovalReply>,
    mut q: Query<(&mut AgentRunState, &mut AgentMessages, &mut AgentApprovalPolicy)>,
) {
    for reply in events.read() {
        let Ok((mut state, mut messages, mut policy)) = q.get_mut(reply.session) else {
            continue;
        };
        let AgentRunState::AwaitingApproval { call_id, name, .. } = &*state else { continue };
        if call_id != &reply.call_id { continue }
        match reply.decision {
            ApprovalDecision::Allow | ApprovalDecision::AllowAlways => {
                if reply.decision == ApprovalDecision::AllowAlways {
                    policy.auto.insert(name.clone());
                }
                *state = AgentRunState::Idle;
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_app() -> App {
        let mut app = App::new();
        app.add_event::<AgentApprovalReply>();
        app.add_systems(Update, handle_approval_reply);
        app
    }

    #[test]
    fn deny_appends_error_result_and_idles() {
        let mut app = make_app();
        let entity = app.world_mut().spawn((
            AgentMessages::default(),
            AgentApprovalPolicy::default(),
            AgentRunState::AwaitingApproval {
                call_id: "abc".into(),
                name: "run_shell".into(),
                args: json!({}),
            },
        )).id();

        app.world_mut().send_event(AgentApprovalReply {
            session: entity,
            call_id: "abc".into(),
            decision: ApprovalDecision::Deny,
        });
        app.update();

        let msgs = app.world().get::<AgentMessages>(entity).unwrap();
        assert_eq!(msgs.0.len(), 1);
        match &msgs.0[0] {
            Message::ToolResult { is_error, .. } => assert!(is_error),
            _ => panic!("expected tool result"),
        }
        assert!(matches!(app.world().get::<AgentRunState>(entity), Some(AgentRunState::Idle)));
    }

    #[test]
    fn allow_always_records_in_policy() {
        let mut app = make_app();
        let entity = app.world_mut().spawn((
            AgentMessages::default(),
            AgentApprovalPolicy::default(),
            AgentRunState::AwaitingApproval {
                call_id: "abc".into(),
                name: "run_shell".into(),
                args: json!({}),
            },
        )).id();

        app.world_mut().send_event(AgentApprovalReply {
            session: entity,
            call_id: "abc".into(),
            decision: ApprovalDecision::AllowAlways,
        });
        app.update();

        let policy = app.world().get::<AgentApprovalPolicy>(entity).unwrap();
        assert!(policy.auto.contains("run_shell"));
    }
}
```

Add to `crates/vmux_agent/src/lib.rs`:

```rust
pub mod systems {
    pub mod approval;
    pub mod dispatch_tool;
    pub mod drain_stream;
    pub mod process_input;
}
```

- [ ] **Step 2: Run tests**

```
env -u CEF_PATH cargo test -p vmux_agent systems::approval::tests
```
Expected: 2 tests pass.

- [ ] **Step 3: Commit**

```
git add crates/vmux_agent/src/systems/approval.rs crates/vmux_agent/src/lib.rs
git commit -m "feat(vmux_agent): handle_approval_reply system"
```

---

## Task 15: GuiAgentPlugin wiring

**Files:**
- Create: `crates/vmux_agent/src/gui_plugin.rs`
- Modify: `crates/vmux_agent/src/lib.rs`
- Modify: `crates/vmux_desktop/src/main.rs` (or wherever the existing `AgentSessionPlugin` is registered) — add `GuiAgentPlugin`

- [ ] **Step 1: Write the plugin**

Create `crates/vmux_agent/src/gui_plugin.rs`:

```rust
use bevy::prelude::*;

use crate::components::{AgentApprovalPolicy, AgentMessages, AgentSession};
use crate::echo::EchoGuiStrategy;
use crate::events::{
    AgentApprovalReply, AgentApprovalRequest, AgentDelta, AgentInput, AgentToolStatus,
};
use crate::strategy::AgentStrategies;
use crate::systems::{approval, dispatch_tool, drain_stream, process_input};

pub struct GuiAgentPlugin;

impl Plugin for GuiAgentPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<AgentSession>()
            .register_type::<AgentMessages>()
            .register_type::<AgentApprovalPolicy>()
            .add_event::<AgentInput>()
            .add_event::<AgentDelta>()
            .add_event::<AgentToolStatus>()
            .add_event::<AgentApprovalRequest>()
            .add_event::<AgentApprovalReply>()
            .add_systems(
                Update,
                (
                    process_input::process_user_input,
                    drain_stream::drain_stream,
                    dispatch_tool::dispatch_tool,
                    approval::handle_approval_reply,
                ),
            );

        if let Some(mut strategies) = app.world_mut().get_resource_mut::<AgentStrategies>() {
            strategies.register(Box::new(EchoGuiStrategy));
        } else {
            app.insert_resource({
                let mut s = AgentStrategies::default();
                s.register(Box::new(EchoGuiStrategy));
                s
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_builds_without_panic() {
        let mut app = App::new();
        app.add_plugins(bevy::tasks::TaskPoolPlugin::default());
        app.add_plugins(GuiAgentPlugin);
        app.update();
    }
}
```

Add to `crates/vmux_agent/src/lib.rs`:

```rust
mod gui_plugin;
pub use gui_plugin::GuiAgentPlugin;
```

Register in `vmux_desktop` next to existing `AgentSessionPlugin`:

```rust
app.add_plugins(vmux_agent::GuiAgentPlugin);
```

- [ ] **Step 2: Run plugin test**

```
env -u CEF_PATH cargo test -p vmux_agent gui_plugin::tests
```
Expected: 1 test passes.

- [ ] **Step 3: Build vmux_desktop**

```
env -u CEF_PATH cargo build -p vmux_desktop
```
Expected: clean build.

- [ ] **Step 4: Commit**

```
git add crates/vmux_agent/src/gui_plugin.rs crates/vmux_agent/src/lib.rs crates/vmux_desktop/src/main.rs
git commit -m "feat(vmux_agent): GuiAgentPlugin wiring"
```

---

## Task 16: URL routing for GUI variant

**Files:**
- Modify: `crates/vmux_agent/src/kind.rs` (or whichever module owns URL parsing post-step-1)

- [ ] **Step 1: Add URL parsing test**

In the appropriate URL-routing test module:

```rust
#[test]
fn agent_url_with_no_cli_segment_resolves_to_gui() {
    let parsed = AgentUrl::parse("vmux://agent/vibe/abc-123").unwrap();
    assert_eq!(parsed.kind, AgentKind::Vibe);
    assert_eq!(parsed.variant, AgentVariant::Gui);
    assert_eq!(parsed.sid, "abc-123");
}

#[test]
fn agent_url_with_cli_segment_resolves_to_cli() {
    let parsed = AgentUrl::parse("vmux://agent/vibe/cli/abc-123").unwrap();
    assert_eq!(parsed.kind, AgentKind::Vibe);
    assert_eq!(parsed.variant, AgentVariant::Cli);
    assert_eq!(parsed.sid, "abc-123");
}
```

- [ ] **Step 2: Run, see it fail**

```
env -u CEF_PATH cargo test -p vmux_agent agent_url
```
Expected: FAIL — `AgentUrl` either doesn't carry `variant` yet or doesn't parse `cli/` segment.

- [ ] **Step 3: Update parser**

In `kind.rs` URL-parsing logic, after splitting the path by `/`:
- segments after `<kind>` are `[<rest>...]`
- if first remaining segment equals `"cli"`, variant = `Cli`, sid = next segment
- else variant = `Gui`, sid = first remaining segment

Add `variant: AgentVariant` field to `AgentUrl` (or equivalent) struct. Update `format_*` helpers to emit nested form: `vmux://agent/<kind>/[cli/]<sid>`.

- [ ] **Step 4: Run tests**

```
env -u CEF_PATH cargo test -p vmux_agent agent_url
```
Expected: PASS.

- [ ] **Step 5: Commit**

```
git add crates/vmux_agent/src/kind.rs
git commit -m "feat(vmux_agent): URL routing recognises GUI vs CLI variant"
```

---

## Task 17: Pane spawn for GUI agent + Dioxus stub page

**Files:**
- Modify: `crates/vmux_desktop/src/...` (URL → entity spawn dispatch — same place that currently spawns CLI agent panes)
- Modify: `crates/vmux_webview_app/src/lib.rs` — add stub page route

- [ ] **Step 1: Add Dioxus stub page**

In `crates/vmux_webview_app/src/lib.rs`, register a route handler for paths matching `agent/<provider>/<sid>` (excluding `agent/<provider>/cli/<sid>`):

```rust
fn agent_gui_page(cx: dioxus::Scope) -> dioxus::Element {
    cx.render(dioxus::rsx! {
        div { class: "agent-gui-stub",
            h1 { "GUI Agent" }
            p { "Stub page — full UI ships in step 4." }
        }
    })
}
```

(Adjust to actual Dioxus 0.7 API used elsewhere in `vmux_webview_app`. Reference an existing page in that file for the right pattern.)

- [ ] **Step 2: Spawn entity on URL open**

In the persistence/URL-dispatch code in `vmux_desktop`, add a branch for `AgentVariant::Gui`:

```rust
AgentVariant::Gui => {
    commands.spawn((
        AgentSession {
            kind: parsed.kind,
            variant: AgentVariant::Gui,
            sid: parsed.sid.clone(),
            provider: provider_str(parsed.kind),
            model: default_model_for(parsed.kind),
        },
        AgentMessages::default(),
        AgentApprovalPolicy::default(),
        AgentRunState::Idle,
    ));
}
```

(`provider_str` and `default_model_for` are small helpers — define inline next to the spawn site.)

- [ ] **Step 3: Build**

```
env -u CEF_PATH cargo build -p vmux_desktop
env -u CEF_PATH cargo build -p vmux_webview_app --target wasm32-unknown-unknown
```
Expected: both succeed.

- [ ] **Step 4: Commit**

```
git add crates/vmux_desktop crates/vmux_webview_app
git commit -m "feat(vmux_desktop): spawn GUI agent pane on URL open + stub Dioxus page"
```

---

## Task 18: Command bar entries per provider

**Files:**
- Modify: `crates/vmux_desktop/src/command_bar.rs`

- [ ] **Step 1: Add three actions**

In the command-bar action enumeration, add:

```rust
NewVibeChat,
NewClaudeChat,
NewCodexChat,
```

Each handler mints a UUIDv4 and opens `vmux://agent/<kind>/<uuid>`:

```rust
fn handle_new_chat(kind: AgentKind, mut events: EventWriter<OpenUrl>) {
    let sid = uuid::Uuid::new_v4().to_string();
    let url = format!("vmux://agent/{}/{}", kind.as_str(), sid);
    events.write(OpenUrl(url));
}
```

(`OpenUrl` and `as_str` follow existing patterns — reference how the current `vibe`/`claude`/`codex` CLI actions are wired.)

- [ ] **Step 2: Update command bar test**

Verify the action list now includes the three new entries:

```rust
#[test]
fn command_bar_includes_new_gui_chat_actions() {
    let actions = command_bar_actions();
    assert!(actions.iter().any(|a| matches!(a, CommandBarAction::NewVibeChat)));
    assert!(actions.iter().any(|a| matches!(a, CommandBarAction::NewClaudeChat)));
    assert!(actions.iter().any(|a| matches!(a, CommandBarAction::NewCodexChat)));
}
```

- [ ] **Step 3: Run test**

```
env -u CEF_PATH cargo test -p vmux_desktop command_bar
```
Expected: PASS.

- [ ] **Step 4: Commit**

```
git add crates/vmux_desktop/src/command_bar.rs
git commit -m "feat(vmux_desktop): command bar New <provider> chat actions"
```

---

## Task 19: End-to-end echo smoke test

**Files:**
- Create: `crates/vmux_agent/tests/echo_smoke.rs`

- [ ] **Step 1: Write the smoke test**

```rust
use bevy::prelude::*;
use vmux_agent::{
    AgentApprovalPolicy, AgentKind, AgentMessages, AgentRunState, AgentSession, AgentVariant,
    GuiAgentPlugin, PendingUserInput,
};
use vmux_agent::message::Message;

fn make_app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins(GuiAgentPlugin);
    app
}

#[test]
fn echo_session_streams_to_assistant_message() {
    let mut app = make_app();
    let entity = app.world_mut().spawn((
        AgentSession {
            kind: AgentKind::Vibe,
            variant: AgentVariant::Gui,
            sid: "smoke".into(),
            provider: "vibe".into(),
            model: "echo-stub".into(),
        },
        AgentMessages::default(),
        AgentApprovalPolicy::default(),
        AgentRunState::Idle,
        PendingUserInput("hello".into()),
    )).id();

    for _ in 0..50 {
        app.update();
        let state = app.world().get::<AgentRunState>(entity).unwrap();
        if matches!(state, AgentRunState::Idle) {
            let msgs = app.world().get::<AgentMessages>(entity).unwrap();
            if msgs.0.len() >= 2 { break; }
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }

    let msgs = app.world().get::<AgentMessages>(entity).unwrap();
    assert_eq!(msgs.0.len(), 2, "expected user + assistant messages");
    matches!(&msgs.0[0], Message::User { text } if text == "hello");
    let assistant_text = match &msgs.0[1] {
        Message::Assistant { blocks } => blocks
            .iter()
            .filter_map(|b| match b {
                vmux_agent::AssistantBlock::Text(t) => Some(t.as_str()),
                _ => None,
            })
            .collect::<String>(),
        _ => panic!("expected assistant message"),
    };
    assert_eq!(assistant_text, "echo: hello");
}
```

- [ ] **Step 2: Run smoke test**

```
env -u CEF_PATH cargo test -p vmux_agent --test echo_smoke
```
Expected: PASS.

- [ ] **Step 3: Run full vmux_agent test suite**

```
env -u CEF_PATH cargo test -p vmux_agent
```
Expected: all pass.

- [ ] **Step 4: Pre-push checks on changed crates**

```
PKGS=$(BASE=origin/main ./scripts/changed-crates.sh)
for pkg in $PKGS; do cargo fmt -p "$pkg" -- --check; done
for pkg in $PKGS; do env -u CEF_PATH cargo clippy -p "$pkg" --all-targets -- -D warnings; done
for pkg in $PKGS; do env -u CEF_PATH cargo test -p "$pkg"; done
```
Expected: all pass. If fmt fails, run `make lint-fix` and re-run.

- [ ] **Step 5: Commit**

```
git add crates/vmux_agent/tests/echo_smoke.rs
git commit -m "test(vmux_agent): end-to-end echo smoke"
```

- [ ] **Step 6: Push branch + open PR**

```
git push -u origin vmx-gui-agent
gh pr create --title "feat: GUI agent skeleton (echo stub)" --body "$(cat <<'EOF'
## Summary
- Adds `AgentVariant::{Gui, Cli}`, splits `AgentStrategy` into core + `CliAgentStrategy` + `GuiAgentStrategy` sub-traits, registry keyed by (kind, variant)
- ECS state machine for GUI sessions: `process_user_input` → `drain_stream` → `dispatch_tool` / `handle_approval_reply`, with `AgentRunState` non-serializable runtime state and durable `AgentSession`/`AgentMessages`/`AgentApprovalPolicy`
- `EchoGuiStrategy` stub registered for `AgentKind::Vibe` to confirm end-to-end plumbing without hitting any provider API
- URL routing recognises `vmux://agent/<kind>/<sid>` as Gui variant; `cli/` segment for existing CLI wrapper
- Three new command bar actions to mint new Gui chats per provider
- Stub Dioxus page for `agent/<provider>/<sid>` route — real chat UI ships in step 4

## Test plan
- [x] `cargo test -p vmux_agent` (unit + integration)
- [x] `cargo test -p vmux_agent --test echo_smoke` (end-to-end)
- [x] Pre-push checks on changed crates (fmt + clippy + test)
- [ ] Manual: open `vmux://agent/vibe/<uuid>` from command bar, confirm pane spawns and stub page renders
EOF
)"
```

---

## Notes for the executing agent

- **Step 1 dependency:** This plan assumes the URL migration in `vmx-claude-codex` has merged. If you encounter compile errors that suggest `AgentKind`/`AgentStrategy` look different than expected, rebase onto post-merge `main` first.
- **Bevy 0.18 `EventWriter`:** uses `.write()` not `.send()` (renamed in 0.16). Verify against existing code.
- **Bevy 0.18 `Task<T>` poll:** `futures_lite::future::poll_once` returns `Option<T>`, not a future you can `.await`.
- **`AgentKind` derives:** Task 7 needs `Serialize + Deserialize + Reflect` on `AgentKind` and `AgentVariant`. Add them in Task 7's commit if missing — reflect derives need `#[derive(Reflect)]` plus the type registered via `app.register_type::<T>()` (done in `GuiAgentPlugin`).
- **Real provider impls (steps 3, 5, 6) replace `EchoGuiStrategy` for each provider.** Don't remove `EchoGuiStrategy` — it stays as the default test fixture for future system-level tests.
