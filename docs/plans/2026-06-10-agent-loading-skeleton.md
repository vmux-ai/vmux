# Agent Startup Loading Skeleton Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Cover the empty terminal grid with a centered loading skeleton while a CLI agent (`vibe`/`claude`/`codex`) starts, dismissing it when the agent paints its full-screen TUI (alt-screen) or after a timeout.

**Architecture:** The service already tracks alacritty's `ALT_SCREEN` mode; surface it on `ServiceMessage::TerminalMode`. The host stores it in `TerminalModeMap`, marks agent terminals with an `AgentLoading` component on page-ready (emitting `TermLoadingEvent { loading: true }`), and clears it on alt-screen entry or a 10s timeout (emitting `loading: false`). The CEF terminal page renders a `pointer-events-none` skeleton overlay while loading, so input still passes through to the PTY but the grid (and its echo) is hidden.

**Tech Stack:** Rust, Bevy ECS, alacritty_terminal, rkyv (host↔CEF bin events), Dioxus (CEF page), Tailwind.

**Spec:** `docs/specs/2026-06-10-agent-loading-skeleton-design.md`

---

## File Structure

- `crates/vmux_service/src/protocol.rs` — add `alt_screen` to `ServiceMessage::TerminalMode`.
- `crates/vmux_service/src/process.rs` — compute + broadcast `alt_screen`; unit test.
- `crates/vmux_core/src/event.rs` — new `TermLoadingEvent` + `TERM_LOADING_EVENT`; rkyv round-trip test. (Re-exported to `vmux_terminal::event` via the existing `pub use vmux_core::event::*;`.)
- `crates/vmux_terminal/src/plugin.rs` — `TerminalModeFlags.alt_screen`; `AgentLoading` component; `arm_agent_loading` + `clear_agent_loading` systems; registration; `ProcessExited` cleanup; Bevy unit tests.
- `crates/vmux_terminal/src/page.rs` — `loading` signal, `TERM_LOADING_EVENT` listener, skeleton overlay; gate the existing rows-empty "Loading…" on not-loading.

---

## Task 1: Service surfaces alt-screen on `TerminalMode`

**Files:**
- Modify: `crates/vmux_service/src/protocol.rs` (the `TerminalMode` variant)
- Modify: `crates/vmux_service/src/process.rs:98` (field), `:525-538` (`maybe_broadcast_mode`)
- Modify (consumer, keeps workspace compiling): `crates/vmux_terminal/src/plugin.rs:144-147` (`TerminalModeFlags`), `:1179-1192` (consume), `:3387-3390` (test literal)
- Test: `crates/vmux_service/src/process.rs` (`#[cfg(test)] mod tests`)

- [ ] **Step 1: Write the failing test** — append inside the `mod tests` block in `crates/vmux_service/src/process.rs` (e.g. after `copy_mode_up_at_alt_screen_top_uses_mouse_wheel_scroll`):

```rust
    #[test]
    fn terminal_mode_broadcasts_alt_screen_toggle() {
        let (wake_tx, _) = mpsc::unbounded_channel();
        let mut process = Process::new_with_wake(
            ProcessId::new(),
            "/bin/sh".to_string(),
            vec![],
            String::new(),
            Vec::new(),
            12,
            8,
            wake_tx,
        )
        .expect("process should spawn");

        let mut rx = process.subscribe();

        process.process_output_for_test(b"\x1b[?1049h");
        process.maybe_broadcast_mode();

        let mut alt_on = None;
        while let Ok(msg) = rx.try_recv() {
            if let ServiceMessage::TerminalMode { alt_screen, .. } = msg {
                alt_on = Some(alt_screen);
            }
        }
        assert_eq!(alt_on, Some(true), "entering alt screen broadcasts alt_screen=true");

        process.process_output_for_test(b"\x1b[?1049l");
        process.maybe_broadcast_mode();

        let mut alt_off = None;
        while let Ok(msg) = rx.try_recv() {
            if let ServiceMessage::TerminalMode { alt_screen, .. } = msg {
                alt_off = Some(alt_screen);
            }
        }
        assert_eq!(alt_off, Some(false), "leaving alt screen broadcasts alt_screen=false");

        process.kill();
    }
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p vmux_service terminal_mode_broadcasts_alt_screen_toggle`
Expected: FAIL — compile error, `ServiceMessage::TerminalMode` has no field `alt_screen`.

