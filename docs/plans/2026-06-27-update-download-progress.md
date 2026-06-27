# Update Download Progress Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. NOTE: do NOT subagent-drive — CEF builds are huge and long agents drop sockets; implement inline with a warm target dir.

**Goal:** Show the update card during download (with a progress bar) and install, then "Restart to update" — instead of only after install completes.

**Architecture:** The desktop updater downloads via `cargo-packager-updater`'s `download_and_install_extended`, sending throttled byte-progress over its existing mpsc channel and waking the winit loop each step. `poll_update_result` maps messages onto a new `UpdateState` enum resource (replacing `StagedUpdate`). `vmux_browser` emits `UpdateProgressEvent` / `UpdateReadyEvent` / `UpdateClearedEvent` to the layout page over the rkyv bin bridge. The Dioxus page renders Downloading → Installing → Ready in the same `UpdateNoticeFooter` card.

**Tech Stack:** Rust, Bevy 0.19-rc, bevy_cef bin events (rkyv), Dioxus (wasm) page, Tailwind v4 CSS, `cargo-packager-updater 0.2.3`.

---

## File Structure

- `crates/vmux_layout/src/event.rs` — add `UpdateProgressEvent` + `UPDATE_PROGRESS_EVENT` + `DebugSimulateDownload` (shared wasm/native wire types).
- `crates/vmux_layout/src/lib.rs` — replace `StagedUpdate(Option<String>)` with `UpdateState` enum (native-only resource).
- `crates/vmux_layout/src/plugin.rs` — init `UpdateState`.
- `crates/vmux_layout/src/page.rs` — `UpdatePhase` signal + listeners; rewrite `UpdateNoticeFooter`; add `UpdateProgressBar` + `download_pct` helper.
- `crates/vmux_browser/src/lib.rs` — `push_update_notice_emit` reads `UpdateState`, emits 3 events; `should_emit_update` helper; debug observers; debug emitter tuple; tests.
- `crates/vmux_desktop/src/updater.rs` — real progress (`download_and_install_extended`), wake, `progress_step` throttle, drain-restructure, `Failed`→`Idle`, debug-sim observer/thread.
- `crates/vmux_desktop/src/debug_page.rs` … (none) — debug button lives in layout's debug page.
- `crates/vmux_layout/src/debug_page.rs` — "Simulate download" button.
- `crates/vmux_server/assets/index.css` — indeterminate-bar keyframe + class.

Dependency order: Task 1 (events) → Task 2 (resource migration, keeps current behavior, workspace stays green) → Task 3 (updater progress) → Task 4 (page UI) → Task 5 (CSS) → Task 6 (debug sim) → Task 7 (verify).

**Warm the build first** (CEF is huge; first compile is long):

```bash
cd .worktrees/update-progress && cargo build -p vmux_desktop
```

Run that in the background before starting so later `cargo test` calls are incremental.

---

## Task 1: Wire events (vmux_layout/event.rs)

**Files:**
- Modify: `crates/vmux_layout/src/event.rs` (add const + struct near line 492; tests near 581)

- [ ] **Step 1: Write failing tests**

In `mod update_event_tests` (after `update_ready_event_rkyv_round_trips`), add:

```rust
    #[test]
    fn update_progress_event_rkyv_round_trips() {
        let evt = UpdateProgressEvent {
            version: "0.0.20".to_string(),
            downloaded: 42,
            total: 100,
            installing: false,
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&evt).unwrap();
        let back = rkyv::from_bytes::<UpdateProgressEvent, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(back.version, "0.0.20");
        assert_eq!(back.downloaded, 42);
        assert_eq!(back.total, 100);
        assert!(!back.installing);
    }
```

And extend `event_ids_are_stable`:

```rust
        assert_eq!(UPDATE_PROGRESS_EVENT, "update-progress");
```

- [ ] **Step 2: Run — expect FAIL (unresolved `UpdateProgressEvent` / `UPDATE_PROGRESS_EVENT`)**

```bash
cargo test -p vmux_layout --lib update_event_tests
```

Expected: compile error, names not found.

- [ ] **Step 3: Add the const + struct**

After line 492 (`pub const UPDATE_CLEARED_EVENT...`) add:

```rust
pub const UPDATE_PROGRESS_EVENT: &str = "update-progress";
```

