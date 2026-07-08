# Agent context-tree collapse Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task (inline — do NOT subagent-drive; CEF builds are heavy and long-running agents drop sockets). Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Collapse each assistant turn's steps (thinking / tool-use / tool-result / plan / diff) under one disclosure headed by an animated, randomly-cycling working/thinking verb while running, resting to `Worked for Ns · K steps` when done — matching Claude / Codex desktop; prose answers stay inline.

**Architecture:** Brain in Bevy core, dumb frontend. Backend folds the flat `AgentMessages` transcript into `Vec<ChatItem>` (`group_turns`), tracks per-turn wall-clock via `AgentRunState` transitions (`AgentTurnMeta`), and ships it as JSON in `ChatSnapshot`. The page only renders items and runs two cosmetic timers (verb swap, live seconds).

**Tech Stack:** Rust, Bevy ECS, Dioxus/WASM, rkyv+serde bin-ipc, Tailwind.

**Design:** `docs/specs/2026-07-08-agent-context-collapse-design.md`

**Scope note — two unrelated `messages_json`:** we only touch `ChatSnapshot.messages_json` in `crates/vmux_agent/src/chat_page/event.rs` (read only by `snapshot_of` + `page.rs`). The identically-named field on `ServiceMessage::AgentMessagesSnapshot` / `PageAgentSnapshot` (vmux_service, vmux_terminal, client/page/plugin.rs) is the raw service→ECS transcript — **do not touch it**.

**Per-task builds:** native `cargo test -p vmux_agent` stays green after every task (the wasm `page.rs` is `#[cfg(target_arch = "wasm32")]`, excluded from native). The wasm page compiles again only after Task 4; it is checked in Task 5.

---

### Task 1: Wire types — `ChatItem`, `ChatTurn`, `ChatBlock::ToolResult`, `WORKING_VERBS`

**Files:**
- Modify: `crates/vmux_agent/src/chat_page/event.rs`

- [ ] **Step 1: Write the failing test**

Add to the `mod tests` block in `crates/vmux_agent/src/chat_page/event.rs`:

```rust
    #[test]
    fn chat_item_turn_roundtrip() {
        let items = vec![
            ChatItem::User { text: "hi".into() },
            ChatItem::Turn(ChatTurn {
                steps: vec![
                    ChatBlock::Thinking("hmm".into()),
                    ChatBlock::ToolResult { content: "ok".into(), is_error: false },
                ],
                answer: vec![ChatBlock::Text("done".into())],
                running: false,
                duration_secs: Some(12),
                step_count: 2,
            }),
        ];
        let json = serde_json::to_string(&items).unwrap();
        let back: Vec<ChatItem> = serde_json::from_str(&json).unwrap();
        assert_eq!(back.len(), 2);
        let ChatItem::Turn(turn) = &back[1] else { panic!("expected turn") };
        assert_eq!(turn.step_count, 2);
        assert_eq!(turn.duration_secs, Some(12));
        assert_eq!(turn.answer.len(), 1);
        assert!(matches!(turn.steps[1], ChatBlock::ToolResult { is_error: false, .. }));
    }

    #[test]
    fn working_verbs_nonempty() {
        assert!(!WORKING_VERBS.is_empty());
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vmux_agent chat_item_turn_roundtrip`
Expected: FAIL — `cannot find type ChatItem` / `ChatTurn` / `WORKING_VERBS` not found.

- [ ] **Step 3: Add the types**

In `crates/vmux_agent/src/chat_page/event.rs`, add the `ToolResult` variant to `ChatBlock` (after the `Plan` variant, before the closing `}` of the enum), and update the enum doc comment:

```rust
/// The page's block type inside a [`ChatTurn`]. Mirrors `vmux_service::message::AssistantBlock`
/// plus `ToolResult`, which `group_turns` folds in from the top-level tool-result message.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum ChatBlock {
    Text(String),
    Thinking(String),
    ToolUse {
        call_id: String,
        name: String,
        args: String,
    },
    Diff {
        call_id: String,
        path: String,
        old_text: Option<String>,
        new_text: String,
    },
    Plan {
        steps: Vec<ChatPlanStep>,
    },
    ToolResult {
        content: String,
        is_error: bool,
    },
}
```

Then add, immediately after the `ChatPlanStep` struct:

