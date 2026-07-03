# Agent Chat: Prompt Queue + Cancel — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. This repo builds CEF (huge); implement **inline with a warm target dir**, do NOT subagent-drive. Per project convention, defer manual/runtime testing to ONE pass at the end (Task 10).

**Goal:** Give the `vmux://agent` chat page a FIFO prompt queue and an interrupt flow (Esc / Ctrl+C / Stop button) that cancels the current turn without killing the session — parity with Claude Code / Codex CLI.

**Architecture:** Host-side ECS owns the queue + pause flag; the Dioxus page stays dumb (renders snapshot, emits intents). Cancel plumbs one new `ClientMessage` to the daemon → ACP `session/cancel`. Queue gating on `Idle` also fixes the current concurrent-`PromptRequest` bug.

**Tech Stack:** Rust, Bevy (ECS, messages, observers), Dioxus/WASM (page), rkyv bin-IPC, `agent_client_protocol` (ACP), Tailwind.

**Spec:** `docs/specs/2026-07-03-agent-chat-queue-cancel-design.md`

---

## File map

| File | Change |
|------|--------|
| `crates/vmux_service/src/protocol.rs` | + `AgentRunStatus::Interrupted`, + `ClientMessage::AgentCancel { sid }`, tests |
| `crates/vmux_service/src/server.rs` | route `AgentCancel` → both managers |
| `crates/vmux_service/src/acp/driver.rs` | + `AcpInput::Cancel`, `cancel_requested` flag, `status_after_prompt` helper, cancel arm |
| `crates/vmux_service/src/acp.rs` | init `cancel_requested` in `AcpShared` |
| `crates/vmux_service/src/agent.rs` | + `SessionInput::Cancel`, `Decision` enum, select! streaming, SSE abort handle |
| `crates/vmux_agent/src/components.rs` | + `PromptQueue { items, paused }`, replace `PendingUserInput` |
| `crates/vmux_agent/src/client/acp.rs` | gate `send_acp_input` on queue+state; optimistic `Streaming` |
| `crates/vmux_agent/src/client/page/plugin.rs` | gate `send_page_agent_input`; `ensure_prompt_queue`; `Interrupted` arm sets `paused`; register queue |
| `crates/vmux_agent/src/chat_page/event.rs` | + `ChatCancel`/`ChatResume`/`ChatClearQueue`, + `ChatSnapshot.queued`/`.paused` |
| `crates/vmux_agent/src/chat_page.rs` | register intents; `on_chat_submit`→queue; `on_chat_cancel`/`resume`/`clear`; `snapshot_of` queued/paused |
| `crates/vmux_agent/src/chat_page/page.rs` | signals, queued chips, Stop/Resume/Clear, Esc/Ctrl+C handlers, `do_submit` |

**Shared decision (used by many tasks):** the drainer dispatches a prompt only when the session is `Idle && !paused && !items.is_empty()`, and sets `AgentRunState::Streaming` optimistically on dispatch so the next tick holds until the daemon confirms. `paused` is set true when a wire `AgentRunStatus::Interrupted` arrives, and cleared by resume / clear / a fresh submit.

---

## Task 1: Wire protocol — `Interrupted` status + `AgentCancel` message

**Files:**
- Modify: `crates/vmux_service/src/protocol.rs:257-262` (AgentRunStatus), `:422-425` (ClientMessage, add after `AgentInput`), `:691` (tests)

- [ ] **Step 1: Add the `Interrupted` variant.** Replace the enum at `protocol.rs:257-262`:

```rust
#[derive(Debug, Clone, PartialEq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum AgentRunStatus {
    Streaming,
    Idle,
    /// The user interrupted the in-flight turn (Esc / Ctrl+C / Stop). Distinct from `Idle`
    /// so the UI can mark the stopped turn and pause the queue instead of auto-advancing.
    Interrupted,
    Errored(String),
}
```

- [ ] **Step 2: Add the `AgentCancel` message.** Insert into `ClientMessage` right after the `AgentInput { sid, text }` variant (`protocol.rs:425`):

```rust
    /// Interrupt the session's in-flight turn without tearing the session down.
    AgentCancel {
        sid: String,
    },
```

- [ ] **Step 3: Write roundtrip tests.** Add to the existing `mod tests` at `protocol.rs:691`:

```rust
    #[test]
    fn agent_cancel_and_interrupted_roundtrip() {
        let msg = ClientMessage::AgentCancel { sid: "s1".into() };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&msg).unwrap();
        let back = rkyv::from_bytes::<ClientMessage, rkyv::rancor::Error>(&bytes).unwrap();
        assert!(matches!(back, ClientMessage::AgentCancel { sid } if sid == "s1"));

        let st = AgentRunStatus::Interrupted;
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&st).unwrap();
        let back = rkyv::from_bytes::<AgentRunStatus, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(back, AgentRunStatus::Interrupted);
    }
```