After the `UpdateReadyEvent` struct (line ~506) add:

```rust
#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct UpdateProgressEvent {
    pub version: String,
    pub downloaded: u64,
    pub total: u64,
    pub installing: bool,
}
```

After the `DebugUpdateClear` struct (line ~565) add:

```rust
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct DebugSimulateDownload;
```

- [ ] **Step 4: Run — expect PASS**

```bash
cargo test -p vmux_layout --lib update_event_tests
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_layout/src/event.rs
git commit -m "feat(layout): add update progress + debug-download wire events"
```

---

## Task 2: UpdateState resource migration (keeps current behavior)

Replaces `StagedUpdate(Option<String>)` with an `UpdateState` enum across the three consumers. Behavior is unchanged after this task (no progress yet): updater still uses plain `download_and_install`, the card still only shows on Ready.

**Files:**
- Modify: `crates/vmux_layout/src/lib.rs:154-156`
- Modify: `crates/vmux_layout/src/plugin.rs:32`
- Modify: `crates/vmux_browser/src/lib.rs` (import line 39; `should_emit_update_notice` 2844-2850; `push_update_notice_emit` 2852-2883; debug observers 2885-2897; tests 5833-5862 and 5864-5893)
- Modify: `crates/vmux_desktop/src/updater.rs:228` and the `Installed` arm 247-252

- [ ] **Step 1: Define `UpdateState`**

Replace `crates/vmux_layout/src/lib.rs:154-156`:

```rust
#[cfg(not(target_arch = "wasm32"))]
#[derive(Resource, Default, Clone, PartialEq, Debug)]
pub enum UpdateState {
    #[default]
    Idle,
    Downloading {
        version: String,
        downloaded: u64,
        total: u64,
    },
    Installing {
        version: String,
    },
    Ready {
        version: String,
    },
}
```

- [ ] **Step 2: Init it in the plugin**

`crates/vmux_layout/src/plugin.rs:32`: replace

```rust
            .init_resource::<crate::StagedUpdate>()
```

with

```rust
            .init_resource::<crate::UpdateState>()
```

- [ ] **Step 3: Migrate the browser emit system + helper**

In `crates/vmux_browser/src/lib.rs`, change the import on line 39 from `StagedUpdate` to `UpdateState`, and add `UPDATE_PROGRESS_EVENT, UpdateProgressEvent` to the event imports (lines 44-45 group).

Replace `should_emit_update_notice` (2844-2850) with:

```rust
fn should_emit_update(
    current: &UpdateState,
    last: &Option<UpdateState>,
    page_ready_changed: bool,
) -> bool {
    last.as_ref() != Some(current)
        || (page_ready_changed && *current != UpdateState::Idle)
}
```

Replace `push_update_notice_emit` (2852-2883) with:

```rust
fn push_update_notice_emit(
    mut commands: Commands,
    browsers: NonSend<Browsers>,
    cef_q: Query<(Entity, Ref<PageReady>), With<LayoutCef>>,
    state: Res<UpdateState>,
    mut last: Local<Option<UpdateState>>,
) {
    let Ok((cef_e, page_ready)) = cef_q.single() else {
        return;
    };
    if !browsers.has_browser(cef_e) || !browsers.host_emit_ready(&cef_e) {
        return;
    }
    if !should_emit_update(&state, &last, page_ready.is_changed()) {
        return;
    }
    match &*state {
        UpdateState::Idle => commands.trigger(BinHostEmitEvent::from_rkyv(
            cef_e,
            UPDATE_CLEARED_EVENT,
            &UpdateClearedEvent,
        )),
        UpdateState::Downloading {
            version,
            downloaded,
            total,
        } => commands.trigger(BinHostEmitEvent::from_rkyv(
            cef_e,
            UPDATE_PROGRESS_EVENT,
            &UpdateProgressEvent {
                version: version.clone(),
                downloaded: *downloaded,
                total: *total,
                installing: false,
            },
        )),
        UpdateState::Installing { version } => commands.trigger(BinHostEmitEvent::from_rkyv(
            cef_e,
            UPDATE_PROGRESS_EVENT,
            &UpdateProgressEvent {
                version: version.clone(),
                downloaded: 0,
                total: 0,
                installing: true,
            },
        )),
        UpdateState::Ready { version } => commands.trigger(BinHostEmitEvent::from_rkyv(
            cef_e,
            UPDATE_READY_EVENT,
            &UpdateReadyEvent {
                version: version.clone(),
            },
        )),
    }
    *last = Some(state.clone());
}
```

