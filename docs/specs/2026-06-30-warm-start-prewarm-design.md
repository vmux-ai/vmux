# Warm-start prewarm — Design

## Problem

Opening `vmux://start/` (the default content of every new tab / stack / pane / space, and
the startup tab) shows a visible cold-load delay. The page only paints after a fresh CEF
browser is created and navigated. The goal: make `vmux://start/` appear **near-instant**
whenever it is opened, with no UI or keybinding changes — pure backend perf.

## Diagnosis

The dominant per-open cost is **CEF browser creation**
(`browser_host_create_browser_sync` — new renderer, V8 context, GPU IOSurface, child
`NSView`), not WASM. The start WASM is one embedded bundle shared by every page, served
from in-memory embedded assets, compiled once at build time and cached by V8/disk. So the
lever is warming the **browser instance** (and letting its page + WASM load ahead of time),
exactly as the command bar already does for its modal.

Reference: cold path today is `handle_start_page_open` (`crates/vmux_layout/src/start/plugin.rs:38`)
→ `CefPageAttachRequest` → `attach_cef_page_to_stack` (`crates/vmux_browser/src/lib.rs:3578`),
which spawns a cold `Browser` for every start open.

## Why not copy the command bar verbatim

The command bar is **one** reused instance — it is a modal, only ever one visible, hidden
and revealed in place (`prewarm_command_bar_modal`, `crates/vmux_layout/src/command_bar/handler.rs:233`).

`vmux://start/` is different: every new tab / stack / pane / space is an **independent,
persistent** start page that stays alive in its own stack. Two open start tabs are two live
browsers. A single hidden instance cannot serve them. Start therefore needs a small **warm
pool**, not a singleton.

## Goal / non-goals

**Goal:** every post-startup start open (new tab / stack / pane / space, and in-place nav to
start) reuses a pre-created start browser whose page + WASM are already loaded, so it appears
with no browser-creation latency.

**Non-goals:** no UI surface, no overlay, no keybinding, no placement/target changes. Start
behavior is identical to today — only faster. The very first startup tab may remain cold
(one-time, already tolerated); the pool benefits every open after boot.

## Architecture

### Warm pool of pre-created start browsers

A small pool (target size `WARM_START_POOL_SIZE`, default `1`, may bump to `2` to absorb
back-to-back opens) of pre-created `vmux://start/` `Browser` entities. Each spare's CEF
browser is created at boot and navigated to start, so the **expensive work — browser
creation, page load, WASM instantiation — is already done** before the user opens anything.

**Parking mode (decided): non-compositing.** A spare is parked `Visibility::Hidden` under a
dedicated holding node (NOT a `Pane` / `Stack`). The renderer still loads and runs the page
while hidden (JS/WASM execute regardless of view visibility) — so it is fully warm — but the
hidden view does **not** composite, giving near-zero idle GPU/CPU cost. This matches the
repo's strict idle-CPU stance. (Alternative, rejected as default: keep it compositing via the
command-bar `Display::Flex` + `Visibility::Hidden` trick for pixel-instant reveal — adopt
only if the non-compositing reveal shows a visible flash in end-to-end testing.)

**Marker + sync exclusion.** Pool entities carry a new `WarmStartSpare` marker that excludes
them from the content visibility / focus / windowed-frame sync systems while parked (the same
way `Modal` / `Header` / `SideSheet` are excluded), so stack-active logic can't unhide,
hide, or mis-position them. Add `Without<WarmStartSpare>` to those content queries in
`crates/vmux_browser/src/lib.rs` (`sync_children_to_ui`, windowed/OSR focus + frame sync).
Spares spawn `Visibility::Hidden` and, being excluded, stay hidden until claimed.

**Backend mode.** Spares are created as `WebviewWindowed` in User-mode backend (what
`Browser::new` produces). This keeps `needs_recreate` false on reveal
(`sync_cef_backend_for_interaction_mode`, `crates/vmux_browser/src/lib.rs:935`) — a backend
flip would close+recreate and negate the win.

**Claim eligibility.** A spare is claimable once its CEF browser exists
(`Browsers::has_browser`) and its page has loaded. Use the most reliable available
load/ready signal; do not depend on `RenderTextureMessage` (that is the OSR texture path and
may not fire for windowed spares).

### Claim flow (hook point: `handle_start_page_open`)

All start opens funnel through `handle_start_page_open`
(`crates/vmux_layout/src/start/plugin.rs:38`), which already has the resolved `task.stack`
and knows the URL is exactly `START_PAGE_URL`. Hook here:

1. If no claimable spare → **fall back to the existing cold path** (emit
   `CefPageAttachRequest`). Graceful degradation; no regression.