- [ ] **Step 4: Run tests.** `cargo test -p vmux_service protocol:: -- agent_cancel_and_interrupted_roundtrip`
  Expected: PASS. (Other crates won't compile yet — the `consume_page_agent_stream` match becomes non-exhaustive; fixed in Task 7. Compile-check `vmux_service` alone here: `cargo test -p vmux_service`.)

- [ ] **Step 5: Commit.**
```bash
git add crates/vmux_service/src/protocol.rs
git commit -m "feat(agent): wire AgentCancel message + Interrupted run status"
```

---

## Task 2: ACP driver — cancel the in-flight prompt

**Files:**
- Modify: `crates/vmux_service/src/acp/driver.rs:28-35` (AcpInput), `:38-46` (AcpShared), `:240-278` (loop), tests `:375`
- Modify: `crates/vmux_service/src/acp.rs:47-55` (AcpShared init)

**How cancel works:** on `Cancel`, set a `cancel_requested` flag, deny any pending permission (unblocks a mid-approval turn), and send ACP `CancelNotification` (`session/cancel`). The agent then resolves the in-flight `PromptRequest`; the spawned completion task reads the flag and emits `Interrupted` instead of `Idle`. The driver loop and child stay alive.

- [ ] **Step 1: Add the `Cancel` variant** at `driver.rs:28-35`:

```rust
pub enum AcpInput {
    User(String),
    Approve {
        call_id: String,
        decision: ApprovalDecision,
    },
    /// Interrupt the in-flight prompt (ACP `session/cancel`); keep the session alive.
    Cancel,
    Close,
}
```

- [ ] **Step 2: Add the flag to `AcpShared`** at `driver.rs:38-46` (add field + import `AtomicBool`):

```rust
use std::sync::atomic::{AtomicBool, Ordering};
```
```rust
pub struct AcpShared {
    pub sid: String,
    pub cwd: PathBuf,
    pub anchor: ProcessId,
    pub stream_tx: broadcast::Sender<ServiceMessage>,
    pub projector: Mutex<AcpProjector>,
    pub pending_perms: Mutex<HashMap<String, oneshot::Sender<ApprovalDecision>>>,
    pub terminals: Mutex<HashMap<String, ProcessId>>,
    /// Set by `AcpInput::Cancel`; read (and reset) when the in-flight prompt resolves so it
    /// reports `Interrupted` rather than `Idle`.
    pub cancel_requested: AtomicBool,
}
```

- [ ] **Step 3: Extract the status decision** as a pure, testable helper (add near the bottom of `driver.rs`, before `#[cfg(test)]`):

```rust
/// Decide the run status to emit after a prompt future resolves. A cancel in flight wins over
/// both success and error so the UI shows `Interrupted`.
fn status_after_prompt(cancelled: bool, errored: Option<String>) -> AgentRunStatus {
    if cancelled {
        AgentRunStatus::Interrupted
    } else if let Some(err) = errored {
        AgentRunStatus::Errored(err)
    } else {
        AgentRunStatus::Idle
    }
}
```

- [ ] **Step 4: Reset the flag on dispatch and use the helper on completion.** Replace the `AcpInput::User(text)` arm body (`driver.rs:242-266`):

```rust
                    AcpInput::User(text) => {
                        main_shared.cancel_requested.store(false, Ordering::SeqCst);
                        main_shared
                            .projector
                            .lock()
                            .unwrap()
                            .push_user(text.clone());
                        main_shared.emit(main_shared.snapshot_message());
                        main_shared.emit_status(AgentRunStatus::Streaming);
                        let cx_prompt = cx.clone();
                        let shared = main_shared.clone();
                        let session_id = session.session_id.clone();
                        cx.spawn(async move {
                            let prompt = PromptRequest::new(
                                session_id,
                                vec![ContentBlock::Text(TextContent::new(text))],
                            );
                            let errored = match cx_prompt.send_request(prompt).block_task().await {
                                Ok(_) => None,
                                Err(err) => Some(err.to_string()),
                            };
                            let cancelled = shared.cancel_requested.swap(false, Ordering::SeqCst);
                            shared.emit(shared.snapshot_message());
                            shared.emit_status(status_after_prompt(cancelled, errored));
                            Ok(())
                        })?;
                    }
```

- [ ] **Step 5: Add the `Cancel` arm** (insert before the `AcpInput::Close` arm at `driver.rs:273`):

```rust
                    AcpInput::Cancel => {
                        main_shared.cancel_requested.store(true, Ordering::SeqCst);
                        for (_id, tx) in main_shared.pending_perms.lock().unwrap().drain() {
                            let _ = tx.send(ApprovalDecision::Deny);
                        }
                        let _ = cx
                            .send_notification(CancelNotification::new(session.session_id.clone()));
                    }
```

- [ ] **Step 6: Initialize the flag** in `acp.rs` `AcpShared { .. }` (`acp.rs:47-55`), add:
```rust
            cancel_requested: std::sync::atomic::AtomicBool::new(false),
```

- [ ] **Step 7: Unit-test the helper.** Add to `driver.rs` `mod tests`:

```rust
    #[test]
    fn status_after_prompt_cancel_wins() {
        assert_eq!(status_after_prompt(false, None), AgentRunStatus::Idle);
        assert_eq!(
            status_after_prompt(false, Some("boom".into())),
            AgentRunStatus::Errored("boom".into())
        );
        assert_eq!(status_after_prompt(true, None), AgentRunStatus::Interrupted);
        assert_eq!(
            status_after_prompt(true, Some("boom".into())),
            AgentRunStatus::Interrupted
        );
    }
```

- [ ] **Step 8: Run + commit.**
```bash
cargo test -p vmux_service acp:: -- status_after_prompt_cancel_wins
git add crates/vmux_service/src/acp/driver.rs crates/vmux_service/src/acp.rs
git commit -m "feat(acp): interrupt in-flight prompt via session/cancel (keep session alive)"
```

---

## Task 3: Provider-direct path — cancel the streaming turn

**Files:**
- Modify: `crates/vmux_service/src/agent.rs:39-46` (SessionInput), `:134-148` (spawn_sse), `:158-182` (recv/decision helpers), `:216-335` (streaming loop + approval)

**Rationale:** the provider path (`AgentVariant::Page`, anthropic/openai/mistral) shares the GUI drainers, so it must honor cancel too. Cancel breaks the turn back to idle, aborts the SSE HTTP task, and emits `Interrupted`.

- [ ] **Step 1: Add `Cancel`** at `agent.rs:39-46`:

```rust
pub enum SessionInput {
    User(String),
    Approve {
        call_id: String,
        decision: ApprovalDecision,
    },
    Cancel,
    Close,
}
```

- [ ] **Step 2: Return the SSE abort handle from `spawn_sse`** (`agent.rs:134-148`):

```rust
fn spawn_sse(
    request: reqwest::Request,
    parse: ParseSse,
) -> (mpsc::UnboundedReceiver<StreamEvent>, tokio::task::JoinHandle<()>) {
    let (cb_tx, cb_rx) = crossbeam_channel::unbounded::<StreamEvent>();
    let (ev_tx, ev_rx) = mpsc::unbounded_channel::<StreamEvent>();
    tokio::task::spawn_blocking(move || {
        while let Ok(event) = cb_rx.recv() {
            if ev_tx.send(event).is_err() {
                break;
            }
        }
    });
    let http = tokio::spawn(async move {
        crate::http::drive_sse(request, parse, cb_tx).await;
    });
    (ev_rx, http)
}
```

- [ ] **Step 3: Add a `Decision` enum + update the wait helpers** (`agent.rs:158-182`). Replace `recv_user` and `await_decision`:

```rust
enum Decision {
    Allow,
    Deny,
    Cancelled,
    Closed,
}

async fn recv_user(input_rx: &mut mpsc::UnboundedReceiver<SessionInput>) -> Option<String> {
    loop {
        match input_rx.recv().await {
            Some(SessionInput::User(text)) => return Some(text),
            Some(SessionInput::Approve { .. }) | Some(SessionInput::Cancel) => continue,
            Some(SessionInput::Close) | None => return None,
        }
    }
}

async fn await_decision(
    input_rx: &mut mpsc::UnboundedReceiver<SessionInput>,
    call_id: &str,
) -> Decision {
    loop {
        match input_rx.recv().await {
            Some(SessionInput::Approve { call_id: cid, decision }) if cid == call_id => {
                return match decision {
                    ApprovalDecision::Allow => Decision::Allow,
                    ApprovalDecision::Deny => Decision::Deny,
                };
            }
            Some(SessionInput::Cancel) => return Decision::Cancelled,
            Some(SessionInput::Approve { .. }) | Some(SessionInput::User(_)) => continue,
            Some(SessionInput::Close) | None => return Decision::Closed,
        }
    }
}
```

- [ ] **Step 4: Make the streaming loop cancellable** (`agent.rs:226-275`). Replace the `let mut ev_rx = spawn_sse(...)` declaration, the `while let Some(event) = ev_rx.recv().await { ... }` loop, AND the original `if !blocks.is_empty() { messages.push(Assistant) }` push at `:273-275` — this block re-does that push exactly once, so do not leave the original:

```rust
            let (mut ev_rx, http) = spawn_sse(request, provider.parse_sse);
            let mut blocks: Vec<AssistantBlock> = Vec::new();
            let mut partial: Option<(String, String, String)> = None;
            let mut pending_tool: Option<(String, String, String)> = None;
            let mut errored: Option<String> = None;
            let mut cancelled = false;

            loop {
                tokio::select! {
                    biased;
                    signal = input_rx.recv() => {
                        match signal {
                            Some(SessionInput::Cancel) => { cancelled = true; break; }
                            Some(SessionInput::Close) | None => { http.abort(); return; }
                            // User is held host-side; Approve only after streaming. Ignore mid-turn.
                            Some(SessionInput::User(_)) | Some(SessionInput::Approve { .. }) => {}
                        }
                    }
                    event = ev_rx.recv() => {
                        let Some(event) = event else { break; };
                        match event {
                            StreamEvent::TextDelta(text) => {
                                append_text(&mut blocks, &text);
                                let _ = stream_tx.send(ServiceMessage::AgentDelta {
                                    sid: sid.clone(),
                                    text,
                                });
                            }
                            StreamEvent::ToolUseStart { call_id, name } => {
                                partial = Some((call_id, name, String::new()));
                            }
                            StreamEvent::ToolUseArgsDelta { call_id, json_chunk } => {
                                if let Some(p) = &mut partial {
                                    if p.0.is_empty() && !call_id.is_empty() {
                                        p.0 = call_id;
                                    }
                                    p.2.push_str(&json_chunk);
                                }
                            }
                            StreamEvent::ToolUseEnd { call_id } => {
                                if let Some((mut cid, name, args)) = partial.take() {
                                    if cid.is_empty() && !call_id.is_empty() {
                                        cid = call_id;
                                    }
                                    blocks.push(AssistantBlock::ToolUse {
                                        call_id: cid.clone(),
                                        name: name.clone(),
                                        args: args.clone(),
                                    });
                                    pending_tool = Some((cid, name, args));
                                }
                            }
                            StreamEvent::StopTurn { .. } => {}
                            StreamEvent::Error(msg) => errored = Some(msg),
                        }
                    }
                }
            }

            if cancelled {
                http.abort();
            }
            if !blocks.is_empty() {
                messages.lock().await.push(Message::Assistant { blocks });
            }
            if cancelled {
                let _ = stream_tx.send(snapshot_message(&sid, &messages).await);
                let _ = stream_tx.send(ServiceMessage::AgentRunStatusChanged {
                    sid: sid.clone(),
                    status: AgentRunStatus::Interrupted,
                });
                break;
            }
```

- [ ] **Step 5: Handle `Cancelled` at the approval await** (`agent.rs:308-319`). Replace the `match await_decision(...)` block:

```rust
                match await_decision(&mut input_rx, &call_id).await {
                    Decision::Closed => return,
                    Decision::Cancelled => {
                        let _ = stream_tx.send(ServiceMessage::AgentRunStatusChanged {
                            sid: sid.clone(),
                            status: AgentRunStatus::Interrupted,
                        });
                        break;
                    }
                    Decision::Deny => {
                        messages.lock().await.push(Message::ToolResult {
                            call_id,
                            content: "Tool call denied by user.".to_string(),
                            is_error: true,
                        });
                        continue;
                    }
                    Decision::Allow => {}
                }
```

- [ ] **Step 6: Compile-check + commit.**
```bash
cargo test -p vmux_service
git add crates/vmux_service/src/agent.rs
git commit -m "feat(agent): cancel provider-direct streaming turn (abort SSE, emit Interrupted)"
```

---

## Task 4: Route `AgentCancel` in the daemon

**Files:**
- Modify: `crates/vmux_service/src/server.rs:755` (after the `AgentInput` arm)

- [ ] **Step 1: Add the routing arm** after the `ClientMessage::AgentInput { .. }` arm (`server.rs:755`):

```rust
            ClientMessage::AgentCancel { sid } => {
                if acp_manager.lock().await.contains(&sid) {
                    acp_manager
                        .lock()
                        .await
                        .input(&sid, crate::acp::AcpInput::Cancel);
                } else {
                    agent_manager
                        .lock()
                        .await
                        .input(&sid, crate::agent::SessionInput::Cancel);
                }
            }
```

- [ ] **Step 2: Compile + commit.**
```bash
cargo build -p vmux_service
git add crates/vmux_service/src/server.rs
git commit -m "feat(agent): route AgentCancel to ACP + provider session managers"
```

---

## Task 5: Host queue component

**Files:**
- Modify: `crates/vmux_agent/src/components.rs:1-29` (add `PromptQueue`, remove `PendingUserInput`), tests `:31-40`

- [ ] **Step 1: Replace `PendingUserInput` with `PromptQueue`.** Change the import line at `components.rs:1` and the component at `:28-29`:

```rust
use std::collections::{HashSet, VecDeque};
```
```rust
/// FIFO of prompts waiting to be dispatched to this session's agent. Drained one at a time
/// while the session is idle; `paused` holds the queue after an interrupt until the user
/// resumes, clears, or submits again.
#[derive(Component, Clone, Debug, Default)]
pub struct PromptQueue {
    pub items: VecDeque<String>,
    pub paused: bool,
}

impl PromptQueue {
    /// The gate for dispatching the next prompt: idle, not paused, and something queued.
    pub fn ready(&self, idle: bool) -> bool {
        idle && !self.paused && !self.items.is_empty()
    }
}
```

- [ ] **Step 2: Unit-test the gate.** Replace the test module at `components.rs:31-40`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_components_default_constructible() {
        let _ = AgentMessages::default();
        let _ = AgentApprovalPolicy::default();
        let _ = PromptQueue::default();
    }

    #[test]
    fn prompt_queue_ready_gate() {
        let mut q = PromptQueue::default();
        assert!(!q.ready(true)); // empty
        q.items.push_back("a".into());
        assert!(q.ready(true)); // idle + queued
        assert!(!q.ready(false)); // busy
        q.paused = true;
        assert!(!q.ready(true)); // paused
    }
}
```

- [ ] **Step 3: Run.** `cargo test -p vmux_agent components:: -- prompt_queue_ready_gate` (will fail to compile until Task 6/7 update the drainers/observers that reference `PendingUserInput`; compile those together, then run). Commit with Task 6.

---

## Task 6: Gate the drainers on the queue

**Files:**
- Modify: `crates/vmux_agent/src/client/acp.rs:12` (import), `:296-311` (`send_acp_input`)
- Modify: `crates/vmux_agent/src/client/page/plugin.rs:6` (import), `:102-120` (`send_page_agent_input`), `:36-45` (add `ensure_prompt_queue` system)

- [ ] **Step 1: ACP drainer.** In `client/acp.rs`, change the import at `:12`:
```rust
use crate::components::{AgentApprovalPolicy, PromptQueue};
```
Replace `send_acp_input` (`:296-311`):

```rust
fn send_acp_input(
    mut q: Query<(&AcpSession, &mut AgentRunState, &mut PromptQueue)>,
    service: Option<Res<ServiceClient>>,
) {
    let Some(service) = service else {
        return;
    };
    for (session, mut state, mut queue) in &mut q {
        if !queue.ready(matches!(*state, AgentRunState::Idle)) {
            continue;
        }
        let Some(text) = queue.items.pop_front() else {
            continue;
        };
        service.0.send(ClientMessage::AgentInput {
            sid: session.sid.clone(),
            text,
        });
        *state = AgentRunState::Streaming;
    }
}
```

- [ ] **Step 2: Page drainer.** In `client/page/plugin.rs`, change the import at `:6`:
```rust
use crate::components::{AgentApprovalPolicy, AgentMessages, AgentSession, PromptQueue};
```
Replace `send_page_agent_input` (`:102-120`):

```rust
fn send_page_agent_input(
    mut q: Query<(&AgentSession, &mut AgentRunState, &mut PromptQueue)>,
    service: Option<Res<ServiceClient>>,
) {
    let Some(service) = service else {
        return;
    };
    for (session, mut state, mut queue) in &mut q {
        if session.variant != AgentVariant::Page {
            continue;
        }
        if !queue.ready(matches!(*state, AgentRunState::Idle)) {
            continue;
        }
        let Some(text) = queue.items.pop_front() else {
            continue;
        };
        service.0.send(ClientMessage::AgentInput {
            sid: session.sid.clone(),
            text,
        });
        *state = AgentRunState::Streaming;
    }
}
```

- [ ] **Step 3: Ensure every session has a queue.** Add a system to `PageAgentPlugin` (it already imports both session types). Register it in the `add_systems(Update, (...))` tuple at `client/page/plugin.rs:37-45` (add `ensure_prompt_queue`), and define it:

```rust
fn ensure_prompt_queue(
    mut commands: Commands,
    q: Query<Entity, (Or<(Added<AcpSession>, Added<AgentSession>)>, Without<PromptQueue>)>,
) {
    for entity in &q {
        commands.entity(entity).insert(PromptQueue::default());
    }
}
```

- [ ] **Step 4: Run the component + drainer crate build.**
```bash
cargo test -p vmux_agent components:: -- prompt_queue_ready_gate
```
Expected: PASS (Task 5 + 6 now compile together with Task 7's `Interrupted` arm — do Step 5 of Task 7 before running the full `-p vmux_agent`).

- [ ] **Step 5: Commit (with Task 5).**
```bash
git add crates/vmux_agent/src/components.rs crates/vmux_agent/src/client/acp.rs crates/vmux_agent/src/client/page/plugin.rs
git commit -m "feat(agent): host-side prompt queue with idle-gated FIFO drain"
```

---

## Task 7: Consume `Interrupted` → pause the queue

**Files:**
- Modify: `crates/vmux_agent/src/client/page/plugin.rs:145-195` (`consume_page_agent_stream` query + status match)

- [ ] **Step 1: Add `PromptQueue` to the system query.** In `consume_page_agent_stream`, change the `q` param (`:150-156`) to include the queue:

```rust
    mut q: Query<(
        Entity,
        &mut AgentMessages,
        &mut AgentRunState,
        &mut PromptQueue,
        Option<&AgentSession>,
        Option<&AcpSession>,
    )>,