```rust
/// A rendered conversation entry: a user bubble or a grouped assistant turn. Built backend by
/// `group_turns`, carried as JSON in [`ChatSnapshot::messages_json`].
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum ChatItem {
    User { text: String },
    Turn(ChatTurn),
}

/// One assistant turn: its collapsed step tree, its inline prose answer, and run-state.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct ChatTurn {
    /// Thinking / tool-use / tool-result / plan / diff — the collapsible "context tree".
    pub steps: Vec<ChatBlock>,
    /// Assistant prose — always rendered inline, never hidden.
    pub answer: Vec<ChatBlock>,
    /// True only for the live (tail) turn while the run is active.
    pub running: bool,
    /// Final wall-clock seconds for a turn that finished this process; `None` otherwise.
    pub duration_secs: Option<u32>,
    /// `steps.len()`, sent explicitly for the header label.
    pub step_count: u32,
}

/// The curated verbs the running-turn header cycles through (owned by the shared contract, not
/// the view). The page picks one at random every few seconds while streaming.
pub const WORKING_VERBS: &[&str] = &[
    "Working", "Thinking", "Pondering", "Noodling", "Percolating", "Conjuring", "Cooking",
    "Brewing", "Musing", "Ruminating", "Scheming", "Synthesizing", "Tinkering", "Churning",
    "Vibing", "Simmering", "Crafting", "Divining", "Mulling", "Spelunking",
];
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p vmux_agent --lib chat_page::event`
Expected: PASS (all event tests, including the two new ones).

- [ ] **Step 5: Commit**

```bash
cd .worktrees/context-collapse
git add crates/vmux_agent/src/chat_page/event.rs
git commit -m "feat(agent): add ChatItem/ChatTurn wire types + WORKING_VERBS"
```

---

### Task 2: Per-turn duration tracking (`AgentTurnMeta` + system)

**Files:**
- Modify: `crates/vmux_agent/src/run_state.rs`
- Modify: `crates/vmux_agent/src/chat_page.rs`

- [ ] **Step 1: Write the failing test**

Add to `crates/vmux_agent/src/chat_page.rs` at the end of the file:

```rust
#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use crate::run_state::{AgentRunState, AgentTurnMeta};
    use bevy::prelude::*;

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(Update, track_turn_duration);
        app
    }

    #[test]
    fn streaming_then_idle_records_one_duration() {
        let mut app = app();
        let e = app.world_mut().spawn(AgentRunState::Streaming).id();
        app.update();
        assert!(app.world().get::<AgentTurnMeta>(e).unwrap().turn_start.is_some());
        *app.world_mut().get_mut::<AgentRunState>(e).unwrap() = AgentRunState::Idle;
        app.update();
        let meta = app.world().get::<AgentTurnMeta>(e).unwrap();
        assert_eq!(meta.durations.len(), 1);
        assert!(meta.turn_start.is_none());
    }

    #[test]
    fn awaiting_approval_does_not_finalize() {
        let mut app = app();
        let e = app.world_mut().spawn(AgentRunState::Streaming).id();
        app.update();
        *app.world_mut().get_mut::<AgentRunState>(e).unwrap() = AgentRunState::AwaitingApproval {
            call_id: "c".into(),
            name: "n".into(),
            args: serde_json::Value::Null,
        };
        app.update();
        let meta = app.world().get::<AgentTurnMeta>(e).unwrap();
        assert!(meta.durations.is_empty());
        assert!(meta.turn_start.is_some());
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vmux_agent streaming_then_idle_records_one_duration`
Expected: FAIL — `AgentTurnMeta` not found / `track_turn_duration` not found.

- [ ] **Step 3: Add `AgentTurnMeta` + `#[require]`**

In `crates/vmux_agent/src/run_state.rs`, add `use std::time::Duration;` under the existing `use bevy::prelude::*;`, add the `#[require(AgentTurnMeta)]` attribute to `AgentRunState`, and add the component:

```rust
use bevy::prelude::*;
use std::time::Duration;

#[derive(Component, Default)]
#[require(AgentTurnMeta)]
pub enum AgentRunState {
    #[default]
    Idle,
    /// Downloading/installing the agent's runtime or package before first spawn.
    Installing {
        pct: Option<u8>,
        message: String,
    },
    Streaming,
    AwaitingApproval {
        call_id: String,
        name: String,
        args: serde_json::Value,
    },
    Errored(String),
}

/// Per-session record of finished turn wall-clock, for the chat page's resting
/// `Worked for Ns` header. Runtime-only. `turn_start` is `Time::elapsed()` at the current
/// turn's first `Streaming`; each `Streaming → Idle/Errored` pushes one entry to `durations`.
#[derive(Component, Default)]
pub struct AgentTurnMeta {
    pub durations: Vec<u32>,
    pub turn_start: Option<Duration>,
}
```

