# Agent Chat: Prompt Queue + Cancel (Ctrl+C / Esc) — Design

Date: 2026-07-03
Status: Proposed
Scope: `vmux://agent` native chat page (ACP host path; provider-direct path secondary)

## Goal

Give the agent chat page prompt-queue + interrupt behavior comparable to Claude Code / Codex CLI:

1. **Queue** — typing and submitting while the agent is busy enqueues the prompt (FIFO) instead of firing a second concurrent turn. Queued prompts are shown; the next one auto-dispatches when the current turn completes normally.
2. **Interrupt** — `Esc`, `Ctrl+C` (when nothing is selected), or a `Stop` button cancels the *current turn* without killing the session. Interrupt pauses the queue (does not auto-advance) and marks the stopped turn `Interrupted`.

## Non-goals (YAGNI)

- Esc-Esc rewind / edit-history.
- Per-item queue reorder / edit / remove (queue items are **view-only**; queue-level Resume/Clear only).
- Ctrl+C-twice-to-quit.
- Rich stop-reason surfacing on the ACP path.

## Current state (baseline)

- **No prompt queue** anywhere. `do_submit` (`crates/vmux_agent/src/chat_page/page.rs:213`) fires `ChatSubmit` on every Enter with no busy gate. `PendingUserInput` (`crates/vmux_agent/src/components.rs:29`) is a single-value component — a second submit overwrites it, and mid-turn submits reach the ACP driver, which spawns a **second concurrent `PromptRequest`** (`crates/vmux_service/src/acp/driver.rs:253`, detached task, `JoinHandle` dropped).
- **No user-facing cancel.** The only ACP `session/cancel` is on session teardown (pane close → `AcpInput::Close` → `CancelNotification` + kill child, `acp/driver.rs:273-289`). The only `onkeydown` on the page handles Enter (`page.rs:195-200`); no Esc, no Ctrl+C.
- Turn state = `AgentRunStatus` (`Streaming | Idle | Errored`, `protocol.rs:257-262`) → `AgentRunState` component (`crates/vmux_agent/src/run_state.rs:3-19`) → `ChatSnapshot.status` string (`chat_page.rs:76-102`) → page `status` signal. Input box is never gated on `status`.

Topology: **Page (WASM/CEF)** ⟷ bin-IPC ⟷ **Bevy host (`vmux_agent` client systems, in `vmux_desktop`)** ⟷ Unix socket (`ClientMessage`/`ServiceMessage`) ⟷ **daemon (`vmux_service`)** ⟷ ACP agent subprocess.

## Approach — host-side ECS queue (chosen)

Queue + pause flag + FIFO gating live in the **Bevy host** (session-entity components + the already-shared drainer systems). The page stays dumb: it renders the snapshot and emits intents. The daemon learns only one new thing (cancel); Resume/Clear are host-local.

Rejected: daemon-side queue (state must be pushed back over the socket, duplicated per backend, pause/resume awkward across the wire, less ECS-testable); page-side queue (violates the dumb-frontend rule, lost on reload, races the authoritative snapshot, and cancel still needs backend plumbing).

Side benefit: gating dispatch on `Idle` eliminates the current concurrent-`PromptRequest` bug.

## State machine (per session)

```
Idle ──submit/drain──▶ Streaming ──ok──▶ Idle ──queue?──▶ auto-advance next
                          │  ▲
              approval ──▶ AwaitingApproval ──▶ Streaming
                          │
             cancel ──▶ Interrupted ──▶ Idle (paused=true)   [queue held, NOT auto-sent]
                          │
             error ──▶ Errored
```

- `paused` is an orthogonal flag on the queue, not a run status. Set true on interrupt; cleared by Resume, Clear, or a fresh submit.
- **Normal completion** (`Streaming → Idle`) auto-advances the queue FIFO while `!paused`.
- **Interrupt** stops the current turn, emits `Interrupted`, sets `paused=true` — nothing auto-fires.
- Interrupt during `AwaitingApproval` = implicit deny + stop → `Interrupted`.

## Components / touch-points

### Wire protocol (`crates/vmux_service/src/protocol.rs`) — minimal

- `ClientMessage::AgentCancel { sid }` — only new inbound message. Routed in `server.rs` beside the `AgentInput` arm (`:743-755`) → ACP: `acp_manager.input(sid, AcpInput::Cancel)`; provider: analogous.
- `AgentRunStatus::Interrupted` — new variant (`:257-262`). Daemon emits it after the turn actually stops (authoritative). Flows back via existing `AgentRunStatusChanged` → `terminal/plugin.rs:1606` → `PageAgentRunStatus` → `consume_page_agent_stream` → `AgentRunState`.
- Resume / Clear are **host-local (ECS only)** — no wire change.

### Daemon ACP (`crates/vmux_service/src/acp/driver.rs`, `acp.rs`)

- Add `AcpInput::Cancel` (`driver.rs:28-35`); manager passthrough in `acp.rs:67` if needed.
- Track the in-flight prompt: store the spawned prompt task's abort handle on `AcpShared` (`driver.rs:38-46`) — today the `cx.spawn` handle at `driver.rs:253` is dropped.
- On `Cancel`: send `CancelNotification::new(session_id)` (ACP `session/cancel`), abort the prompt task, emit `AgentRunStatus::Interrupted`. **Keep the driver loop + child alive** (reverse of the `Close` arm at `:273-289`); next `User` input must still work.
- Provider-direct path (`crates/vmux_service/src/agent.rs`, `SessionInput` `:39-46`): add `Cancel`, abort the stream task, emit `Interrupted`. Secondary but included since the GUI drainers and `consume_page_agent_stream` are shared.