- [ ] **Step 3: Add the field to the protocol** — in `crates/vmux_service/src/protocol.rs`, find:

```rust
    TerminalMode {
        process_id: ProcessId,
        mouse_capture: bool,
        copy_mode: bool,
    },
```

Replace with:

```rust
    TerminalMode {
        process_id: ProcessId,
        mouse_capture: bool,
        copy_mode: bool,
        alt_screen: bool,
    },
```

- [ ] **Step 4: Compute and broadcast it** — in `crates/vmux_service/src/process.rs`:

Change the field at line 98 from:

```rust
    last_terminal_mode: Option<(bool, bool)>,
```

to:

```rust
    last_terminal_mode: Option<(bool, bool, bool)>,
```

Replace `maybe_broadcast_mode` (lines ~524-538) with:

```rust
    fn maybe_broadcast_mode(&mut self) {
        use alacritty_terminal::term::TermMode;
        let mouse_capture = self.term.mode().intersects(TermMode::MOUSE_MODE);
        let copy_mode = self.copy_mode.is_some();
        let alt_screen = self.term.mode().contains(TermMode::ALT_SCREEN);
        let cur = (mouse_capture, copy_mode, alt_screen);
        if self.last_terminal_mode != Some(cur) {
            self.last_terminal_mode = Some(cur);
            let _ = self.patch_tx.send(ServiceMessage::TerminalMode {
                process_id: self.id,
                mouse_capture,
                copy_mode,
                alt_screen,
            });
        }
    }
```

- [ ] **Step 5: Update the host consumer so the workspace compiles** — in `crates/vmux_terminal/src/plugin.rs`:

Add the field to `TerminalModeFlags` (lines 143-147):

```rust
#[derive(Default, Clone, Copy, Debug)]
pub struct TerminalModeFlags {
    pub mouse_capture: bool,
    pub copy_mode: bool,
    pub alt_screen: bool,
}
```

Update the `TerminalMode` match arm (lines ~1179-1192) to:

```rust
            ServiceMessage::TerminalMode {
                process_id,
                mouse_capture,
                copy_mode,
                alt_screen,
            } => {
                mode_map.modes.insert(
                    process_id,
                    TerminalModeFlags {
                        mouse_capture,
                        copy_mode,
                        alt_screen,
                    },
                );
                set_local_copy_mode(&mut local_copy_mode, process_id, copy_mode);
            }
```

Update the test literal at lines ~3387-3390 from:

```rust
            TerminalModeFlags {
                mouse_capture: false,
                copy_mode: false,
            },
```

to:

```rust
            TerminalModeFlags {
                mouse_capture: false,
                copy_mode: false,
                alt_screen: false,
            },
```

- [ ] **Step 6: Run the test to verify it passes**

Run: `cargo test -p vmux_service terminal_mode_broadcasts_alt_screen_toggle`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/vmux_service/src/protocol.rs crates/vmux_service/src/process.rs crates/vmux_terminal/src/plugin.rs
git commit -m "feat(service): broadcast alt-screen state on TerminalMode"
```

---

## Task 2: Core `TermLoadingEvent`

**Files:**
- Modify: `crates/vmux_core/src/event.rs` (constants block near line 5; new struct; test in `mod tests`)
- Test: `crates/vmux_core/src/event.rs`

- [ ] **Step 1: Write the failing test** — append inside the `mod tests` block in `crates/vmux_core/src/event.rs` (after `term_title_event_rkyv_roundtrip`):

```rust
    #[test]
    fn term_loading_event_rkyv_roundtrip() {
        let original = TermLoadingEvent {
            loading: true,
            label: "Vibe".to_string(),
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&original).expect("serialize");
        let recovered =
            rkyv::from_bytes::<TermLoadingEvent, rkyv::rancor::Error>(&bytes).expect("deserialize");
        assert_eq!(original.loading, recovered.loading);
        assert_eq!(original.label, recovered.label);
    }
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p vmux_core term_loading_event_rkyv_roundtrip`
Expected: FAIL — `TermLoadingEvent` not found.

- [ ] **Step 3: Add the constant** — in `crates/vmux_core/src/event.rs`, after the other `TERM_*_EVENT` constants (the line `pub const TERM_TITLE_EVENT: &str = "term_title";`), add:

```rust
pub const TERM_LOADING_EVENT: &str = "term_loading";
```

- [ ] **Step 4: Add the struct** — in `crates/vmux_core/src/event.rs`, after the `TermThemeEvent` struct definition, add:

```rust
#[derive(
    Debug, Clone, Serialize, Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub struct TermLoadingEvent {
    pub loading: bool,
    pub label: String,
}
```

- [ ] **Step 5: Run the test to verify it passes**

Run: `cargo test -p vmux_core term_loading_event_rkyv_roundtrip`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_core/src/event.rs
git commit -m "feat(core): add TermLoadingEvent for agent startup overlay"
```