- [ ] **Step 4: Migrate the debug observers**

Replace `on_debug_update_ready` / `on_debug_update_clear` (2885-2897):

```rust
fn on_debug_update_ready(
    trigger: On<BinReceive<DebugUpdateReady>>,
    mut state: ResMut<UpdateState>,
) {
    *state = UpdateState::Ready {
        version: trigger.event().payload.version.clone(),
    };
}

fn on_debug_update_clear(
    _trigger: On<BinReceive<DebugUpdateClear>>,
    mut state: ResMut<UpdateState>,
) {
    *state = UpdateState::Idle;
}
```

- [ ] **Step 5: Update the updater poll resource type + Installed arm**

`crates/vmux_desktop/src/updater.rs`: change `poll_update_result` param (line 228) from

```rust
    mut staged: ResMut<vmux_layout::StagedUpdate>,
```

to

```rust
    mut state: ResMut<vmux_layout::UpdateState>,
```

Replace the `Installed` arm (247-252):

```rust
            UpdateResult::Installed { version } => {
                bevy::log::info!("update v{version} installed, will take effect on next launch");
                *state = vmux_layout::UpdateState::Ready { version };
                checker.done = true;
                return;
            }
```

- [ ] **Step 6: Update browser tests**

Replace `mod update_notice_tests` (5833-5862):

```rust
#[cfg(test)]
mod update_notice_tests {
    use super::should_emit_update;
    use vmux_layout::UpdateState;

    fn downloading(v: &str) -> UpdateState {
        UpdateState::Downloading {
            version: v.into(),
            downloaded: 1,
            total: 2,
        }
    }

    #[test]
    fn emits_on_change() {
        assert!(should_emit_update(&UpdateState::Ready { version: "v2".into() }, &None, false));
        assert!(should_emit_update(&UpdateState::Idle, &Some(downloading("v2")), false));
    }

    #[test]
    fn no_emit_when_unchanged_and_no_page_ready() {
        assert!(!should_emit_update(&UpdateState::Idle, &Some(UpdateState::Idle), false));
        let r = UpdateState::Ready { version: "v2".into() };
        assert!(!should_emit_update(&r, &Some(r.clone()), false));
    }

    #[test]
    fn re_emits_non_idle_on_page_ready() {
        let r = UpdateState::Ready { version: "v2".into() };
        assert!(should_emit_update(&r, &Some(r.clone()), true));
        assert!(!should_emit_update(&UpdateState::Idle, &Some(UpdateState::Idle), true));
    }
}
```

Replace `mod debug_update_observer_tests` (5864-5893) body to use `UpdateState`:

```rust
    #[test]
    fn debug_ready_sets_state_then_clear_resets() {
        let mut app = App::new();
        app.init_resource::<vmux_layout::UpdateState>()
            .add_observer(on_debug_update_ready)
            .add_observer(on_debug_update_clear);

        app.world_mut().trigger(BinReceive::<DebugUpdateReady> {
            webview: Entity::PLACEHOLDER,
            payload: DebugUpdateReady {
                version: "v9.0.0".into(),
            },
        });
        assert_eq!(
            *app.world().resource::<vmux_layout::UpdateState>(),
            vmux_layout::UpdateState::Ready { version: "v9.0.0".into() }
        );

        app.world_mut().trigger(BinReceive::<DebugUpdateClear> {
            webview: Entity::PLACEHOLDER,
            payload: DebugUpdateClear,
        });
        assert_eq!(
            *app.world().resource::<vmux_layout::UpdateState>(),
            vmux_layout::UpdateState::Idle
        );
    }
```

(Keep the `use super::*; use bevy_cef::prelude::BinReceive;` lines at the top of that module.)

- [ ] **Step 7: Run tests — expect PASS**

```bash
cargo test -p vmux_layout --lib
cargo test -p vmux_browser --lib update_notice_tests
cargo test -p vmux_browser --lib debug_update_observer_tests
```

Expected: PASS (vmux_browser first run recompiles CEF — slow).