- [ ] **Step 4: Add the tracking system + register it**

In `crates/vmux_agent/src/chat_page.rs`, extend the run_state import (line ~27) and register the system. Change:

```rust
use crate::run_state::AgentRunState;
```
to:
```rust
use crate::run_state::{AgentRunState, AgentTurnMeta};
```

In `AgentChatPagePlugin::build`, change the `add_systems` line:
```rust
            .add_systems(Update, (push_chat_to_page, push_chat_on_ready));
```
to:
```rust
            .add_systems(
                Update,
                (
                    (track_turn_duration, push_chat_to_page).chain(),
                    push_chat_on_ready,
                ),
            );
```

Add the system (place it just above `snapshot_of`):

```rust
/// Record per-turn wall-clock from `AgentRunState` edges (covers page + ACP mutation sites
/// uniformly). Idempotent: the `turn_start` guard tolerates repeated same-state sets and does
/// not reset across a mid-turn `AwaitingApproval`.
#[cfg(not(target_arch = "wasm32"))]
fn track_turn_duration(
    time: Res<Time>,
    mut sessions: Query<(&AgentRunState, &mut AgentTurnMeta), Changed<AgentRunState>>,
) {
    for (state, mut meta) in &mut sessions {
        match state {
            AgentRunState::Streaming => {
                if meta.turn_start.is_none() {
                    meta.turn_start = Some(time.elapsed());
                }
            }
            AgentRunState::Idle | AgentRunState::Errored(_) => {
                if let Some(start) = meta.turn_start.take() {
                    meta.durations.push(time.elapsed().saturating_sub(start).as_secs() as u32);
                }
            }
            AgentRunState::AwaitingApproval { .. } | AgentRunState::Installing { .. } => {}
        }
    }
}
```

Note: `AgentTurnMeta.durations` is written but not yet read until Task 3 — a transient "field never read" warning here is expected and clears in Task 3. Tests still pass.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p vmux_agent --lib chat_page`
Expected: PASS (both duration tests).

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_agent/src/run_state.rs crates/vmux_agent/src/chat_page.rs
git commit -m "feat(agent): track per-turn duration via AgentRunState edges"
```

---

### Task 3: Grouping (`group_turns`) + snapshot rewrite

**Files:**
- Create: `crates/vmux_agent/src/chat_page/turns.rs`
- Modify: `crates/vmux_agent/src/chat_page.rs`
- Modify: `crates/vmux_agent/src/chat_page/event.rs` (field rename + rkyv test)

- [ ] **Step 1: Write the failing test**

Create `crates/vmux_agent/src/chat_page/turns.rs` with the tests first (implementation added Step 3):