2. If a claimable spare exists:
   a. `clear_stack_children(task.stack)` — remove any existing content (no-op for the
      freshly-created empty stacks of new tab/stack/pane; required for in-place/ActiveStack
      targets that replace current content).
   b. Reparent the spare: `ChildOf(task.stack)`, remove `WarmStartSpare`, add
      `CefKeyboardTarget`. Normal content sync now positions it from the stack's live rect
      and unhides it (active stack). Reveal rides the existing `webview_reveal.rs` 2-frame
      anti-flash path, or immediately if no flash is observed.
   c. Set the stack's `PageMetadata` to start (url / title / icon / bg) — this is what tab
      strips and `apply_cef_state_from_webview` read.
   d. **Push fresh launcher data** to the revealed spare (see Correctness) so it reflects
      current tabs/spaces/history, not boot-time state.
   e. **Focus** the launcher input via `StartFocusInput`
      (`crates/vmux_layout/src/start/event.rs:19`).
   f. Mark the task handled (`PageOpenHandled`) so `attach_cef_page_requests` skips the cold
      spawn.
3. Enqueue a **refill** — spawn one replacement spare (cold create, off the critical path;
   the user already sees their instant start page).

### Refill

A system maintains `WARM_START_POOL_SIZE` claimable spares: whenever the count drops below
target, spawn a new `WarmStartSpare` start browser under the holding node. Initial fill
happens at boot (after the window/CEF are up).

## Correctness requirements

- **Fresh data on reveal (load-bearing).** The start page emits `StartDataRequest` on mount
  and the host answers with the command-bar payload (`on_start_data_request`,
  `crates/vmux_layout/src/start/plugin.rs:59`). A spare loaded at boot captured *boot-time*
  tabs/spaces/history. On claim, the host must re-build and push the current payload to the
  spare entity (reuse the `on_start_data_request` builder) so the launcher is not stale.
- **Input focus on reveal.** Send `StartFocusInput` to the spare on claim so the launcher
  input is focused, matching today's cold-open behavior.
- **Clear before reparent, never after.** `clear_stack_children` must run on the target
  stack *before* the spare becomes its child; it must never run against a stack that already
  holds a live spare (it would despawn the spare → close the browser via the
  `WebviewSource` `on_despawn` hook, `patches/bevy_cef-0.5.2/src/webview.rs:155`).
- **Single-window assumption.** Spares have no `HostWindow` and bind to `primary_window`;
  this serves every space (spaces are sibling UI nodes under `Main`, not OS windows). If
  multi-window is ever introduced, the pool must become per-window.

## Idle cost

Near-zero: a parked spare does not composite (hidden view), so it costs no steady-state
GPU/CPU beyond the one-time load. The start hero has no infinite CSS animations anyway (only
a one-shot 700ms mount transition, `crates/vmux_layout/src/start/page.rs:37`). This is the
key reason for the non-compositing parking mode over the command-bar keep-painting trick.

## Risks & mitigations

| Risk | Mitigation |
|------|-----------|
| Reparent mis-positions or recreates the browser | Spike-confirmed sound: position recomputed every frame from live parent rect (`sync_windowed_frames`, `crates/vmux_browser/src/lib.rs:1168`); no teardown on `ChildOf` change. Keep spare in User/windowed backend. |
| Visible flash on reveal (page wasn't compositing while parked) | Reveal via existing 2-frame `webview_reveal.rs` path; if still flashing, switch the parking mode to keep-compositing. |
| Empty pool on back-to-back opens | Default pool size 1; bump constant to 2. Cold fallback covers the miss with no regression. |
| Stale launcher data | Push fresh `StartData` payload on claim (required, above). |
| Spare swept by stack/focus/layout systems | `WarmStartSpare` excluded from content sync queries; parked under non-stack holding node. |
| First startup tab still cold | Accepted (one-time); pool warms immediately after for all subsequent opens. |

## Testing

ECS message+system integration tests (no real CEF needed):

- **Boot fill:** after running the refill schedule, `WARM_START_POOL_SIZE` entities carry
  `WarmStartSpare`.
- **Claim reuses spare:** with a claimable spare present, sending a start `PageOpenTask`
  reparents the spare to `task.stack` (assert `ChildOf` == stack), sets the stack's
  `PageMetadata` to start, inserts `PageOpenHandled`, and does **not** emit a
  `CefPageAttachRequest`.
- **Empty pool falls back:** with no claimable spare, the same task emits a
  `CefPageAttachRequest` (existing cold path) — no panic, no regression.
- **Refill:** after a claim, the spare count is restored to target on the next refill run.
- **Fresh data on claim:** a claim triggers a fresh start-data push to the revealed entity.

Register any new message types in the owning plugin's `build()` (idempotent) so
`cargo test --workspace` passes. Runtime/visual verification (actual instant paint) is a
single manual pass at the end by the user.

## Files to touch

- `crates/vmux_layout/src/start/plugin.rs` — pool resource, boot fill, refill system, claim
  hook in `handle_start_page_open`, `WarmStartSpare` marker, fresh-data push + focus on claim.
- `crates/vmux_layout/src/start.rs` / `start/event.rs` — any new marker/message exports.
- `crates/vmux_browser/src/lib.rs` — add `Without<WarmStartSpare>` to content
  visibility/focus/frame sync queries; expose/share `clear_stack_children`.
- `crates/vmux_layout/src/cef.rs` — spare `Browser` bundle helper (reuse `Browser::new`),
  parked node styling.
