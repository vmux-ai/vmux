# Agent Startup Loading Skeleton Design

## Overview

When a CLI agent (`vibe`, `claude`, `codex`) is opened via `vmux://agent/<kind>/`, the host spawns a PTY terminal and the CEF terminal page renders its grid immediately. There is a gap between spawn and the agent painting its full-screen TUI. During that gap the empty grid is visible and any keyboard/mouse input echoes onto it.

Replace that empty-grid window with a centered loading skeleton that covers the grid until the agent's TUI has painted. The skeleton is dismissed when the process enters the alternate screen buffer (the moment a TUI takes over the screen) or, as a safety net, after a timeout. Input continues to pass through to the PTY unchanged; the skeleton only hides the grid visually.

## Scope

**In scope:**
- Detect alternate-screen-buffer activation server-side and surface it to the host.
- Host-computed per-terminal `loading` state for CLI-agent terminals (`vibe`/`claude`/`codex`), driven by spawn → alt-screen/timeout.
- A new `TermLoadingEvent` delivering the loading flag + agent label to the CEF terminal page.
- A centered skeleton overlay in the terminal page (pulsing agent name + shimmer bar + "starting…").
- Dismiss on alt-screen entry or timeout; also dismiss on process exit.

**Out of scope:**
- Page agents (`vmux://agent/<provider>/<model>/...`) — those render a Browser, not a PTY terminal, and have their own placeholder.
- Error/setup pages (e.g. missing-CLI setup) — those are Browser data-URLs, never an agent PTY terminal, so they never enter the loading state.
- Changing the existing plain-terminal "Loading…" text (shown briefly while rows are empty for non-agent terminals).
- Configurable skeleton appearance / themes.

## Behavior

1. User opens `vmux://agent/vibe/` (or `claude`/`codex`). Host spawns the terminal entity with `AgentSession { kind }` + a PTY launch (existing flow in `vmux_agent`).
2. Once the terminal's CEF page is ready, the host marks the terminal as loading and emits `TermLoadingEvent { loading: true, label }` where `label` is the agent display name.
3. The page renders a centered skeleton overlay covering the grid. `pointer-events-none` lets mouse events fall through to the grid; keyboard handling is unchanged. The grid (and any input echo) is hidden behind the opaque overlay.
4. When the agent process enters the alternate screen buffer (`TermMode::ALT_SCREEN`), the host emits `TermLoadingEvent { loading: false, .. }`. The page hides the skeleton, revealing the painted TUI.
5. Safety net: if alt-screen is not entered within `AGENT_LOADING_TIMEOUT` (10s), the host clears loading and emits `loading: false` anyway.
6. If the process exits while loading, the host clears loading (the stack closes via existing exit handling).

Non-agent terminals never receive `TermLoadingEvent` and behave exactly as today.

## Data Flow

```
alacritty term.mode() ALT_SCREEN
   │  (vmux_service: Process::maybe_broadcast_mode)
   ▼
ServiceMessage::TerminalMode { mouse_capture, copy_mode, alt_screen }
   │  (vmux_terminal host: drain loop)
   ▼
host loading-state systems  ──emit──▶  TermLoadingEvent { loading, label }
   │                                       │ (CEF bin event)
   │                                       ▼
   └── spawn/page-ready, timeout       terminal page (page.rs) renders skeleton
```

## Components & Changes

### 1. `crates/vmux_service/src/protocol.rs`
- Add `alt_screen: bool` to the `ServiceMessage::TerminalMode` variant (alongside `mouse_capture`, `copy_mode`).

### 2. `crates/vmux_service/src/process.rs`
- In `maybe_broadcast_mode`, compute `alt_screen = self.term.mode().contains(TermMode::ALT_SCREEN)`.
- Widen the change-tracking tuple `last_terminal_mode` from `(bool, bool)` to `(bool, bool, bool)` and include `alt_screen` in the emitted `TerminalMode`.