```
Update the three `q.get_mut(entity)` destructures and the `by_sid` closure to match the new arity (add `_` for the queue where unused): the `by_sid` filter closure becomes `|(e, _, _, _, page, acp)|`; the snapshot branch `let Ok((_, mut messages, _, _, _, _))`; the approval branch `let Ok((_, _, mut state, _, _, _))`.

- [ ] **Step 2: Handle `Interrupted` in the status match** (`:189-193`). Replace with:

```rust
        if let Some(&entity) = by_sid.get(&status.sid)
            && let Ok((_, _, mut state, mut queue, _, _)) = q.get_mut(entity)
        {
            match &status.status {
                AgentRunStatus::Idle => *state = AgentRunState::Idle,
                AgentRunStatus::Streaming => *state = AgentRunState::Streaming,
                AgentRunStatus::Interrupted => {
                    *state = AgentRunState::Idle;
                    queue.paused = true;
                }
                AgentRunStatus::Errored(message) => {
                    *state = AgentRunState::Errored(message.clone())
                }
            }
        }
```

- [ ] **Step 3: Write an ECS message-driven test.** Add to `client/page/plugin.rs` `mod tests`. Register only the four stream messages + the one system (avoids pulling the full `PageAgentPlugin` provider graph), and use `AcpSession` (simple field shape, no non-`Default` enums):

```rust
    #[test]
    fn interrupted_status_pauses_queue_and_idles() {
        use crate::client::acp::AcpSession;
        use crate::components::PromptQueue;
        use vmux_service::agent_events::{
            PageAgentAwaitingApproval, PageAgentDelta, PageAgentRunStatus, PageAgentSnapshot,
        };
        use vmux_service::protocol::AgentRunStatus;

        let mut app = App::new();
        app.add_plugins(bevy::app::TaskPoolPlugin::default())
            .add_message::<PageAgentDelta>()
            .add_message::<PageAgentRunStatus>()
            .add_message::<PageAgentAwaitingApproval>()
            .add_message::<PageAgentSnapshot>()
            .add_systems(Update, consume_page_agent_stream);

        let mut queue = PromptQueue::default();
        queue.items.push_back("next".into());
        let e = app
            .world_mut()
            .spawn((
                AcpSession {
                    agent_id: "a".into(),
                    sid: "s1".into(),
                    cwd: std::path::PathBuf::from("/tmp"),
                    anchor: vmux_core::ProcessId::new(),
                },
                AgentMessages::default(),
                AgentRunState::Streaming,
                queue,
            ))
            .id();
        app.world_mut().write_message(PageAgentRunStatus {
            sid: "s1".into(),
            status: AgentRunStatus::Interrupted,
        });
        app.update();

        let world = app.world();
        assert!(matches!(
            world.get::<AgentRunState>(e),
            Some(AgentRunState::Idle)
        ));
        let q = world.get::<PromptQueue>(e).unwrap();
        assert!(q.paused, "queue must pause after interrupt");
        assert_eq!(q.items.len(), 1, "held item must not auto-advance");
    }