---

## Task 3: Host loading lifecycle

**Files:**
- Modify: `crates/vmux_terminal/src/plugin.rs` — new component + const + two systems + registration + `ProcessExited` cleanup; Bevy unit tests in `mod tests`.

Note: `crate::event::*` is already imported (line 33), so `TermLoadingEvent` and `TERM_LOADING_EVENT` are in scope. `BinHostEmitEvent`, `vmux_core::agent::{AgentSession, AgentKind}`, `ProcessId`, `Terminal`, and `vmux_core::page::PageReady` are already imported/used in this file.

- [ ] **Step 1: Add the component and timeout constant** — in `crates/vmux_terminal/src/plugin.rs`, just below the `TerminalModeFlags` struct (after line ~147), add:

```rust
const AGENT_LOADING_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

#[derive(Component, Debug, Clone, Copy)]
pub struct AgentLoading {
    pub since: Instant,
}
```

(`Instant` is already imported — it is used by `TerminalWebShortcutState` at line 98.)

- [ ] **Step 2: Add the two systems** — in `crates/vmux_terminal/src/plugin.rs`, add these free functions near the other terminal systems (e.g. just above `fn on_term_ready`):

```rust
fn arm_agent_loading(
    newly_ready: Query<
        (Entity, &vmux_core::agent::AgentSession),
        (With<Terminal>, Added<PageReady>),
    >,
    mut commands: Commands,
) {
    for (entity, session) in &newly_ready {
        commands.entity(entity).insert(AgentLoading {
            since: Instant::now(),
        });
        commands.trigger(BinHostEmitEvent::from_rkyv(
            entity,
            TERM_LOADING_EVENT,
            &crate::event::TermLoadingEvent {
                loading: true,
                label: session.kind.display_name().to_string(),
            },
        ));
    }
}

fn clear_agent_loading(
    loading_q: Query<
        (Entity, &ProcessId, &vmux_core::agent::AgentSession, &AgentLoading),
        With<Terminal>,
    >,
    mode_map: Res<TerminalModeMap>,
    mut commands: Commands,
) {
    for (entity, pid, session, loading) in &loading_q {
        let alt_screen = mode_map
            .modes
            .get(pid)
            .map(|m| m.alt_screen)
            .unwrap_or(false);
        if alt_screen || loading.since.elapsed() >= AGENT_LOADING_TIMEOUT {
            commands.entity(entity).remove::<AgentLoading>();
            commands.trigger(BinHostEmitEvent::from_rkyv(
                entity,
                TERM_LOADING_EVENT,
                &crate::event::TermLoadingEvent {
                    loading: false,
                    label: session.kind.display_name().to_string(),
                },
            ));
        }
    }
}
```

- [ ] **Step 3: Register the systems** — in `crates/vmux_terminal/src/plugin.rs`, in `impl Plugin for TerminalPlugin`'s `build`, add an `add_systems` call (place after the existing `.init_resource::<TerminalModeMap>()` chain / alongside other `Update` registrations):

```rust
        app.add_systems(Update, (arm_agent_loading, clear_agent_loading.after(poll_service_messages)));
```

- [ ] **Step 4: Clear loading on process exit** — in `crates/vmux_terminal/src/plugin.rs`, inside the `ServiceMessage::ProcessExited` arm (the entity loop near line 1120-1135), add `.remove::<AgentLoading>()` to the entity-command chain that already does `.insert(ProcessExited).remove::<CloseRequiresConfirmation>()`:

