# Agent context tree collapse under a working/thinking header

- **Date:** 2026-07-08
- **Status:** Approved (design)
- **Surface:** native-chat / ACP agent chat page (`crates/vmux_agent`)

## Problem

During an agent turn the chat page dumps every step — `Thinking`, `ToolUse`,
`ToolResult`, `Plan`, `Diff` — inline into the transcript as individual
`<details>`, and shows a **separate** bottom spinner with a hardcoded
`"Working…"` label (`chat_page/page.rs:173-183`). This does not match the
Claude / Codex desktop pattern, where a turn's intermediate work collapses under
a single animated header and the prose answer stays inline.

## Goal

Match Claude / Codex desktop turn UX:

- Per assistant **turn**, collapse the steps (the "context tree") into **one
  disclosure**.
- While running, that disclosure's header is the animated spinner: bouncing dots
  + a **randomly cycling verb** + a live elapsed counter.
- When done, the header rests to `Worked for 12s · 4 steps`, still collapsed,
  expandable into the step tree.
- The assistant's prose (`Text`) always renders inline, never hidden.

Architecture principle (user directive): **brain in Bevy core, frontend as dumb
as possible.** See [[feedback_dumb_dioxus_frontend]]. All grouping / duration /
step-count logic is computed backend and pushed as derived structure; the page
only renders it and runs cosmetic animation timers.

## Non-goals

- CLI-PTY agents (they render their own CLI in a terminal; vmux does not restyle
  them). See [[reference_agent_hosting_models]].
- Preserving fine-grained interleave of prose vs tools within a turn (Codex
  style). v1 groups a turn's steps into one disclosure with the prose below
  (claude.ai style). Deferred; revisit only if requested.
- Persisting the resting duration for turns that completed in a **prior** process
  (post-`loadSession` resume): those show `· N steps` with no seconds.

## Behavior spec

For each assistant turn:

| Turn state | Header |
|---|---|
| running, no steps yet, no answer yet | animated: `[dots] {verb}… · {live elapsed}` |
| running, has steps | animated header over collapsed steps; answer (if any) streams inline below |
| running, no steps, answer streaming | no header — the streaming answer is the progress signal |
| done, `step_count > 0` | resting: `Worked for {elapsed} · {k} steps`, collapsed, expandable |
| done, `step_count == 0` | no header — answer only |

- Verb cycles every ~2.5 s while any turn is running; each swap is a random pick
  from a curated gerund set.
- Live elapsed counter is a frontend animation (reuses the existing 1 s loop).
- Resting duration is a backend fact (survives reload within the same process).
- `installing`, `errored`, and `awaiting` states are unchanged. Only the
  `streaming` bottom spinner is replaced by the per-turn running header.

## Data model (brain → dumb frontend)

Carried as JSON inside the existing `ChatSnapshot.messages_json` string field
(kept that name — its value changes from `Vec<ChatMessage>` to `Vec<ChatItem>`,
only the doc comment updates). `ChatSnapshot` stays an rkyv bin-event payload;
both ends are always rebuilt and shipped together, so no persisted
`STORE_SCHEMA_VERSION` bump is involved.

New serde types in `crates/vmux_agent/src/chat_page/event.rs` (this module
already compiles to wasm and native — plain serde, no Bevy, so no `cfg` gate; see
[[reference_vmux_core_event_wasm]]):

```rust
enum ChatItem {
    User { text: String },
    Turn(ChatTurn),
}

struct ChatTurn {
    steps: Vec<ChatBlock>,        // Thinking, ToolUse, ToolResult, Plan, Diff — in chronological order
    answer: Vec<ChatBlock>,       // Text prose — always rendered inline
    running: bool,                // true only for the live (tail) turn
    duration_secs: Option<u32>,   // Some when finished this process; None otherwise
    step_count: u32,              // steps.len(), sent explicitly for the header
}
```

Add `ChatBlock::ToolResult { content: String, is_error: bool }` so tool results
fold into `steps` in order (previously a top-level `ChatMessage::ToolResult`).

Curated verb list lives here too, owned by the shared contract layer rather than
the view:

```rust
pub const WORKING_VERBS: &[&str] = &[
    "Working", "Thinking", "Pondering", "Noodling", "Percolating", "Conjuring",
    "Cooking", "Brewing", "Musing", "Ruminating", "Scheming", "Synthesizing",
    "Tinkering", "Churning", "Vibing", "Simmering", "Crafting", "Divining",
    "Mulling", "Spelunking",
];
```

## Backend design (brain — `crates/vmux_agent`, native)

### Grouping — pure, unit-tested

`fn group_turns(messages: &[Message], durations: &[u32], running: bool) -> Vec<ChatItem>`
(in `chat_page.rs` or a `chat_page/turns.rs` sibling; no `mod.rs`).

Walk `messages` (`crates/vmux_service/src/message.rs`) keeping one open
`current: Option<ChatTurn>`:

- `Message::User { text }` → flush `current` (push it as a `ChatItem::Turn` if
  `Some`), push `ChatItem::User`, then open a fresh empty `current`.
- `Message::Assistant { blocks }` → open `current` if `None` (leading-assistant
  case), then for each `AssistantBlock`: `Text` → `answer`;
  `Thinking | ToolUse | Plan | Diff` → `steps`.
- `Message::ToolResult { content, is_error, .. }` → open `current` if `None`,
  push `ChatBlock::ToolResult` onto `steps`.
- End of walk → flush `current`.

This emits **exactly one `ChatItem::Turn` per turn that started** (one per `User`
message in `messages`, plus an optional leading turn), so turn ordinal ==
duration ordinal.

