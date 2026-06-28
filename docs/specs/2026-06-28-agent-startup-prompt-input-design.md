# Agent Startup Prompt Input — Design

**Date:** 2026-06-28
**Status:** Approved (design), pending implementation

## Problem

CLI agents (claude / codex / vibe) take several seconds to boot before their TUI
is ready for input. During that window vmux shows an agent-startup loading
skeleton (`vmux_terminal/src/page.rs`, the `TermLoadingEvent` overlay) — a shimmer
chat with a **fake input bar**. The user can only wait.

Goal: let the user start typing a prompt *while the agent boots*, and have vmux
deliver that prompt into the agent the moment it is ready.

## Existing mechanics this builds on

- **Agents are PTY terminals.** `handle_spawn_agent_requests`
  (`vmux_agent/src/plugin.rs`) spawns the CLI as a `TerminalLaunch`; the process
  starts immediately. The prompt therefore cannot be passed as a launch arg after
  the fact — it must be typed into the PTY once the agent is ready.
- **The "rain" already exists.** `arm_agent_loading` inserts `AgentLoading` on
  `Added<PageReady>` for agent terminals and emits `TermLoadingEvent{loading:true}`;
  `clear_agent_loading` removes it (emitting `loading:false`) when the terminal
  enters `alt_screen` (TUI up) or after a 10s timeout (`AGENT_LOADING_TIMEOUT`).
  (`vmux_terminal/src/plugin.rs:2680`.)
- **Readiness signal.** `alt_screen == true` in `TerminalModeMap` is the real
  "agent TUI is up and accepting input" signal — the same one `clear_agent_loading`
  uses.
- **Page → backend events** are routed per-entity: the page calls
  `try_cef_bin_emit_rkyv(&Event)`; the backend observes `On<BinReceive<Event>>` and
  reads `trigger.event_target()` to get the terminal entity (see `on_term_key`,
  `on_term_resize`). Inbound event types are registered in
  `BinEventEmitterPlugin::<(...)>::for_hosts(&["terminal"])`.
- **In-page DOM inputs work** under this CEF setup — `vmux_editor/src/page.rs:969`
  runs a focused `<textarea>` alongside a container `onkeydown`, proving a focused
  child input captures typing and (with `stop_propagation`) keeps it away from the
  terminal container handler.

## Approach (chosen)