### 3. `crates/vmux_core/src/event.rs`
- Add `pub const TERM_LOADING_EVENT: &str = "term_loading";`.
- Add:
  ```rust
  #[derive(Debug, Clone, Serialize, Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
  pub struct TermLoadingEvent {
      pub loading: bool,
      pub label: String,
  }
  ```
  (matching the derive set used by the other term events).

### 4. `crates/vmux_terminal/src/plugin.rs`
- Extend `TerminalModeFlags` with `alt_screen: bool`; populate it in the `ServiceMessage::TerminalMode` branch.
- Add a marker component for in-flight loading, e.g. `AgentLoading { since: Instant }`, inserted on the agent terminal entity.
- Emit `loading: true`: in the existing service-message drain, when a terminal entity has `AgentSession`, its page is ready (`host_emit_ready`), and it has not yet been marked loading, insert `AgentLoading` and trigger `TermLoadingEvent { loading: true, label: kind.display_name() }`.
- Clear loading (insert/trigger `loading: false`, remove `AgentLoading`) when **either**:
  - the `TerminalMode` for that process reports `alt_screen == true`, or
  - a dedicated system finds `AgentLoading.since.elapsed() >= AGENT_LOADING_TIMEOUT` (`const AGENT_LOADING_TIMEOUT: Duration = Duration::from_secs(10)`).
- In the `ServiceMessage::ProcessExited` branch, remove `AgentLoading` if present (no event needed; the stack closes).
- (Consistency) On `RestartAgentPty` for an agent terminal, re-arm loading: re-insert `AgentLoading` and emit `loading: true`.

### 5. `crates/vmux_terminal/src/page.rs`
- Add a signal `loading: Signal<Option<String>>` (`Some(label)` while loading).
- Add a `use_bin_event_listener::<TermLoadingEvent, _>(TERM_LOADING_EVENT, ...)` that sets the signal from the event.
- Render, above the grid, when `loading()` is `Some(label)`:
  - Full-viewport overlay: `absolute inset-0 z-40 flex flex-col items-center justify-center pointer-events-none`, opaque `bg-term-bg`.
  - Centered content: agent `label`, a shimmer bar (`h-2 w-40 rounded-md bg-term-fg/10 animate-pulse`), and muted "starting…" text.
- Gate the existing rows-empty "Loading…" overlay on `loading().is_none()` so the two don't stack for agent terminals.

## Edge Cases

- **Missing CLI / error / setup page:** rendered as a Browser data-URL, not an agent PTY terminal → no `AgentSession` on a terminal → no loading state. Unaffected.
- **Deep link to an already-running agent:** focuses the existing tab; no spawn, no loading.
- **Agent that never enters alt-screen** (non-TUI or hung): timeout clears the skeleton after 10s.
- **Plain shells:** never marked loading; no behavior change.
- **Resize while loading:** overlay is viewport-filling and re-centers; no special handling.
- **Process exits during loading:** loading marker removed; existing exit flow closes the stack.

## Testing

- **`vmux_service` (unit):** feed `\x1b[?1049h` to a `Process` and assert `TerminalMode` broadcasts `alt_screen == true`; feed `\x1b[?1049l` and assert it returns to `false`. (Model on the existing alt-screen copy-mode test.)
- **`vmux_core` (unit):** rkyv round-trip for `TermLoadingEvent`.
- **`vmux_terminal` host (Bevy unit):** with an `AgentSession` terminal whose page is ready, run the schedule and assert a `loading: true` `TermLoadingEvent` is emitted once; deliver a `TerminalMode { alt_screen: true }` and assert `loading: false` is emitted and `AgentLoading` removed; a timeout test (seed `AgentLoading.since` in the past) asserts `loading: false` is emitted without alt-screen. Use typed messages/systems per project conventions, not ad-hoc helpers.
- **Page (manual):** the Dioxus/CEF page is wasm-rendered; verify visually that opening `vmux://agent/vibe/` shows the centered skeleton until vibe's TUI paints, that mouse passes through, and that no empty grid / input echo is visible during startup.