- [ ] **Step 8: Commit**

```bash
git add crates/vmux_layout/src/lib.rs crates/vmux_layout/src/plugin.rs crates/vmux_browser/src/lib.rs crates/vmux_desktop/src/updater.rs
git commit -m "refactor(update): replace StagedUpdate with UpdateState enum"
```

---

## Task 3: Updater real progress

Swap to `download_and_install_extended`, stream throttled progress, wake the loop, drain-restructure the poll so messages always apply, and reset to `Idle` on failure.

**Files:**
- Modify: `crates/vmux_desktop/src/updater.rs` (imports; `UpdateResult` 204-208; `poll_update_result` 223-289; `run_update_check` 291-320; add `progress_step` + tests)

- [ ] **Step 1: Write failing `progress_step` tests**

In `mod tests` (bottom of updater.rs) add:

```rust
    #[test]
    fn progress_step_emits_on_percent_increase() {
        assert_eq!(progress_step(50, 100, 0), Some(50));
        assert_eq!(progress_step(50, 100, 50), None);
        assert_eq!(progress_step(100, 100, 50), Some(100));
    }

    #[test]
    fn progress_step_caps_at_100() {
        assert_eq!(progress_step(250, 100, 0), Some(100));
    }

    #[test]
    fn progress_step_unknown_total_buckets_by_512k() {
        let bucket = 512 * 1024;
        assert_eq!(progress_step(0, 0, 0), None);
        assert_eq!(progress_step(bucket + 1, 0, 0), Some(1));
        assert_eq!(progress_step(bucket + 1, 0, 1), None);
    }
```

- [ ] **Step 2: Run — expect FAIL (`progress_step` not found)**

```bash
cargo test -p vmux_desktop --lib progress_step
```

Expected: compile error.

- [ ] **Step 3: Add `progress_step`**

Add near `run_update_check`:

```rust
fn progress_step(downloaded: u64, total: u64, last_marker: u64) -> Option<u64> {
    if total > 0 {
        let pct = (downloaded.saturating_mul(100) / total).min(100);
        (pct > last_marker).then_some(pct)
    } else {
        let bucket = downloaded / (512 * 1024);
        (bucket > last_marker).then_some(bucket)
    }
}
```

- [ ] **Step 4: Run — expect PASS**

```bash
cargo test -p vmux_desktop --lib progress_step
```

Expected: PASS.

- [ ] **Step 5: Extend `UpdateResult`**

Replace the enum (204-208):

```rust
enum UpdateResult {
    NoUpdate,
    Downloading {
        version: String,
        downloaded: u64,
        total: u64,
    },
    Installing {
        version: String,
    },
    Installed {
        version: String,
    },
    Failed(String),
}
```

- [ ] **Step 6: Add winit imports**

At the top of updater.rs add:

```rust
use bevy::winit::{EventLoopProxyWrapper, WinitUserEvent};
```

- [ ] **Step 7: Rewrite `poll_update_result` (drain always, then poll)**

Replace the whole function (223-289). Add a `proxy` param and restructure:

```rust
fn poll_update_result(
    mut checker: ResMut<UpdateChecker>,
    config: Res<UpdateConfig>,
    settings: Res<AppSettings>,
    time: Res<Time>,
    mut state: ResMut<vmux_layout::UpdateState>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
) {
    // Always drain + apply background messages, even after `done`, so the debug
    // simulator and any late real messages still reach the UI.
    let mut results = Vec::new();
    if let Ok(rx) = checker.rx.lock() {
        while let Ok(result) = rx.try_recv() {
            results.push(result);
        }
    }
    for result in results {
        match result {
            UpdateResult::NoUpdate => {
                checker.in_flight = false;
                bevy::log::debug!("no update available");
            }
            UpdateResult::Downloading {
                version,
                downloaded,
                total,
            } => {
                *state = vmux_layout::UpdateState::Downloading {
                    version,
                    downloaded,
                    total,
                };
            }
            UpdateResult::Installing { version } => {
                *state = vmux_layout::UpdateState::Installing { version };
            }
            UpdateResult::Installed { version } => {
                checker.in_flight = false;
                bevy::log::info!("update v{version} installed, will take effect on next launch");
                *state = vmux_layout::UpdateState::Ready { version };
                checker.done = true;
            }
            UpdateResult::Failed(e) => {
                checker.in_flight = false;
                bevy::log::debug!("update check failed: {e}");
                if !matches!(
                    *state,
                    vmux_layout::UpdateState::Idle | vmux_layout::UpdateState::Ready { .. }
                ) {
                    *state = vmux_layout::UpdateState::Idle;
                }
            }
        }
    }

    if checker.done {
        return;
    }
    if !settings.auto_update {
        return;
    }
    if checker.in_flight {
        return;
    }

    checker.timer.tick(time.delta());
    if !checker.timer.just_finished() {
        return;
    }
    if !checker.started {
        checker.started = true;
        checker.timer.set_duration(config.poll_interval);
        checker.timer.set_mode(TimerMode::Repeating);
        checker.timer.reset();
    }

    let tx = checker.tx.clone();
    let endpoint = config.endpoint.clone();
    let pubkey = config.pubkey.clone();
    let wake = make_wake(proxy.as_deref());
    checker.in_flight = true;

    std::thread::spawn(move || {
        run_update_check(&endpoint, &pubkey, &tx, &*wake);
    });
}
```