Backend-buffered, deliver-on-ready. The page renders a real input in the existing
skeleton and streams the draft text to the backend on every change. The backend
holds the draft on the terminal entity and flushes it into the PTY the instant the
agent TUI is ready (`alt_screen`). This keeps state/logic in the backend (the
project's "dumb Dioxus frontend" rule), handles the ready-while-typing race
cleanly, and reuses the existing loading lifecycle and inbound-event plumbing.

Rejected alternatives:

- *Frontend-buffered, single handoff* — the overlay unmounts exactly when we'd
  send (race-prone), and it pushes decision logic into the page.
- *Delay spawn, pass prompt as CLI arg* — defeats the goal (no type-while-booting
  overlap) and CLI support for a positional prompt is uneven.

## Delivery semantics

- Everything typed is buffered backend-side.
- **Enter (no Shift)** in the box = commit (submit intent). **Shift+Enter** =
  newline. Enter is ignored while an IME composition is active
  (`KeyboardEvent.isComposing`), so confirming a candidate never submits.
- On agent ready (`alt_screen == true`):
  - Always deliver the buffered text into the PTY, then **drop the buffer** —
    even an empty/no-op buffer is cleared so a latched `submit` cannot leak into
    the next boot.
  - Append `\r` (submit) **only if** the user committed with Enter.
  - Ready-while-typing race (no Enter yet): deliver the current text **unsent** —
    it lands in the live composer and the user keeps editing.
- Delivery bytes wrap the text in **bracketed paste** (`ESC[200~` … `ESC[201~`),
  then a trailing `\r` only when committed. Bracketed paste prevents multiline /
  special characters from pre-submitting; the explicit `\r` is the only submit.
  Any embedded `ESC[201~` in the draft is stripped first so a paste cannot break
  out of paste mode early.
- **Delivery** gates on `alt_screen`, **not** the skeleton's 10s timeout — a
  timeout-cleared skeleton still buffers and waits for true readiness before
  typing. Note: the *visible* composer lives inside the loading overlay, which
  shares the existing splash lifecycle and is removed at the 10s timeout; on an
  unusually slow boot (TUI not up within 10s) the composer disappears while the
  backend keeps buffering and still delivers on readiness. Keeping the composer
  mounted until `alt_screen` is a separate change to the shared splash lifecycle
  and is out of scope here.
- The page resets the draft + committed state on every loading-session
  transition, so a re-armed splash (e.g. PTY restart) opens clean instead of
  reusing the previous text.

## Components & data flow

### `vmux_core::event`

```rust
pub const AGENT_PROMPT_DRAFT_EVENT: &str = "agent_prompt_draft";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize,
         rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct AgentPromptDraftEvent {
    pub text: String,
    pub submit: bool,
}
```

### Frontend — `vmux_terminal/src/page.rs`

Replace the fake input bar in the loading overlay (the `mx-4 mb-4 …` shimmer block)
with a real `<textarea>`:

- Local `draft: Signal<String>` and `committed: Signal<bool>`.
- Styled to match the existing bar (rounded, accent caret); autofocus on mount via
  `use_effect`.
- `oninput` → update `draft`, emit `AgentPromptDraftEvent{ text, submit:false }`.
- `onkeydown` → `stop_propagation()` (keep keys off the terminal container handler);
  `Enter && !shift` → set `committed`, emit `{ text, submit:true }`, `prevent_default`;
  `Shift+Enter` → newline (default).
- When `committed`, show a subtle "queued — sends when ready" affordance; keep the
  text visible.

The overlay stays `pointer-events-none` except the input region
(`pointer-events-auto`).

### Backend — `vmux_terminal/src/plugin.rs`

- Add `AgentPromptDraftEvent` to the `BinEventEmitterPlugin` tuple
  (`for_hosts(&["terminal"])`).
- Component:

  ```rust
  #[derive(Component, Default)]
  struct BufferedAgentPrompt { text: String, submit: bool }
  ```

- Observer `on_agent_prompt_draft(On<BinReceive<AgentPromptDraftEvent>>)`: upsert
  `BufferedAgentPrompt` on `event_target()`, only for entities with `AgentSession`.
  Replace `text` each event; `submit` latches true once set.
- System `flush_buffered_agent_prompt` (scheduled after `poll_service_messages`, so
  `TerminalModeMap` is fresh): for `(Entity, &ProcessId, &BufferedAgentPrompt)` with
  `AgentSession`, if `TerminalModeMap` reports `alt_screen == true`, send
  `ClientMessage::ProcessInput { process_id, data }` (bracketed-paste(text) + `\r`
  if `submit`), then `remove::<BufferedAgentPrompt>()`.

## Testing

- **core:** rkyv roundtrip for `AgentPromptDraftEvent` (mirror the `TermLoadingEvent`
  roundtrip test in `event.rs`).
- **backend** (`plugin.rs`, App + systems harness like the `clear_agent_loading`
  tests):
  - observer upserts `BufferedAgentPrompt` on an agent terminal; ignores a
    non-agent terminal.
  - `submit` latches once set across subsequent non-submit drafts.
  - `flush_buffered_agent_prompt` sends `ProcessInput` only when `alt_screen` true;
    nothing before.
  - committed draft → delivered bytes end with `\r`; race (submit=false) → no `\r`.
  - buffer removed after flush.

## Risks

- **Bracketed paste support.** Assumes the agent TUI enables bracketed-paste mode
  (claude / codex / vibe do). If a future agent does not, the escapes could print
  literally; fallback is raw text + `\r`. Flag for the end-to-end runtime test.
- **Submit timing.** A trailing `\r` immediately after `alt_screen` flips assumes
  the composer is focused and accepting input at that instant. Verify in the runtime
  test; add a one-frame delay only if needed.

## Out of scope

- Rich editing in the draft box (history, completion) — it is a plain composer.
- Non-agent (plain) terminals — the input only appears in the agent-startup overlay.