```rust
//! Folds a flat agent transcript (`vmux_service::message::Message`) into rendered `ChatItem`s:
//! user bubbles and grouped assistant turns. Pure + unit-tested — the brain for the dumb chat
//! page (see the context-collapse design).

use crate::chat_page::event::{ChatBlock, ChatItem, ChatPlanStep, ChatTurn};
use vmux_service::message::{AssistantBlock, Message, PlanStep};

#[cfg(test)]
mod tests {
    use super::*;

    fn assistant(blocks: Vec<AssistantBlock>) -> Message {
        Message::Assistant { blocks }
    }
    fn tool(id: &str) -> AssistantBlock {
        AssistantBlock::ToolUse { call_id: id.into(), name: "run".into(), args: "{}".into() }
    }

    #[test]
    fn splits_steps_and_answer_folds_tool_result() {
        let msgs = vec![
            Message::User { text: "hi".into() },
            assistant(vec![AssistantBlock::Thinking("t".into()), tool("c1")]),
            Message::ToolResult { call_id: "c1".into(), content: "ok".into(), is_error: false },
            assistant(vec![AssistantBlock::Text("done".into())]),
        ];
        let items = group_turns(&msgs, &[], false);
        assert_eq!(items.len(), 2);
        assert!(matches!(&items[0], ChatItem::User { text } if text == "hi"));
        let ChatItem::Turn(t) = &items[1] else { panic!() };
        assert_eq!(t.step_count, 3);
        assert_eq!(t.steps.len(), 3);
        assert!(matches!(t.steps[2], ChatBlock::ToolResult { .. }));
        assert_eq!(t.answer.len(), 1);
        assert!(!t.running);
    }

    #[test]
    fn one_turn_per_user_durations_by_ordinal() {
        let msgs = vec![
            Message::User { text: "a".into() },
            assistant(vec![AssistantBlock::Text("1".into())]),
            Message::User { text: "b".into() },
            assistant(vec![AssistantBlock::Text("2".into())]),
        ];
        let items = group_turns(&msgs, &[5, 9], false);
        assert_eq!(items.len(), 4);
        let ChatItem::Turn(t0) = &items[1] else { panic!() };
        let ChatItem::Turn(t1) = &items[3] else { panic!() };
        assert_eq!(t0.duration_secs, Some(5));
        assert_eq!(t1.duration_secs, Some(9));
    }

    #[test]
    fn missing_duration_is_none() {
        let msgs = vec![
            Message::User { text: "a".into() },
            assistant(vec![AssistantBlock::Text("1".into())]),
            Message::User { text: "b".into() },
            assistant(vec![AssistantBlock::Text("2".into())]),
        ];
        let items = group_turns(&msgs, &[5], false);
        let ChatItem::Turn(t1) = &items[3] else { panic!() };
        assert_eq!(t1.duration_secs, None);
    }

    #[test]
    fn running_marks_and_nulls_last_turn() {
        let msgs = vec![
            Message::User { text: "a".into() },
            assistant(vec![AssistantBlock::Text("1".into())]),
        ];
        let items = group_turns(&msgs, &[5], true);
        let ChatItem::Turn(t) = &items[1] else { panic!() };
        assert!(t.running);
        assert_eq!(t.duration_secs, None);
    }

    #[test]
    fn running_emits_empty_tail_turn_after_user() {
        let msgs = vec![Message::User { text: "a".into() }];
        let items = group_turns(&msgs, &[], true);
        assert_eq!(items.len(), 2);
        let ChatItem::Turn(t) = &items[1] else { panic!() };
        assert!(t.running);
        assert_eq!(t.step_count, 0);
        assert!(t.answer.is_empty());
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vmux_agent --lib chat_page::turns`
Expected: FAIL — `group_turns` not found (and the module is not declared yet; if `cargo` reports the file is not part of the crate, that resolves in Step 4).

- [ ] **Step 3: Implement `group_turns`**

Prepend the implementation to `crates/vmux_agent/src/chat_page/turns.rs` (above the `#[cfg(test)] mod tests`):

```rust
/// Group `messages` into `ChatItem`s: one `ChatItem::User` per user message, followed by one
/// `ChatItem::Turn` per started turn. `durations[i]` is the finished seconds of the `i`-th
/// emitted turn (by ordinal); out-of-range → `None`. When `running`, the last turn is marked
/// live and forced to `duration_secs = None`.
pub fn group_turns(messages: &[Message], durations: &[u32], running: bool) -> Vec<ChatItem> {
    let mut items: Vec<ChatItem> = Vec::new();
    let mut current: Option<ChatTurn> = None;
    let mut ordinal: usize = 0;

    for msg in messages {
        match msg {
            Message::User { text } => {
                flush(&mut items, &mut current, &mut ordinal, durations);
                items.push(ChatItem::User { text: text.clone() });
                current = Some(ChatTurn::default());
            }
            Message::Assistant { blocks } => {
                let turn = current.get_or_insert_with(ChatTurn::default);
                for block in blocks {
                    match block {
                        AssistantBlock::Text(t) => turn.answer.push(ChatBlock::Text(t.clone())),
                        AssistantBlock::Thinking(t) => {
                            turn.steps.push(ChatBlock::Thinking(t.clone()))
                        }
                        AssistantBlock::ToolUse { call_id, name, args } => {
                            turn.steps.push(ChatBlock::ToolUse {
                                call_id: call_id.clone(),
                                name: name.clone(),
                                args: args.clone(),
                            })
                        }
                        AssistantBlock::Diff { call_id, path, old_text, new_text } => {
                            turn.steps.push(ChatBlock::Diff {
                                call_id: call_id.clone(),
                                path: path.clone(),
                                old_text: old_text.clone(),
                                new_text: new_text.clone(),
                            })
                        }
                        AssistantBlock::Plan { steps } => turn.steps.push(ChatBlock::Plan {
                            steps: steps.iter().map(map_plan_step).collect(),
                        }),
                    }
                }
            }
            Message::ToolResult { content, is_error, .. } => {
                let turn = current.get_or_insert_with(ChatTurn::default);
                turn.steps.push(ChatBlock::ToolResult {
                    content: content.clone(),
                    is_error: *is_error,
                });
            }
        }
    }
    flush(&mut items, &mut current, &mut ordinal, durations);

    if running {
        if let Some(ChatItem::Turn(last)) = items.last_mut() {
            last.running = true;
            last.duration_secs = None;
        }
    }
    items
}

fn flush(
    items: &mut Vec<ChatItem>,
    current: &mut Option<ChatTurn>,
    ordinal: &mut usize,
    durations: &[u32],
) {
    if let Some(mut turn) = current.take() {
        turn.step_count = turn.steps.len() as u32;
        turn.duration_secs = durations.get(*ordinal).copied();
        *ordinal += 1;
        items.push(ChatItem::Turn(turn));
    }
}

fn map_plan_step(step: &PlanStep) -> ChatPlanStep {
    ChatPlanStep { content: step.content.clone(), status: step.status.clone() }
}
```