- [ ] **Step 8: Add `make_wake` + rewrite `run_update_check`**

Add a helper that builds a `Send` wake closure from the proxy (no-op if absent):

```rust
fn make_wake(proxy: Option<&EventLoopProxyWrapper>) -> Box<dyn Fn() + Send> {
    match proxy {
        Some(p) => {
            let proxy = (**p).clone();
            Box::new(move || {
                let _ = proxy.send_event(WinitUserEvent::WakeUp);
            })
        }
        None => Box::new(|| {}),
    }
}
```

Replace `run_update_check` (291-320):

```rust
fn run_update_check(
    endpoint: &str,
    pubkey: &str,
    tx: &mpsc::Sender<UpdateResult>,
    wake: &(dyn Fn() + Send),
) {
    let current: semver::Version = match env!("CARGO_PKG_VERSION").parse() {
        Ok(v) => v,
        Err(e) => {
            let _ = tx.send(UpdateResult::Failed(format!("bad current version: {e}")));
            return;
        }
    };

    let url = match endpoint.parse() {
        Ok(u) => u,
        Err(e) => {
            let _ = tx.send(UpdateResult::Failed(format!("bad endpoint URL: {e}")));
            return;
        }
    };

    let config = cargo_packager_updater::Config {
        endpoints: vec![url],
        pubkey: pubkey.to_string(),
        ..Default::default()
    };

    let update = match cargo_packager_updater::check_update(current, config) {
        Ok(Some(u)) => u,
        Ok(None) => {
            let _ = tx.send(UpdateResult::NoUpdate);
            return;
        }
        Err(e) => {
            let _ = tx.send(UpdateResult::Failed(format!("{e}")));
            return;
        }
    };

    let version = update.version.clone();

    let downloaded = std::cell::Cell::new(0u64);
    let total = std::cell::Cell::new(0u64);
    let marker = std::cell::Cell::new(0u64);

    let on_chunk = |chunk_len: usize, content_len: Option<u64>| {
        if total.get() == 0 {
            if let Some(t) = content_len {
                total.set(t);
            }
        }
        downloaded.set(downloaded.get().saturating_add(chunk_len as u64));
        if let Some(m) = progress_step(downloaded.get(), total.get(), marker.get()) {
            marker.set(m);
            let _ = tx.send(UpdateResult::Downloading {
                version: version.clone(),
                downloaded: downloaded.get(),
                total: total.get(),
            });
            wake();
        }
    };
    let on_finish = || {
        let _ = tx.send(UpdateResult::Installing {
            version: version.clone(),
        });
        wake();
    };

    match update.download_and_install_extended(on_chunk, on_finish) {
        Ok(()) => {
            let _ = tx.send(UpdateResult::Installed { version });
            wake();
        }
        Err(e) => {
            let _ = tx.send(UpdateResult::Failed(format!("install failed: {e}")));
            wake();
        }
    }
}
```

- [ ] **Step 9: Run desktop tests — expect PASS**

```bash
cargo test -p vmux_desktop --lib
```

Expected: PASS (existing `relaunch_plan`/pubkey/endpoint tests + new `progress_step` tests).

- [ ] **Step 10: Commit**

