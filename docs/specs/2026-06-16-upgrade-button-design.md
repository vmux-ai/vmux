# Upgrade Button + `vmux://debug` — Design

Date: 2026-06-16

## Problem

The auto-updater works end-to-end (verified: live manifest, published assets, signature
cryptographically valid against the app's baked-in pubkey) but is **invisible**. Every
outcome is silent — `NoUpdate`/`Failed` log at `debug!` (filtered out below the default
`info` level), `Installed` logs `info!` to stdout only, and there is no UI. Updates apply
on the next launch with no prompt, so users cannot tell the feature works or that a new
version is ready.

## Goal

Discord-style update UX:

- **Startup + runtime:** keep silent background download + install (unchanged).
- **When an install is staged on disk:** show a footer at the bottom of the left side
  sheet — `● New version available · vX.Y.Z · [Restart to update]`. Clicking relaunches
  into the new version immediately (already downloaded).
- **`vmux://debug`:** an internal page to emulate the update-ready and restart flows on
  demand, so the behavior is testable without cutting a real release.

Non-goals: changing download/verify/install internals; surfacing update state anywhere
other than the side-sheet footer (YAGNI); progress UI (download is silent/background).

## Architecture

Mirrors the existing typed-rkyv IPC bridge used by `tab.rs` / `page.rs`:

- **ECS → UI:** `BinEventEmitterPlugin::<(T,)>` + emit.
- **UI → ECS:** `try_cef_bin_emit_rkyv(&T)` from the Dioxus page; `On<BinReceive<T>>`
  observer on the ECS side.
- **Event types:** rkyv-derived, defined in `vmux_core::event` (new `event/update.rs`),
  visible to both `vmux_desktop` (updater, producer/consumer) and `vmux_layout` (page.rs
  footer + debug page). `vmux_desktop` already depends on both `vmux_core` and
  `vmux_layout`.

### Events (`vmux_core::event::update`)

| Event | Direction | Payload | Meaning |
|-------|-----------|---------|---------|
| `UpdateReadyEvent` | ECS → UI | `version: String` | An install is staged; show footer |
| `UpdateClearedEvent` | ECS → UI | — | Hide footer |
| `RestartRequestEvent` | UI → ECS | — | User clicked Restart; relaunch |
| `DebugUpdateReady` | UI → ECS | `version: String` | Debug: simulate staged install |
| `DebugUpdateClear` | UI → ECS | — | Debug: clear update state |

All are plain IPC events; harmless in release. `vmux://debug` is always available (like
the other `vmux://` pages); no feature gate.

## Components

### 1. `updater.rs` (vmux_desktop)

- New resource `UpdateState { Idle | Ready { version: String } }`.
- On `UpdateResult::Installed { version }`: set `UpdateState::Ready { version }`, keep
  `done = true`, and emit `UpdateReadyEvent { version }` (replaces the silent `info!`-only
  path). The existing silent-install behavior is otherwise untouched.
- Register `BinEventEmitterPlugin::<(UpdateReadyEvent, UpdateClearedEvent)>`.
- Observers:
  - `On<BinReceive<RestartRequestEvent>>` → relaunch (see below).
  - `On<BinReceive<DebugUpdateReady>>` → set `Ready { version }` + emit `UpdateReadyEvent`
    (drives the real ECS→UI path).
  - `On<BinReceive<DebugUpdateClear>>` → set `Idle` + emit `UpdateClearedEvent`.

**Relaunch (macOS):**
- App bundle = `current_exe()` then `.ancestors().nth(3)` (`…/Vmux.app/Contents/MacOS/vmux_desktop` → `…/Vmux.app`).
- If the exe **is** inside a `.app`: spawn a detached
  `sh -c 'while kill -0 <pid> 2>/dev/null; do sleep 0.2; done; open "<app>"'`, then send
  `AppExit::Success`. The waiter relaunches the (already-replaced) bundle after the current
  process exits.
- If the exe is **not** inside a `.app` (dev / `cargo run`): do **not** exit. Log the
  command that would run (`info!`). Makes `vmux://debug` testable in dev without killing
  the session.
- Relaunch command construction is factored behind a small seam (e.g. a function returning
  the `Command`/args) so it can be unit-tested without spawning or exiting.

### 2. Side-sheet footer (`vmux_layout/src/page.rs`)

- `use_bin_event_listener::<UpdateReadyEvent>` → `use_signal(Option<String>)` (the version);
  `use_bin_event_listener::<UpdateClearedEvent>` → reset to `None`.
- When `Some(version)`, render a pinned footer at the **bottom of the left side sheet**:
  status dot + "New version available" + `version` + a `vmux_ui` Button "Restart to update".
- Button `onclick` → `try_cef_bin_emit_rkyv(&RestartRequestEvent)`.
- Visible only while the side sheet is open (its chosen placement; no other surface).

### 3. `vmux://debug` page

- New wasm page module `vmux_layout::debug_page` exposing `Page` (Dioxus), mirroring
  `vmux_space::page` / `vmux_service::page`.
- Register in the `vmux_server` `web_pages!` macro: `render_debug: "debug" => vmux_layout::debug_page::Page`.
- Spawn `PageManifest { host: "debug" }` natively where the other manifests are registered,
  so `vmux://debug/` routes like `vmux://spaces/`.
- Controls:
  - Text input for version (default e.g. `v99.0.0`).
  - **Simulate update available** → `try_cef_bin_emit_rkyv(&DebugUpdateReady { version })`
    → footer appears in the side sheet.
  - **Clear update** → `try_cef_bin_emit_rkyv(&DebugUpdateClear)` → footer disappears.
  - **Trigger restart** → `try_cef_bin_emit_rkyv(&RestartRequestEvent)` → exercises the
    relaunch path (real relaunch on installed builds; logged no-op in dev).

## Data flow

Real: `poll` → `download_and_install` Ok → `UpdateState::Ready` + emit `UpdateReadyEvent`
→ footer signal set → footer renders → click → `RestartRequestEvent` → observer → spawn
relauncher + `AppExit`.

Debug: open `vmux://debug` → "Simulate update available" → `DebugUpdateReady` → ECS sets
`Ready` + emits `UpdateReadyEvent` → footer renders (identical to real path) → "Restart to
update" (or "Trigger restart") → `RestartRequestEvent` → relaunch.

## Edge cases

- **Dev builds:** real installs never succeed (no/empty pubkey) → footer only appears via
  `vmux://debug`. Restart logs instead of exiting. Both safe and observable.
- **Side sheet closed:** footer not visible. Accepted (chosen placement).
- **Background install failure:** no footer (silent, as today). Out of scope to surface.
- **Repeated polls after a real install:** running version stays old until relaunch, so
  later polls still see remote > current; `done = true` already prevents re-install, and
  `UpdateState::Ready` is idempotent. Footer keeps showing the staged version.

## Testing

- `UpdateState` transition: `Installed { version }` → `Ready { version }` + `UpdateReadyEvent`
  queued.
- `DebugUpdateReady` observer → `Ready` + `UpdateReadyEvent`; `DebugUpdateClear` → `Idle` +
  `UpdateClearedEvent`.
- Relaunch seam: inside-`.app` path builds the expected `open`/waiter command; outside-`.app`
  path returns the log-only branch (no exit). Pure unit test, no spawn.
- UI: `UpdateReadyEvent` sets the footer signal → footer node present; click emits
  `RestartRequestEvent`. `UpdateClearedEvent` clears it.
- Manual (`vmux://debug`): simulate → footer appears → restart → relaunch (installed) or
  logged command (dev).

## Scope

New: `vmux_core::event::update` (5 events), `vmux_layout::debug_page`, one side-sheet footer,
`updater.rs` state + observers + relaunch seam, one `web_pages!` entry, one `PageManifest`.
No changes to the updater's download/verify/install internals.