```rust
                        commands
                            .entity(entity)
                            .insert(ProcessExited)
                            .remove::<CloseRequiresConfirmation>()
                            .remove::<AgentLoading>();
```

- [ ] **Step 5: Write the failing tests** — append to the `#[cfg(test)] mod tests` block in `crates/vmux_terminal/src/plugin.rs`. Ensure these imports are available in the test module (add any missing to its `use` lines): `use super::*;`, `use vmux_core::agent::{AgentSession, AgentKind};`, `use vmux_core::page::PageReady;`, `use std::time::{Duration, Instant};`.

```rust
    #[test]
    fn agent_terminal_armed_loading_on_page_ready() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(Update, arm_agent_loading);
        let e = app
            .world_mut()
            .spawn((
                Terminal,
                AgentSession {
                    kind: AgentKind::Vibe,
                },
                PageReady,
            ))
            .id();
        app.update();
        assert!(app.world().get::<AgentLoading>(e).is_some());
    }

    #[test]
    fn agent_loading_cleared_when_alt_screen_active() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<TerminalModeMap>()
            .add_systems(Update, clear_agent_loading);
        let pid = ProcessId::new();
        let e = app
            .world_mut()
            .spawn((
                Terminal,
                AgentSession {
                    kind: AgentKind::Vibe,
                },
                pid,
                AgentLoading {
                    since: Instant::now(),
                },
            ))
            .id();
        app.world_mut()
            .resource_mut::<TerminalModeMap>()
            .modes
            .insert(
                pid,
                TerminalModeFlags {
                    mouse_capture: false,
                    copy_mode: false,
                    alt_screen: true,
                },
            );
        app.update();
        assert!(app.world().get::<AgentLoading>(e).is_none());
    }

    #[test]
    fn agent_loading_cleared_after_timeout() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<TerminalModeMap>()
            .add_systems(Update, clear_agent_loading);
        let pid = ProcessId::new();
        let e = app
            .world_mut()
            .spawn((
                Terminal,
                AgentSession {
                    kind: AgentKind::Vibe,
                },
                pid,
                AgentLoading {
                    since: Instant::now() - AGENT_LOADING_TIMEOUT - Duration::from_secs(1),
                },
            ))
            .id();
        app.update();
        assert!(app.world().get::<AgentLoading>(e).is_none());
    }

    #[test]
    fn agent_loading_retained_while_starting() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<TerminalModeMap>()
            .add_systems(Update, clear_agent_loading);
        let pid = ProcessId::new();
        let e = app
            .world_mut()
            .spawn((
                Terminal,
                AgentSession {
                    kind: AgentKind::Vibe,
                },
                pid,
                AgentLoading {
                    since: Instant::now(),
                },
            ))
            .id();
        app.update();
        assert!(app.world().get::<AgentLoading>(e).is_some());
    }
```

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test -p vmux_terminal agent_loading agent_terminal_armed_loading_on_page_ready`
Expected: PASS (4 tests). If `arm_agent_loading`/`clear_agent_loading` are reported unused outside `cfg(test)`, that is resolved by Step 3 registration — confirm the registration is present.

- [ ] **Step 7: Commit**

```bash
git add crates/vmux_terminal/src/plugin.rs
git commit -m "feat(terminal): drive agent loading state from page-ready to alt-screen/timeout"
```

---

## Task 4: Terminal page skeleton overlay

**Files:**
- Modify: `crates/vmux_terminal/src/page.rs` (signals near lines 24-31; listeners near 38-111; rsx overlay near 227-250)

This is the CEF/Dioxus (wasm) page; it is verified manually (Task 5, Step 2), not by a unit test.

- [ ] **Step 1: Add the loading signal** — in `crates/vmux_terminal/src/page.rs`, with the other `use_signal` declarations (after `let mut service_error = use_signal(String::new);` near line 31), add:

```rust
    let mut loading = use_signal(|| None::<String>);
```

- [ ] **Step 2: Add the event listener** — in `crates/vmux_terminal/src/page.rs`, alongside the other `use_bin_event_listener` calls (e.g. after the `_title_listener` block near line 111), add:

```rust
    let _loading_listener =
        use_bin_event_listener::<TermLoadingEvent, _>(TERM_LOADING_EVENT, move |evt| {
            loading.set(if evt.loading { Some(evt.label) } else { None });
        });