- [ ] **Step 4: Declare the module + wire `snapshot_of`**

In `crates/vmux_agent/src/chat_page.rs`, add the module declaration under the existing `pub mod event;` (line ~5):

```rust
#[cfg(not(target_arch = "wasm32"))]
mod turns;
```

Add its import next to the other `#[cfg(not(target_arch = "wasm32"))] use` lines:

```rust
#[cfg(not(target_arch = "wasm32"))]
use crate::chat_page::turns::group_turns;
```

Rewrite `snapshot_of` to take `AgentTurnMeta` and build items. Replace the whole `fn snapshot_of(...) -> ChatSnapshot { ... }` with:

```rust
#[cfg(not(target_arch = "wasm32"))]
fn snapshot_of(
    messages: &AgentMessages,
    state: &AgentRunState,
    turn_meta: Option<&AgentTurnMeta>,
    profile: Option<&Profile>,
    meta: Option<&PageMetadata>,
    queue: &PromptQueue,
) -> ChatSnapshot {
    let durations: &[u32] = turn_meta.map(|m| m.durations.as_slice()).unwrap_or(&[]);
    let running = matches!(state, AgentRunState::Streaming);
    let items = group_turns(&messages.0, durations, running);
    let messages_json = serde_json::to_string(&items).unwrap_or_else(|_| "[]".to_string());
    let (status, error, call_id, name) = match state {
        AgentRunState::Idle => ("idle", String::new(), String::new(), String::new()),
        AgentRunState::Installing { pct, message } => {
            let text = match pct {
                Some(p) => format!("{message} ({p}%)"),
                None => message.clone(),
            };
            ("installing", text, String::new(), String::new())
        }
        AgentRunState::Streaming => ("streaming", String::new(), String::new(), String::new()),
        AgentRunState::AwaitingApproval { call_id, name, .. } => {
            ("awaiting", String::new(), call_id.clone(), name.clone())
        }
        AgentRunState::Errored(message) => {
            ("errored", message.clone(), String::new(), String::new())
        }
    };
    let (agent_name, accent_color) = profile
        .map(|p| (p.name.clone(), p.avatar.color.clone()))
        .unwrap_or_default();
    let agent_icon = meta
        .map(|m| m.icon.favicon_url().to_string())
        .unwrap_or_default();
    ChatSnapshot {
        messages_json,
        status: status.to_string(),
        error,
        approval_call_id: call_id,
        approval_name: name,
        agent_name,
        agent_icon,
        accent_color,
        queued: queue.items.iter().cloned().collect(),
        paused: queue.paused,
    }
}
```

Update `push_chat_on_ready`: add `Option<&AgentTurnMeta>` to the `sessions` query tuple and thread it. Change the query type to:

```rust
    sessions: Query<(
        &AgentMessages,
        &AgentRunState,
        Option<&AgentTurnMeta>,
        Option<&Profile>,
        Option<&PageMetadata>,
        &PromptQueue,
    )>,
```
its destructure to:
```rust
        let Ok((messages, state, turn_meta, profile, meta, queue)) = sessions.get(parent.parent())
        else {
            continue;
        };
```
and its `snapshot_of(...)` call to:
```rust
            &snapshot_of(messages, state, turn_meta, profile, meta, queue),
```

Update `push_chat_to_page`: add `Option<&AgentTurnMeta>` to the query tuple and `Changed<AgentTurnMeta>` to the `Or<>` filter. Change the query to:

```rust
    sessions: Query<
        (
            Entity,
            &AgentMessages,
            &AgentRunState,
            Option<&AgentTurnMeta>,
            Option<&Profile>,
            Option<&PageMetadata>,
            &PromptQueue,
        ),
        Or<(
            Changed<AgentMessages>,
            Changed<AgentRunState>,
            Changed<AgentTurnMeta>,
            Changed<PromptQueue>,
        )>,
    >,
```
its loop header to:
```rust
    for (stack, messages, state, turn_meta, profile, meta, queue) in &sessions {
```
and its `snapshot_of(...)` call to:
```rust
            &snapshot_of(messages, state, turn_meta, profile, meta, queue),
```