```bash
git add crates/vmux_desktop/src/updater.rs
git commit -m "feat(update): stream download progress and wake the loop"
```

---

## Task 4: Page UI (Downloading / Installing / Ready)

**Files:**
- Modify: `crates/vmux_layout/src/page.rs` (signal/listeners 84-92; render 152-154; `UpdateNoticeFooter` 797-817; add `UpdatePhase`, `UpdateProgressBar`, `download_pct`; add a test in `mod tests`)

- [ ] **Step 1: Write failing `download_pct` test**

In `mod tests` at the bottom of page.rs add:

```rust
    #[test]
    fn download_pct_clamps_and_handles_zero_total() {
        assert_eq!(download_pct(0, 0), 0);
        assert_eq!(download_pct(50, 100), 50);
        assert_eq!(download_pct(250, 100), 100);
    }
```

- [ ] **Step 2: Run — expect FAIL**

```bash
cargo test -p vmux_layout --lib download_pct
```

Expected: compile error (`download_pct` not found).

- [ ] **Step 3: Add `download_pct` + `UpdatePhase`**

Add near `UpdateNoticeFooter`:

```rust
fn download_pct(downloaded: u64, total: u64) -> u64 {
    if total == 0 {
        return 0;
    }
    (downloaded.saturating_mul(100) / total).min(100)
}

#[derive(Clone, PartialEq)]
enum UpdatePhase {
    Downloading {
        version: String,
        downloaded: u64,
        total: u64,
    },
    Installing {
        version: String,
    },
    Ready {
        version: String,
    },
}
```

- [ ] **Step 4: Replace the signal + listeners (84-92)**

```rust
    let mut update_phase = use_signal(|| None::<UpdatePhase>);
    let _update_progress_listener = use_bin_event_listener::<crate::event::UpdateProgressEvent, _>(
        crate::event::UPDATE_PROGRESS_EVENT,
        move |evt| {
            update_phase.set(Some(if evt.installing {
                UpdatePhase::Installing {
                    version: evt.version,
                }
            } else {
                UpdatePhase::Downloading {
                    version: evt.version,
                    downloaded: evt.downloaded,
                    total: evt.total,
                }
            }));
        },
    );
    let _update_ready_listener = use_bin_event_listener::<crate::event::UpdateReadyEvent, _>(
        crate::event::UPDATE_READY_EVENT,
        move |evt| update_phase.set(Some(UpdatePhase::Ready { version: evt.version })),
    );
    let _update_cleared_listener = use_bin_event_listener::<crate::event::UpdateClearedEvent, _>(
        crate::event::UPDATE_CLEARED_EVENT,
        move |_| update_phase.set(None),
    );
```

- [ ] **Step 5: Replace the render site (152-154)**

```rust
                        if let Some(phase) = update_phase() {
                            UpdateNoticeFooter { phase }
                        }
```

- [ ] **Step 6: Rewrite `UpdateNoticeFooter` + add `UpdateProgressBar` (797-817)**

```rust
#[component]
fn UpdateNoticeFooter(phase: UpdatePhase) -> Element {
    let (label, version) = match &phase {
        UpdatePhase::Downloading { version, .. } => ("Downloading update", version.clone()),
        UpdatePhase::Installing { version } => ("Installing update…", version.clone()),
        UpdatePhase::Ready { version } => ("New version available", version.clone()),
    };
    rsx! {
        div {
            class: "shrink-0 mx-2 mb-2 mt-2 flex flex-col gap-2 rounded-md glass px-3 py-2 text-foreground",
            div { class: "flex items-center gap-2",
                span { class: "inline-block h-2 w-2 shrink-0 rounded-full bg-green-500" }
                span { class: "min-w-0 flex-1 text-ui font-medium", "{label}" }
                span { class: "shrink-0 text-xs text-muted-foreground", "{version}" }
            }
            match phase {
                UpdatePhase::Downloading { downloaded, total, .. } => rsx! {
                    UpdateProgressBar { downloaded, total }
                },
                UpdatePhase::Installing { .. } => rsx! {
                    UpdateProgressBar { downloaded: 0, total: 0 }
                },
                UpdatePhase::Ready { .. } => rsx! {
                    button {
                        r#type: "button",
                        class: "w-full cursor-pointer rounded-md bg-primary px-2.5 py-1.5 text-ui font-medium text-primary-foreground hover:opacity-90",
                        onclick: move |_| {
                            let _ = try_cef_bin_emit_rkyv(&crate::event::RestartRequestEvent);
                        },
                        "Restart to update"
                    }
                },
            }
        }
    }
}

#[component]
fn UpdateProgressBar(downloaded: u64, total: u64) -> Element {
    let determinate = total > 0;
    let pct = download_pct(downloaded, total);
    rsx! {
        div { class: "h-1.5 w-full overflow-hidden rounded-full bg-foreground/10",
            if determinate {
                div {
                    class: "h-full rounded-full bg-primary transition-[width] duration-200",
                    style: "width:{pct}%",
                }
            } else {
                div { class: "h-full w-1/3 rounded-full bg-primary update-progress-indeterminate" }
            }
        }
    }
}
```