```

- [ ] **Step 3: Gate the existing rows-empty overlay** — in `crates/vmux_terminal/src/page.rs`, change the `waiting` computation (near line 242) from:

```rust
                let waiting = rows.read().is_empty() && service_error.read().is_empty();
```

to:

```rust
                let waiting = rows.read().is_empty()
                    && service_error.read().is_empty()
                    && loading.read().is_none();
```

- [ ] **Step 4: Render the skeleton overlay** — in `crates/vmux_terminal/src/page.rs`, immediately after the `waiting` overlay block (the closing `})` of the `waiting.then(...)` expression, before the grid `div { style: "padding:..." }`), add:

```rust
            {
                let label = loading.read().clone();
                label.map(|label| rsx! {
                    div {
                        class: "absolute inset-0 z-40 flex flex-col items-center justify-center pointer-events-none bg-term-bg",
                        div {
                            class: "mb-3 text-sm",
                            style: "color:var(--term-fg);opacity:0.75;",
                            "{label}"
                        }
                        div {
                            class: "h-2 w-40 rounded-md animate-pulse",
                            style: "background:var(--term-fg);opacity:0.12;",
                        }
                        div {
                            class: "mt-2 text-xs",
                            style: "color:var(--term-fg);opacity:0.4;",
                            "starting…"
                        }
                    }
                })
            }
```

- [ ] **Step 5: Verify it compiles**

Run: `cargo check -p vmux_terminal`
Expected: PASS (no errors). `TermLoadingEvent` and `TERM_LOADING_EVENT` resolve via the existing `use crate::event::*;` at the top of `page.rs`.

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_terminal/src/page.rs
git commit -m "feat(terminal): render centered skeleton while agent starts"
```

---

## Task 5: Final verification

**Files:** none (verification only).

- [ ] **Step 1: Format, lint, and run the affected crate tests**

Run:
```bash
cargo fmt
cargo clippy -p vmux_service -p vmux_core -p vmux_terminal --all-targets -- -D warnings
cargo test -p vmux_service -p vmux_core -p vmux_terminal
```
Expected: fmt clean, clippy clean, all tests PASS. Fix any issue before continuing (do not commit broken code).

- [ ] **Step 2: Manual UI check** — build/run the desktop app, open `vmux://agent/vibe/` (repeat for `claude`, `codex`). Confirm:
  - A centered skeleton (agent name + pulsing bar + "starting…") covers the grid immediately.
  - No empty grid and no keyboard/mouse echo is visible during startup.
  - The skeleton disappears the moment the agent's TUI paints (alt-screen), revealing the live UI.
  - A non-agent terminal (plain shell tab) shows no skeleton (unchanged behavior).
  - Mouse clicks during loading still land on the underlying terminal (pass-through).

- [ ] **Step 3: Commit any fmt/clippy fixups**

```bash
git add -A
git commit -m "chore(terminal): fmt/clippy fixups for agent loading skeleton"
```

(Skip this commit if Step 1 produced no changes.)

---

## Self-Review

- **Spec coverage:** alt-screen detection → Task 1. `TermLoadingEvent` → Task 2. Host loading state (page-ready arm, alt-screen/timeout clear, exit cleanup, all CLI agents via `AgentSession` + `kind.display_name()`) → Task 3. Page skeleton + input pass-through (`pointer-events-none`) + hiding the empty grid → Task 4. Tests (service unit, core rkyv, host Bevy) + manual page check → Tasks 1-5. All spec sections mapped.
- **Placeholder scan:** none — every code/command step is concrete.
- **Type consistency:** `TerminalModeFlags { mouse_capture, copy_mode, alt_screen }` consistent across producer (Task 1 process), consumer (Task 1 host), and tests (Tasks 1, 3). `TermLoadingEvent { loading: bool, label: String }` + `TERM_LOADING_EVENT` consistent across core (Task 2), host emit (Task 3), page listener (Task 4). `AgentLoading { since: Instant }` and `AGENT_LOADING_TIMEOUT` consistent across Task 3.