- [ ] **Step 5: Rename the wire field + fix its rkyv test**

In `crates/vmux_agent/src/chat_page/event.rs`, update the `ChatSnapshot` field doc + name:

```rust
    /// `serde_json` of `Vec<ChatItem>` (user bubbles + grouped assistant turns).
    pub messages_json: String,
```

Wait — keep the field **named** `messages_json` (renaming the rkyv field is churn for zero gain and the name is already generic). Only the doc comment changes (above). No code in `snapshot_of` or `page.rs` needs a field rename. Leave the existing `chat_snapshot_rkyv_roundtrip` test as-is.

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test -p vmux_agent --lib chat_page`
Expected: PASS (turns tests + duration tests + event tests). The Task 2 "field never read" warning is gone (durations now read by `snapshot_of`).

- [ ] **Step 7: Commit**

```bash
git add crates/vmux_agent/src/chat_page.rs crates/vmux_agent/src/chat_page/turns.rs crates/vmux_agent/src/chat_page/event.rs
git commit -m "feat(agent): group transcript into turns for the chat snapshot"
```

---

### Task 4: Frontend — render turns, disclosure, verb cycling

**Files:**
- Modify: `crates/vmux_agent/Cargo.toml` (add `js-sys`)
- Modify: `crates/vmux_agent/src/chat_page/page.rs`
- Modify: `crates/vmux_agent/src/chat_page/event.rs` (remove dead `ChatMessage` + its test)

This task has no native unit test (wasm/UI); it is verified by `cargo check --target wasm32-unknown-unknown` here and by the runtime pass in Task 5.

- [ ] **Step 1: Add the `js-sys` wasm dependency**

In `crates/vmux_agent/Cargo.toml`, under `[target.'cfg(target_arch = "wasm32")'.dependencies]`, add:

```toml
js-sys = "0.3"
```

- [ ] **Step 2: Update imports + signals + listener**

In `crates/vmux_agent/src/chat_page/page.rs`, replace the event import (lines 3-6):

```rust
use crate::chat_page::event::{
    CHAT_SNAPSHOT_EVENT, ChatApproval, ChatBlock, ChatCancel, ChatClearQueue, ChatItem, ChatResume,
    ChatSnapshot, ChatSubmit, ChatTurn, WORKING_VERBS,
};
```

Replace the `messages` signal declaration (line 32):
```rust
    let mut items = use_signal(Vec::<ChatItem>::new);
```

Add a `verb` signal right after `let mut paused = use_signal(|| false);` (line 44):
```rust
    let mut verb = use_signal(|| "Working".to_string());
```

Add the verb-cycling future right after the existing elapsed `use_future` block (after line 55):
```rust
    use_future(move || async move {
        loop {
            gloo_timers::future::TimeoutFuture::new(2500).await;
            if status() == "streaming" {
                let n = WORKING_VERBS.len();
                let idx = ((js_sys::Math::random() * n as f64) as usize).min(n - 1);
                verb.set(WORKING_VERBS[idx].to_string());
            }
        }
    });
```

In the scroll-pin `use_effect` (line 60), change `let _ = messages.read().len();` to:
```rust
        let _ = items.read().len();