- [ ] **Step 7: Run — expect PASS (native) + wasm typecheck**

```bash
cargo test -p vmux_layout --lib download_pct
cargo check -p vmux_layout --target wasm32-unknown-unknown
```

Expected: native test PASS; wasm check compiles.

- [ ] **Step 8: Commit**

```bash
git add crates/vmux_layout/src/page.rs
git commit -m "feat(layout): render download/install phases with progress bar"
```

---

## Task 5: Indeterminate-bar CSS

**Files:**
- Modify: `crates/vmux_server/assets/index.css` (after `.pane-loading-ring`, line ~137)

- [ ] **Step 1: Append the keyframe + class**

```css
@keyframes update-indeterminate {
  0% {
    transform: translateX(-100%);
  }
  100% {
    transform: translateX(400%);
  }
}

.update-progress-indeterminate {
  animation: update-indeterminate 1.2s ease-in-out infinite;
}
```

- [ ] **Step 2: Commit**

```bash
git add crates/vmux_server/assets/index.css
git commit -m "feat(ui): indeterminate update progress bar animation"
```

(The class compiles into `dist/assets/index.css` during the full build in Task 7.)

---

## Task 6: Debug "Simulate download"

Lets the bar be exercised without a real release. The page→host `DebugSimulateDownload` event spawns a host thread that scripts Downloading → Installing → Ready through the real mpsc/poll/wake path.

**Files:**
- Modify: `crates/vmux_browser/src/lib.rs:99` (add `DebugSimulateDownload` to the debug emitter tuple)
- Modify: `crates/vmux_desktop/src/updater.rs` (UpdatePlugin observer + `simulate_download`)
- Modify: `crates/vmux_layout/src/debug_page.rs:3` import + button

- [ ] **Step 1: Register the debug event for the "debug" host**

`crates/vmux_browser/src/lib.rs:99-101`: change

```rust
                BinEventEmitterPlugin::<(DebugUpdateReady, DebugUpdateClear)>::for_hosts(&[
                    "debug",
                ]),
```

to

```rust
                BinEventEmitterPlugin::<(DebugUpdateReady, DebugUpdateClear, DebugSimulateDownload)>::for_hosts(&[
                    "debug",
                ]),
```

Add `DebugSimulateDownload` to the `vmux_layout::event` import group near lines 39-45.

- [ ] **Step 2: Add the observer + simulator in updater.rs**

In `UpdatePlugin::build`, add to the chain:

```rust
        .add_observer(on_debug_simulate_download)
```

Add (note `BinReceive` is already imported at the top of updater.rs):

```rust
fn on_debug_simulate_download(
    _trigger: On<BinReceive<vmux_layout::event::DebugSimulateDownload>>,
    checker: Res<UpdateChecker>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
) {
    let tx = checker.tx.clone();
    let wake = make_wake(proxy.as_deref());
    std::thread::spawn(move || {
        simulate_download(&tx, &*wake);
    });
}

fn simulate_download(tx: &mpsc::Sender<UpdateResult>, wake: &(dyn Fn() + Send)) {
    let version = "0.0.0-sim".to_string();
    let total: u64 = 24 * 1024 * 1024;
    let step = total / 50;
    let mut downloaded = 0u64;
    while downloaded < total {
        downloaded = downloaded.saturating_add(step).min(total);
        let _ = tx.send(UpdateResult::Downloading {
            version: version.clone(),
            downloaded,
            total,
        });
        wake();
        std::thread::sleep(Duration::from_millis(60));
    }
    let _ = tx.send(UpdateResult::Installing {
        version: version.clone(),
    });
    wake();
    std::thread::sleep(Duration::from_millis(1200));
    let _ = tx.send(UpdateResult::Installed { version });
    wake();
}
```