```

- [ ] **Step 4: Run + commit.**
```bash
cargo test -p vmux_agent -- interrupted_status_pauses_queue_and_idles prompt_queue_ready_gate
git add crates/vmux_agent/src/client/page/plugin.rs
git commit -m "feat(agent): interrupt pauses the queue and returns to idle"
```

---

## Task 8: Page intents + snapshot fields + host observers

**Files:**
- Modify: `crates/vmux_agent/src/chat_page/event.rs:19-29` (ChatSnapshot fields), append new intents; tests `:112-122`
- Modify: `crates/vmux_agent/src/chat_page.rs` (imports, plugin registration, observers, `snapshot_of`)

- [ ] **Step 1: Extend `ChatSnapshot`** at `chat_page/event.rs:19-29` (add two fields before the closing brace):

```rust
    /// Prompts queued behind the running turn (FIFO), oldest first. View-only on the page.
    pub queued: Vec<String>,
    /// True after an interrupt: the queue is held (not auto-advancing) until resume/clear/submit.
    pub paused: bool,
```

- [ ] **Step 2: Add the three page→native intents** at the end of `chat_page/event.rs` (before the test module):

```rust
/// Page → native: interrupt the in-flight turn (Esc / Ctrl+C / Stop).
#[derive(
    Clone, Debug, Default,
    serde::Serialize, serde::Deserialize,
    rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub struct ChatCancel;