```

In the snapshot listener (lines 73-76), change the parse to `Vec<ChatItem>`:
```rust
    let _listener = use_bin_event_listener::<ChatSnapshot, _>(CHAT_SNAPSHOT_EVENT, move |snap| {
        if let Ok(parsed) = serde_json::from_str::<Vec<ChatItem>>(&snap.messages_json) {
            items.set(parsed);
        }
```
(leave the rest of the listener body unchanged.)

- [ ] **Step 3: Update the transcript render region**

In the `div { class: "mx-auto flex max-w-3xl flex-col gap-4", ... }` block (lines 160-206): change the empty-state guard, the render loop, and **delete** the `if status() == "streaming"` bottom spinner (lines 173-183). Replace lines 161-183 with:

```rust
                    if items.read().is_empty() && status() == "idle" {
                        div { class: "flex flex-col items-center gap-3 py-24 text-center",
                            {avatar_node(&agent_icon(), &accent(), &agent, &header_name, "h-14 w-14 text-xl")}
                            h2 { class: "bg-gradient-to-b from-foreground to-foreground/50 bg-clip-text text-3xl font-semibold capitalize tracking-tight text-transparent",
                                "{header_name}"
                            }
                            p { class: "text-sm text-muted-foreground", "Ready when you are." }
                        }
                    }
                    for (i , item) in items.read().iter().enumerate() {
                        {render_item(i, item, &verb(), elapsed())}
                    }
```

Leave the `if status() == "installing"`, `if status() == "errored"`, and `if paused()` blocks (lines 184-205) unchanged.

- [ ] **Step 4: Replace `render_message` with turn renderers**

In `crates/vmux_agent/src/chat_page/page.rs`, replace the entire `fn render_message(...) { ... }` (lines 379-417) with:

```rust
fn render_item(key: usize, item: &ChatItem, verb: &str, elapsed: u32) -> Element {
    match item {
        ChatItem::User { text } => rsx! {
            div {
                key: "{key}",
                class: "max-w-[80%] self-end whitespace-pre-wrap rounded-2xl bg-foreground/[0.08] px-4 py-2.5 text-sm",
                "{text}"
            }
        },
        ChatItem::Turn(turn) => render_turn(key, turn, verb, elapsed),
    }
}

fn render_turn(key: usize, turn: &ChatTurn, verb: &str, elapsed: u32) -> Element {
    let show_header = turn.step_count > 0 || (turn.running && turn.answer.is_empty());
    rsx! {
        div { key: "{key}", class: "flex max-w-[85%] flex-col gap-2 self-start",
            if show_header {
                {render_turn_header(turn, verb, elapsed)}
            }
            for (j , block) in turn.answer.iter().enumerate() {
                {render_block(j, block)}
            }
        }
    }
}

fn render_turn_header(turn: &ChatTurn, verb: &str, elapsed: u32) -> Element {
    if turn.running {
        rsx! {
            details { class: "group rounded-xl bg-foreground/[0.04] px-3 py-2 ring-1 ring-inset ring-foreground/10",
                summary { class: "flex cursor-pointer select-none items-center gap-2.5 text-sm list-none [&::-webkit-details-marker]:hidden",
                    span { class: "text-[10px] text-muted-foreground transition group-open:rotate-90", "▸" }
                    span { class: "flex items-end gap-1",
                        span { class: "h-1.5 w-1.5 animate-bounce rounded-full bg-foreground/70 [animation-delay:-0.32s]" }
                        span { class: "h-1.5 w-1.5 animate-bounce rounded-full bg-foreground/70 [animation-delay:-0.16s]" }
                        span { class: "h-1.5 w-1.5 animate-bounce rounded-full bg-foreground/70" }
                    }
                    span { class: "animate-pulse bg-gradient-to-r from-foreground/45 via-foreground to-foreground/45 bg-clip-text font-medium text-transparent", "{verb}…" }
                    span { class: "tabular-nums text-xs text-muted-foreground", "{fmt_elapsed(elapsed)}" }
                }
                {render_steps(turn)}
            }
        }
    } else {
        let label = match turn.duration_secs {
            Some(secs) => format!("Worked for {} · {} steps", fmt_elapsed(secs), turn.step_count),
            None => format!("{} steps", turn.step_count),
        };
        rsx! {
            details { class: "group rounded-xl bg-foreground/[0.04] px-3 py-2 ring-1 ring-inset ring-foreground/10",
                summary { class: "flex cursor-pointer select-none items-center gap-2 text-xs text-muted-foreground list-none [&::-webkit-details-marker]:hidden",
                    span { class: "text-[10px] transition group-open:rotate-90", "▸" }
                    span { class: "font-medium", "{label}" }
                }
                {render_steps(turn)}
            }
        }
    }
}

fn render_steps(turn: &ChatTurn) -> Element {
    rsx! {
        div { class: "mt-2 flex flex-col gap-2",
            for (j , block) in turn.steps.iter().enumerate() {
                {render_block(j, block)}
            }
        }
    }
}
```

- [ ] **Step 5: Add the `ToolResult` arm to `render_block`**

In `fn render_block`, add this arm after the `ChatBlock::Diff { .. } => { ... }` arm (before the closing `}` of the `match`):

```rust
        ChatBlock::ToolResult { content, is_error } => {
            let tone = if *is_error { "text-red-500" } else { "text-muted-foreground" };
            let label = if *is_error { "Error" } else { "Output" };
            rsx! {
                details {
                    key: "{key}",
                    class: "group rounded-xl bg-foreground/[0.05] px-3 py-2 ring-1 ring-inset ring-foreground/10",
                    summary { class: "flex cursor-pointer select-none items-center gap-2 text-xs {tone} list-none [&::-webkit-details-marker]:hidden",
                        span { class: "text-[10px] transition group-open:rotate-90", "▸" }
                        span { "{label}" }
                    }
                    pre { class: "mt-1.5 max-h-72 overflow-auto whitespace-pre-wrap font-mono text-[11px] text-muted-foreground", "{content}" }
                }
            }
        }
```

- [ ] **Step 6: Remove the now-dead `ChatMessage` type + test**

In `crates/vmux_agent/src/chat_page/event.rs`, delete the `ChatMessage` enum (lines ~112-127, including its doc comment) and delete the `chat_message_mirror_matches_service_message_json` test.

- [ ] **Step 7: Verify the wasm page compiles**

Run: `cargo check --target wasm32-unknown-unknown -p vmux_agent`
Expected: compiles (no errors). If `error: target may not be installed`, run `rustup target add wasm32-unknown-unknown` first.

- [ ] **Step 8: Commit**

```bash
git add crates/vmux_agent/Cargo.toml crates/vmux_agent/src/chat_page/page.rs crates/vmux_agent/src/chat_page/event.rs
git commit -m "feat(agent): collapse turn steps under animated cycling-verb header"
```

---

### Task 5: Verify, format, and open the PR

**Files:** none (verification + PR).

- [ ] **Step 1: Format (crates only, restore vendored patches)**

Run:
```bash
cargo fmt
git checkout -- patches/ 2>/dev/null || true
git diff --stat
```
Expected: only `crates/vmux_agent/**` reformatted. Stage + amend into the last commit if fmt changed anything:
```bash
git add crates/ && git commit --amend --no-edit
```

- [ ] **Step 2: Native tests + clippy**

Run:
```bash
cargo test -p vmux_agent
cargo clippy -p vmux_agent --all-targets -- -D warnings
```
Expected: tests PASS; clippy clean (no warnings).

- [ ] **Step 3: wasm typecheck**

Run: `cargo check --target wasm32-unknown-unknown -p vmux_agent`
Expected: compiles.

- [ ] **Step 4: Runtime pass (user-driven)**

Ask the user to run the app and open an agent (native-chat or ACP). Verify:
- On submit, an animated header appears with a verb that changes every ~2.5 s and a live seconds counter; the step tree is collapsed under it.
- The prose answer streams inline below the header.
- On turn end, the header rests to `Worked for Ns · K steps`; expanding it shows the thinking/tool/plan/diff tree.
- A pure-text answer (no tools) shows no leftover header.
- Read `~/Library/Application Support/Vmux/dev/logs/vmux-dev.<date>.log` for panics if anything misrenders.

- [ ] **Step 5: Push + open PR**

```bash
git push -u origin feat/agent-context-collapse
gh pr create --title "feat(agent): collapse turn context under an animated working header" --body "<summary + design link>"
```
Return the PR URL.

- [ ] **Step 6: Delete this plan file** (per AGENTS.md — plans are deleted once implemented) and commit that deletion on the branch.

---

## Self-Review

**Spec coverage:**
- Behavior table (running / done / zero-step) → Task 4 `render_turn` + `render_turn_header` (`show_header`, running vs resting).
- `ChatItem`/`ChatTurn`/`ChatBlock::ToolResult`/`WORKING_VERBS` wire model → Task 1.
- `group_turns` (split, fold tool-result, one-per-user, duration-by-ordinal, running tail) → Task 3.
- Per-turn duration via `AgentRunState` edges, `AwaitingApproval` no-reset → Task 2.
- Brain→dumb push (`snapshot_of` + both push systems + `Changed<AgentTurnMeta>` + ordering) → Tasks 2-3.
- Random verb cycle from core list, live seconds, delete old spinner → Task 4.
- Resume tolerance (missing durations → `None`) → Task 3 (`missing_duration_is_none`).
- Tests native-only, wasm checked → Tasks 1-3 + Task 5.

**Placeholder scan:** none — every code step carries full code; the `<summary + design link>` in Task 5 Step 5 is PR prose, filled at PR time.

**Type consistency:** `group_turns(&[Message], &[u32], bool) -> Vec<ChatItem>` used identically in Task 3 tests + `snapshot_of`. `AgentTurnMeta { durations, turn_start }` consistent across Tasks 2-3. `snapshot_of(messages, state, turn_meta, profile, meta, queue)` arg order identical in both call sites. `render_item(key, item, verb, elapsed)` / `render_turn` / `render_turn_header` / `render_steps` signatures consistent. `ChatSnapshot.messages_json` kept (not renamed) — no dangling references.