`BinReceive` import: confirm the top of updater.rs has `use bevy_cef::prelude::{BinEventEmitterPlugin, BinReceive, JsEmitEventPlugin, Receive};` (it does). `Duration` is already imported.

- [ ] **Step 3: Add the debug button**

`crates/vmux_layout/src/debug_page.rs:3`: change the import to

```rust
use crate::event::{DebugSimulateDownload, DebugUpdateClear, DebugUpdateReady, RestartRequestEvent};
```

After the "Simulate update available" button (line ~33) add:

```rust
                    button {
                        r#type: "button",
                        class: "{BTN}",
                        onclick: move |_| {
                            let _ = try_cef_bin_emit_rkyv(&DebugSimulateDownload);
                        },
                        "Simulate download"
                    }
```

- [ ] **Step 4: Build-check both crates — expect PASS**

```bash
cargo check -p vmux_desktop
cargo check -p vmux_layout --target wasm32-unknown-unknown
```

Expected: both compile.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_browser/src/lib.rs crates/vmux_desktop/src/updater.rs crates/vmux_layout/src/debug_page.rs
git commit -m "feat(debug): simulate download to exercise the progress card"
```

---

## Task 7: Verify + handoff

- [ ] **Step 1: Format + targeted tests**

```bash
cargo fmt --all
git checkout -- patches/    # cargo fmt also reformats vendored patches; keep only crates/ changes
cargo test -p vmux_layout -p vmux_browser -p vmux_desktop
```

Expected: all PASS. (If `cargo fmt` touched `patches/`, the checkout drops it.)

- [ ] **Step 2: Full build (compiles wasm pages + CSS into dist, then desktop)**

```bash
cargo build -p vmux_desktop
```

Expected: success; `crates/vmux_server/dist/assets/index.css` now contains `update-indeterminate` / `update-progress-indeterminate`.

- [ ] **Step 3: Runtime test (user)** — single manual pass:
  - Launch vmux, open the side sheet, open the Debug page, click **Simulate download**.
  - Confirm the side-sheet footer card shows: green dot + "Downloading update" + version + a determinate bar advancing 0→100%, then "Installing update…" with the sliding indeterminate bar, then "New version available" + "Restart to update".
  - Click **Restart to update** → app relaunches.
  - Click **Clear update** → card disappears.

- [ ] **Step 4: Commit any fmt-only changes if not already committed, then delete this plan**

```bash
git rm docs/plans/2026-06-27-update-download-progress.md
git commit -m "chore: remove completed update-progress plan"
```

---

## Self-Review

**Spec coverage:**
- Card appears during download with progress bar → Task 4 (`UpdateProgressBar`, determinate). ✓
- Same card morphs Downloading → Installing → Ready → Task 4 (`UpdatePhase` + `UpdateNoticeFooter`). ✓
- Real byte progress → Task 3 (`download_and_install_extended` + `on_chunk`). ✓
- Throttle → Task 3 (`progress_step`). ✓
- Loop wakes so the bar repaints under Reactive mode → Task 3 (`make_wake`, `WakeUp`). ✓
- Indeterminate when no content-length → Task 4 (`determinate` branch) + Task 5 (CSS). ✓
- Failure clears the card + retries → Task 3 (`Failed`→`Idle`, `done` untouched). ✓
- Testable without a release → Task 6 (debug simulate). ✓
- Keep "Restart to update" string (source-scrape tests) → Task 4 keeps it verbatim. ✓

**Placeholder scan:** none — every code step has full code.

**Type consistency:** `UpdateState` (4 variants) used identically in lib.rs, plugin.rs, browser, updater. `UpdateProgressEvent { version, downloaded, total, installing }` consistent across event.rs, browser emit, page listener. `progress_step`/`download_pct` signatures match call sites. `make_wake(Option<&EventLoopProxyWrapper>) -> Box<dyn Fn()+Send>` used by both `poll_update_result` and `on_debug_simulate_download`. ✓