### Bevy host (`crates/vmux_agent` client systems, native)

- `PromptQueue { items: VecDeque<String>, paused: bool }` replaces single-value `PendingUserInput` (`components.rs:29`).
- Drainer — `send_acp_input` (`client/acp.rs:296-311`) and `send_page_agent_input` (`client/page/plugin.rs:102-120`): pop-front and send `ClientMessage::AgentInput` **only when `AgentRunState::Idle && !paused && !items.is_empty()`**. Otherwise leave queued.
- `consume_page_agent_stream` (`client/page/plugin.rs:145-216`): on `Interrupted` set `paused=true`; on normal `Idle` do nothing (drainer auto-advances next tick).
- Observers:
  - `on_chat_submit` (exists, `chat_page.rs:135-149`): `items.push_back(text)`; if `paused`, clear `paused` (submit = resume + append).
  - `on_chat_cancel` (new): send `ClientMessage::AgentCancel { sid }`.
  - `on_chat_resume` (new): `paused = false` (drainer picks up).
  - `on_chat_clear_queue` (new): `items.clear()` (and `paused = false`).
- `snapshot_of` (`chat_page.rs:76-102`): add `queued: Vec<String>` + `paused: bool`; reflect `Interrupted` in status. Push on change via existing `push_chat_to_page`.

### Page (`crates/vmux_agent/src/chat_page/page.rs`, `event.rs`)

- New intents in `chat_page/event.rs` (rkyv wire payloads, shared native+wasm): `ChatCancel`, `ChatResume`, `ChatClearQueue`. Register in the `BinEventEmitterPlugin::<(...)>` tuple at `chat_page.rs:40`.
- `do_submit` (`page.rs:213-232`): always emit `ChatSubmit` (append). **Remove the local `status="streaming"` force (`:230`)** — let the snapshot drive state.
- `ChatSnapshot` (`chat_page/event.rs:19-29`): add `queued` + `paused` fields.
- Render:
  - View-only queued chips above the input (from `snapshot.queued`).
  - `Stop` button replaces `Send` while `Streaming`/`AwaitingApproval` → emits `ChatCancel`.
  - When `paused && !queued.is_empty()`: show `▶ Resume (N)` (`ChatResume`) + `✕ Clear` (`ChatClearQueue`).
  - `── interrupted ──` marker on the stopped assistant turn (driven by `Interrupted` status).
- `onkeydown` (`page.rs:195-200`):
  - **Enter** (no Shift) → submit (existing).
  - **Esc** → if streaming: `ChatCancel` (+`prevent_default`); else clear draft.
  - **Ctrl+C** → if streaming **and** `window.getSelection()` is empty: `ChatCancel` (+`prevent_default`); else allow native copy.

## Data flow

**Submit (busy):** page `ChatSubmit` → `on_chat_submit` pushes to `PromptQueue.items` → drainer sees `Streaming`, holds → snapshot shows new chip.
**Turn completes (ok):** daemon `Idle` → host clears streaming → drainer sees `Idle && !paused`, pops front → `AgentInput` → daemon → next turn.
**Interrupt:** page `ChatCancel` → `on_chat_cancel` → `ClientMessage::AgentCancel` → `server.rs` → `AcpInput::Cancel` → driver sends `session/cancel` + aborts task + emits `Interrupted` → host sets `paused=true` → page shows `Interrupted` marker + `Resume`/`Clear`.
**Resume:** page `ChatResume` → `paused=false` → drainer drains FIFO. **Clear:** `ChatClearQueue` → items emptied.

## Edge cases

- Interrupt with empty queue → stop turn, idle, nothing held.
- Interrupt during `AwaitingApproval` → resolve the pending permission as deny, then `Interrupted`.
- Fresh submit while paused → append to tail, unpause, dispatch FIFO (held items first).
- Ctrl+C with an active text selection → native copy (never hijacked).
- Concurrent-`PromptRequest` bug is fixed implicitly by the `Idle`-gated drainer.

## Testing (Bevy message/ECS, per AGENTS.md)

- **Queue:** send `ChatSubmit` ×N while `Streaming` → `PromptQueue.items` grows, exactly one `AgentInput` emitted; flip to `Idle` → next `AgentInput` drains (auto-advance).
- **Cancel:** `ChatCancel` → `ClientMessage::AgentCancel` emitted; on `Interrupted` → `paused=true`, queue retained, no auto-advance.
- **Resume/Clear:** `ChatResume` unpauses and drains; `ChatClearQueue` empties.
- **ACP driver:** `AcpInput::Cancel` → `CancelNotification` sent, child alive, status `Interrupted`, subsequent `User` still processes.
- **Snapshot:** `ChatSnapshot` round-trips `queued` / `paused` / `Interrupted`.
- **Page source-scrape:** update `include_str!` text asserts in `style.rs` + `tests/page_source.rs` (native `cargo test -p vmux_agent` / `vmux_layout`) — refactors of `page.rs` break these.
- Register any new plugin-written message types in the plugin `build()` (idempotent), and run `cargo test --workspace` before pushing.

## Reconciliation note

Q2 chose **view-only** queue items and Q3 chose **hold queue, stay idle** on interrupt. Items stay view-only (no per-item edit/remove/reorder); the queue gets **queue-level** `Resume` + `Clear` only when paused, so a held queue is never a trap.