**Duration pairing.** The `i`-th emitted `Turn` (0-based across `Turn` items
only) takes `durations[i]` → `duration_secs`; out of range → `None`. Because a
turn is emitted per started turn and one duration is pushed per
`Streaming → Idle/Errored` (streaming is serial — turn `N+1` cannot start before
turn `N` idles), completed turns and `durations` stay index-aligned. The trailing
running turn has no entry yet → `None`; also force `duration_secs = None` whenever
`running`. Queued prompts live in `PromptQueue`, not `messages`, so they neither
emit a turn nor consume a duration.

Mark only the **last** turn `running = true` (and only when the run is active).

`snapshot_of` (`chat_page.rs:104`) calls `group_turns`, serializes to
`items_json`, and keeps pushing on `Changed<AgentMessages> | Changed<AgentRunState>`.

### Per-turn duration tracking

New Bevy component on the session entity (native-only, `cfg(not(target_arch =
"wasm32"))`, alongside `AgentRunState` in `run_state.rs`):

```rust
struct AgentTurnMeta {
    durations: Vec<u32>,        // one per completed turn, by order
    turn_start: Option<Duration>, // Time::elapsed() at turn start
}
```

Drive it from the existing `AgentRunState` transition site
(`client/page/plugin.rs`, `consume_page_agent_stream`) using `Res<Time>`
(deterministic + testable — advance virtual time in tests):

- entering `Streaming` and `turn_start.is_none()` → `turn_start = Some(now)`.
  (Do **not** reset on `AwaitingApproval → Streaming`; approval is mid-turn.)
- entering `Idle` or `Errored` with `turn_start = Some(start)` →
  `durations.push((now - start).as_secs()); turn_start = None`.
- `AwaitingApproval` → leave `turn_start` untouched.

`AgentMessagesSnapshot` replace (resume/reload) does not clear `durations`;
`group_turns` tolerates `durations.len() < turn_count` (extra turns → `None`).

## Frontend design (dumb — `chat_page/page.rs`)

- Deserialize `items_json` → `Vec<ChatItem>`; replace the flat `messages` render
  loop. Empty-state check becomes `items.is_empty() && status == "idle"`.
- Render `ChatItem::User` as today's user bubble.
- Render `ChatItem::Turn`:
  - header per the behavior table; `running` → `<details>` with the animated
    summary (bouncing dots + `{verb}…` + `fmt_elapsed(elapsed())`), else resting
    summary (`Worked for {fmt_elapsed(d)} · {k} steps`). Omit the disclosure
    entirely when `step_count == 0 && (!running || !answer.is_empty())`.
  - expanded body renders `steps` via the existing per-block renderers (move the
    `ToolResult` arm from `render_message` into `render_block`).
  - render `answer` blocks inline, below the disclosure.
- Delete the standalone `status == "streaming"` bottom spinner (lines 173-183);
  its role moves into the running turn header.
- **Verb cycling:** a `use_future` loop every 2500 ms picks
  `WORKING_VERBS[(Math::random() * len) as usize]` into a `verb` signal while
  `status == "streaming"`. This is the only new timer; the 1 s `elapsed` loop is
  reused. No other logic on the page.

Cosmetic-animation timers (verb swap, live seconds) stay frontend — same class as
the existing CSS bounce; all facts (grouping, counts, final duration, vocabulary)
come from the backend/shared layer.

## Edge cases

- **User submits, nothing streamed yet:** trailing running turn (empty) → header
  shows immediately.
- **Answer streaming with no tools:** header suppressed once `answer` non-empty
  (`step_count == 0`); answer is its own progress.
- **Zero-step finished turn:** no disclosure, answer only.
- **Resume (`loadSession`):** past turns render `· N steps` (no seconds); new
  turns in this process get durations.
- **Interrupted turn:** `Interrupted → Idle` finalizes duration like a normal
  end; `paused` divider unchanged.

## Testing

Native `cargo test -p vmux_agent` (no CEF):

- `group_turns`:
  - boundaries: `User` splits turns; assistant activity groups under the turn.
  - split: `Text` → `answer`; `Thinking/ToolUse/Plan/Diff` → `steps`;
    `ToolResult` → `steps` in order.
  - `step_count` matches `steps.len()`.
  - one `Turn` emitted per `User`; last turn `running` iff `running`; empty tail
    turn still emitted when `running`.
  - durations paired by turn ordinal; out of range → `None`; running turn always
    `None`.
  - zero-step turn shape.
- duration tracking (Bevy `App` with virtual `Time`, per repo test conventions —
  chain builder calls, send typed messages, assert component state):
  `Streaming`→advance→`Idle` pushes one `≈` duration; `AwaitingApproval` between
  does not reset `turn_start`.

CI `fmt` + `clippy` + tests. Verify the page build via `cargo check --target
wasm32-unknown-unknown -p vmux_agent` (the wasm contract in `event.rs`).

## Files touched

- `crates/vmux_agent/src/chat_page/event.rs` — `ChatItem`, `ChatTurn`,
  `ChatBlock::ToolResult`, `WORKING_VERBS`; `ChatSnapshot` field rename.
- `crates/vmux_agent/src/chat_page.rs` (+ optional `chat_page/turns.rs`) —
  `group_turns`, `snapshot_of` rewrite.
- `crates/vmux_agent/src/run_state.rs` — `AgentTurnMeta` component.
- `crates/vmux_agent/src/client/page/plugin.rs` — duration tracking on
  `AgentRunState` transitions.
- `crates/vmux_agent/src/chat_page/page.rs` — turn rendering, verb cycling,
  delete the old bottom spinner.

## Rollout

Single PR off `origin/main`. No new crates ([[feedback_no_new_crates]]), no store
schema bump. Frontend stays dumb ([[feedback_dumb_dioxus_frontend]]); runtime
tested in one pass at the end ([[feedback_finish_then_test]],
[[feedback_verify_observable_behavior]]).