/// Page → native: resume a queue paused by a prior interrupt.
#[derive(
    Clone, Debug, Default,
    serde::Serialize, serde::Deserialize,
    rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub struct ChatResume;

/// Page → native: drop all queued prompts.
#[derive(
    Clone, Debug, Default,
    serde::Serialize, serde::Deserialize,
    rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub struct ChatClearQueue;
```

- [ ] **Step 3: Register intents.** In `chat_page.rs`: update the import at `:16` to add the three types, and the plugin at `:40-43`:

```rust
use crate::chat_page::event::{
    CHAT_SNAPSHOT_EVENT, ChatApproval, ChatCancel, ChatClearQueue, ChatResume, ChatSnapshot,
    ChatSubmit,
};
```
```rust
        app.add_plugins(
            BinEventEmitterPlugin::<(
                ChatSubmit,
                ChatApproval,
                ChatCancel,
                ChatResume,
                ChatClearQueue,
            )>::for_hosts(&["agent"]),
        )
        .add_observer(on_chat_submit)
        .add_observer(on_chat_approval)
        .add_observer(on_chat_cancel)
        .add_observer(on_chat_resume)
        .add_observer(on_chat_clear_queue)
        .add_systems(Update, (push_chat_to_page, push_chat_on_ready));
```

- [ ] **Step 4: Update imports for queue + sessions** in `chat_page.rs:17-22`:

```rust
use crate::client::acp::AcpSession;
use crate::components::{AgentMessages, AgentSession, PromptQueue};
use crate::events::{AgentApprovalReply, ApprovalDecision};
use crate::run_state::AgentRunState;
use vmux_service::client::ServiceClient;
use vmux_service::protocol::ClientMessage;
```

- [ ] **Step 5: `on_chat_submit` → enqueue + unpause.** Replace `on_chat_submit` (`chat_page.rs:135-149`):

```rust
#[cfg(not(target_arch = "wasm32"))]
fn on_chat_submit(
    trigger: On<BinReceive<ChatSubmit>>,
    child_of: Query<&ChildOf>,
    mut queues: Query<&mut PromptQueue>,
) {
    let webview = trigger.event().webview;
    let text = trigger.event().payload.text.clone();
    let Ok(parent) = child_of.get(webview) else {
        return;
    };
    if let Ok(mut queue) = queues.get_mut(parent.parent()) {
        queue.items.push_back(text);
        queue.paused = false;
    }
}
```

- [ ] **Step 6: Add `on_chat_cancel` / `on_chat_resume` / `on_chat_clear_queue`** (after `on_chat_approval`):

```rust
#[cfg(not(target_arch = "wasm32"))]
fn on_chat_cancel(
    trigger: On<BinReceive<ChatCancel>>,
    child_of: Query<&ChildOf>,
    sessions: Query<(Option<&AcpSession>, Option<&AgentSession>)>,
    service: Option<Res<ServiceClient>>,
) {
    let Some(service) = service else {
        return;
    };
    let Ok(parent) = child_of.get(trigger.event().webview) else {
        return;
    };
    let Ok((acp, page)) = sessions.get(parent.parent()) else {
        return;
    };
    let Some(sid) = acp.map(|s| s.sid.clone()).or_else(|| page.map(|s| s.sid.clone())) else {
        return;
    };
    service.0.send(ClientMessage::AgentCancel { sid });
}

#[cfg(not(target_arch = "wasm32"))]
fn on_chat_resume(
    trigger: On<BinReceive<ChatResume>>,
    child_of: Query<&ChildOf>,
    mut queues: Query<&mut PromptQueue>,
) {
    let Ok(parent) = child_of.get(trigger.event().webview) else {
        return;
    };
    if let Ok(mut queue) = queues.get_mut(parent.parent()) {
        queue.paused = false;
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn on_chat_clear_queue(
    trigger: On<BinReceive<ChatClearQueue>>,
    child_of: Query<&ChildOf>,
    mut queues: Query<&mut PromptQueue>,
) {
    let Ok(parent) = child_of.get(trigger.event().webview) else {
        return;
    };
    if let Ok(mut queue) = queues.get_mut(parent.parent()) {
        queue.items.clear();
        queue.paused = false;
    }
}
```

- [ ] **Step 7: Feed queue into the snapshot.** `snapshot_of` and its two callers must read `PromptQueue`. Change `snapshot_of` signature/body (`chat_page.rs:76-102`) to take the queue:

```rust
#[cfg(not(target_arch = "wasm32"))]
fn snapshot_of(messages: &AgentMessages, state: &AgentRunState, queue: &PromptQueue) -> ChatSnapshot {
    let messages_json = serde_json::to_string(&messages.0).unwrap_or_else(|_| "[]".to_string());
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
    ChatSnapshot {
        messages_json,
        status: status.to_string(),
        error,
        approval_call_id: call_id,
        approval_name: name,
        queued: queue.items.iter().cloned().collect(),
        paused: queue.paused,
    }
}
```
Then update the two callers to query + pass `PromptQueue`:
- `push_chat_on_ready` (`:53`): `sessions: Query<(&AgentMessages, &AgentRunState, &PromptQueue)>;` destructure `(messages, state, queue)`; call `snapshot_of(messages, state, queue)`.
- `push_chat_to_page` (`:108-111`): add `&PromptQueue` to the tuple and the `Or<(Changed<AgentMessages>, Changed<AgentRunState>, Changed<PromptQueue>)>` filter; destructure `(stack, messages, state, queue)`; call `snapshot_of(messages, state, queue)`.

- [ ] **Step 8: Update the snapshot roundtrip test** at `event.rs:112-122` to cover the new fields:

```rust
    #[test]
    fn chat_snapshot_rkyv_roundtrip() {
        let v = ChatSnapshot {
            messages_json: "[]".to_string(),
            status: "streaming".to_string(),
            queued: vec!["a".into(), "b".into()],
            paused: true,
            ..Default::default()
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&v).unwrap();
        let back = rkyv::from_bytes::<ChatSnapshot, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(back.status, "streaming");
        assert_eq!(back.queued, vec!["a".to_string(), "b".to_string()]);
        assert!(back.paused);
    }
```

- [ ] **Step 9: Run + commit.**
```bash
cargo test -p vmux_agent -- chat_snapshot_rkyv_roundtrip
git add crates/vmux_agent/src/chat_page/event.rs crates/vmux_agent/src/chat_page.rs
git commit -m "feat(agent): chat cancel/resume/clear intents + queued/paused snapshot"
```

---

## Task 9: Page UI — chips, Stop/Resume/Clear, Esc/Ctrl+C

**Files:**
- Modify: `crates/vmux_agent/src/chat_page/page.rs` (imports, signals, snapshot listener, input bar, key handlers, `do_submit`)

- [ ] **Step 1: Import the new intents** at `page.rs:3-5`:

```rust
use crate::chat_page::event::{
    CHAT_SNAPSHOT_EVENT, ChatApproval, ChatBlock, ChatCancel, ChatClearQueue, ChatMessage,
    ChatResume, ChatSnapshot, ChatSubmit,
};
```

- [ ] **Step 2: Add signals** after `at_bottom`/`last_top` (`page.rs:28-29`):

```rust
    let mut queued = use_signal(Vec::<String>::new);
    let mut paused = use_signal(|| false);
```

- [ ] **Step 3: Populate them in the snapshot listener** (`page.rs:58-72`), add inside the closure:

```rust
        queued.set(snap.queued.clone());
        paused.set(snap.paused);
```

- [ ] **Step 4: Render queued chips + interrupted marker.** Immediately after the messages `for` loop closes (`page.rs:121`, inside the `max-w-3xl` column), add:

```rust
                    for (qi , qtext) in queued.read().iter().enumerate() {
                        div {
                            key: "q{qi}",
                            class: "max-w-[80%] self-end whitespace-pre-wrap rounded-2xl border border-dashed border-foreground/20 bg-foreground/[0.03] px-4 py-2.5 text-sm text-muted-foreground",
                            span { class: "mr-2 text-[10px] uppercase tracking-wide text-foreground/40", "queued" }
                            "{qtext}"
                        }
                    }
                    if paused() {
                        div { class: "flex items-center gap-3 py-1 text-xs text-muted-foreground",
                            span { class: "h-px flex-1 bg-foreground/10" }
                            span { class: "shrink-0", "interrupted" }
                            span { class: "h-px flex-1 bg-foreground/10" }
                        }
                    }
```

- [ ] **Step 5: Stop/Send button + Resume/Clear row.** Replace the input bar block (`page.rs:187-208`) — the `div { class: "relative z-10 border-t ..." }` — with:

```rust
            div { class: "relative z-10 border-t border-foreground/10 bg-background/50 px-4 py-3 backdrop-blur-xl",
                if paused() && !queued.read().is_empty() {
                    div { class: "mx-auto mb-2 flex max-w-3xl items-center gap-2",
                        button {
                            class: "rounded-lg bg-foreground/10 px-3 py-1.5 text-xs font-medium hover:bg-foreground/20",
                            onclick: move |_| { let _ = try_cef_bin_emit_rkyv(&ChatResume); },
                            "▶ Resume ({queued.read().len()})"
                        }
                        button {
                            class: "rounded-lg px-3 py-1.5 text-xs text-muted-foreground hover:bg-foreground/10",
                            onclick: move |_| { let _ = try_cef_bin_emit_rkyv(&ChatClearQueue); },
                            "✕ Clear"
                        }
                    }
                }
                div { class: "mx-auto flex max-w-3xl items-end gap-2",
                    textarea {
                        class: "max-h-40 flex-1 resize-none rounded-xl bg-foreground/[0.06] px-3.5 py-2.5 text-sm ring-1 ring-inset ring-foreground/10 transition focus:bg-foreground/[0.09] focus:outline-none focus:ring-foreground/25",
                        rows: "1",
                        placeholder: "Message the agent…",
                        value: "{draft}",
                        oninput: move |e| draft.set(e.value()),
                        onkeydown: move |e| {
                            let streaming = matches!(status().as_str(), "streaming" | "awaiting");
                            if e.key() == Key::Enter && !e.modifiers().shift() {
                                e.prevent_default();
                                do_submit(draft, at_bottom);
                            } else if e.key() == Key::Escape {
                                if streaming {
                                    e.prevent_default();
                                    let _ = try_cef_bin_emit_rkyv(&ChatCancel);
                                } else if !draft.peek().is_empty() {
                                    draft.set(String::new());
                                }
                            } else if e.modifiers().ctrl()
                                && matches!(e.key(), Key::Character(c) if c == "c")
                                && streaming
                                && !has_text_selection()
                            {
                                e.prevent_default();
                                let _ = try_cef_bin_emit_rkyv(&ChatCancel);
                            }
                        },
                    }
                    if matches!(status().as_str(), "streaming" | "awaiting") {
                        button {
                            class: "rounded-xl bg-red-500/90 px-4 py-2 text-sm font-medium text-white hover:brightness-110 active:scale-[0.99]",
                            onclick: move |_| { let _ = try_cef_bin_emit_rkyv(&ChatCancel); },
                            "■ Stop"
                        }
                    } else {
                        button {
                            class: "rounded-xl bg-foreground px-4 py-2 text-sm font-medium text-background hover:brightness-110 active:scale-[0.99]",
                            onclick: move |_| do_submit(draft, at_bottom),
                            "Send"
                        }
                    }
                }
            }
```

- [ ] **Step 6: Simplify `do_submit`** (`page.rs:213-232`) — no more optimistic message/status (snapshot drives it):

```rust
fn do_submit(mut draft: Signal<String>, mut at_bottom: Signal<bool>) {
    let text = draft.peek().trim().to_string();
    if text.is_empty() {
        return;
    }
    if try_cef_bin_emit_rkyv(&ChatSubmit { text }).is_err() {
        return;
    }
    at_bottom.set(true);
    draft.set(String::new());
}
```
Remove the now-unused `messages`/`status` params from both `do_submit` call sites (done in Step 5) and drop the `use ... ChatMessage` optimism. `messages`/`status` signals remain (still used for rendering). Delete the now-unused `render_message` import? No — still used. Just ensure no unused-variable warnings: `do_submit` no longer needs `messages`/`status`.

- [ ] **Step 7: Add the selection helper** near `current_agent` (`page.rs:16`):

```rust
/// True when the page has a non-collapsed text selection — so Ctrl+C should copy, not interrupt.
fn has_text_selection() -> bool {
    web_sys::window()
        .and_then(|w| w.get_selection().ok().flatten())
        .map(|s| !s.is_collapsed())
        .unwrap_or(false)
}
```

- [ ] **Step 8: Enable the web-sys `Selection` feature** (required by `has_text_selection`). In `crates/vmux_agent/Cargo.toml:49`:

```toml
web-sys = { version = "0.3", features = ["Window", "Document", "Element", "Selection"] }
```

- [ ] **Step 9: Typecheck the page (wasm).**
```bash
cargo check -p vmux_agent --target wasm32-unknown-unknown
```
Expected: no errors. (Full page bundling happens in the app build; Task 10.)

- [ ] **Step 10: Commit.**
```bash
git add crates/vmux_agent/src/chat_page/page.rs crates/vmux_agent/Cargo.toml
git commit -m "feat(agent): queue chips, Stop/Resume/Clear, Esc+Ctrl+C interrupt on chat page"
```

---

## Task 10: Verification pass (build, checks, runtime)

- [ ] **Step 1: Workspace tests.** `cargo test --workspace` → all green (register any plugin-written messages in `build()` if a test complains; commit post-fixes).
- [ ] **Step 2: Format + lint.** `cargo fmt` then `git checkout -- patches/` (fmt touches vendored patches — keep only `crates/` changes). `cargo clippy --workspace -- -D warnings`.
- [ ] **Step 3: Build the app** (warm target already): `make dev` build path, or the project's standard build, and confirm the page bundles.
- [ ] **Step 4: Runtime test (user).** Hand to the user to verify in-app:
  - Type + Enter mid-turn → prompt shows as a **queued** chip; auto-sends when the turn ends.
  - **Esc** and **Ctrl+C** (no selection) and **Stop** each interrupt the running turn → `── interrupted ──` marker, back to idle.
  - After interrupt with a held queue → **Resume** dispatches it, **Clear** drops it.
  - Ctrl+C with text selected → copies (does not interrupt).
  - Interrupt during a permission prompt → denies + stops.
- [ ] **Step 5: Delete this plan file** once fully verified (`git rm docs/plans/2026-07-03-agent-chat-queue-cancel.md`), then open the PR.

---

## Known limitations (documented, out of scope)

- A misbehaving ACP agent that ignores `session/cancel` leaves the turn "streaming" until it resolves; no client-side timeout in v1.
- Provider-path cancel aborts the SSE HTTP task; a token already in flight upstream may still be billed.
- Queue items are view-only (no per-item edit/remove/reorder) by design; only queue-level Resume/Clear.
