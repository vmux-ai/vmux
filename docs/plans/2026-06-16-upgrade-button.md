# Upgrade Button + `vmux://debug` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. NOTE: do NOT subagent-drive this plan — CEF builds are huge and long-lived agents drop sockets. Implement inline with a warm target dir.

**Goal:** Surface a "New version available · [Restart to update]" footer at the bottom of the left side sheet when an update is staged on disk, and add a `vmux://debug` page to emulate the flow without cutting a release. Silent background install is unchanged.

**Architecture:** A `StagedUpdate(Option<String>)` resource is the single source of truth. The updater (vmux_desktop) sets it when an install succeeds; `vmux://debug` sets it via debug IPC events. `vmux_browser` (the layout-webview IPC hub) reads it and emits `UpdateReadyEvent`/`UpdateClearedEvent` to the `LayoutCef` webview; the layout Dioxus page (vmux_layout::page) renders the footer and emits `RestartRequestEvent` on click; the updater receives that and relaunches. All events are rkyv types in `vmux_layout::event`.

**Tech Stack:** Rust, Bevy 0.19-rc, bevy_cef (rkyv bin IPC: `BinHostEmitEvent`/`BinReceive`/`BinEventEmitterPlugin`), Dioxus (wasm pages), minisign/cargo-packager-updater (existing).

---

## File Structure

- `crates/vmux_layout/src/event.rs` — **modify**: add 5 event types + 2 const ids (wasm-safe, shared).
- `crates/vmux_layout/src/lib.rs` — **modify**: `StagedUpdate` resource, `DEBUG_PAGE_MANIFEST` const, declare `debug` (native) + `debug_page` (wasm) modules.
- `crates/vmux_layout/src/plugin.rs` — **modify**: `init_resource::<StagedUpdate>()`, register `handle_debug_page_open` in `PageOpenSet::HandleKnownPages`.
- `crates/vmux_layout/src/cef.rs` — **modify**: spawn `DEBUG_PAGE_MANIFEST`.
- `crates/vmux_layout/src/debug.rs` — **create** (native): `DEBUG_PAGE_URL`, `DebugView` webview bundle, `handle_debug_page_open`.
- `crates/vmux_layout/src/debug_page.rs` — **create** (wasm): Dioxus `Page` with version input + 3 buttons.
- `crates/vmux_layout/src/page.rs` — **modify**: update listeners + `UpdateNoticeFooter` in side sheet.
- `crates/vmux_browser/src/lib.rs` — **modify**: `push_update_notice_emit` system + decision helper, `on_debug_update_ready`/`on_debug_update_clear` observers + plugin reg.
- `crates/vmux_desktop/src/updater.rs` — **modify**: set `StagedUpdate` on install, `RestartRequestEvent` receive + `relaunch_plan` seam.
- `crates/vmux_server/src/lib.rs` — **modify**: `web_pages!` entry `render_debug: "debug" => vmux_layout::debug_page::Page`.

---

### Task 1: Update IPC event types

**Files:**
- Modify: `crates/vmux_layout/src/event.rs` (append after existing consts/types)
- Test: same file (`#[cfg(test)] mod` — add if absent)

- [ ] **Step 1: Write the failing test**

Append to `crates/vmux_layout/src/event.rs`:

```rust
#[cfg(test)]
mod update_event_tests {
    use super::*;

    #[test]
    fn update_ready_event_rkyv_round_trips() {
        let evt = UpdateReadyEvent { version: "v9.9.9".to_string() };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&evt).unwrap();
        let back = rkyv::from_bytes::<UpdateReadyEvent, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(back.version, "v9.9.9");
    }

    #[test]
    fn event_ids_are_stable() {
        assert_eq!(UPDATE_READY_EVENT, "update-ready");
        assert_eq!(UPDATE_CLEARED_EVENT, "update-cleared");
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `bash -c "cargo test -p vmux_layout --lib update_event_tests 2>&1 | tail -20"`
Expected: FAIL — `cannot find type UpdateReadyEvent` / `UPDATE_READY_EVENT`.

- [ ] **Step 3: Add the events + consts**

Append to `crates/vmux_layout/src/event.rs` (above the test module):

```rust
pub const UPDATE_READY_EVENT: &str = "update-ready";
pub const UPDATE_CLEARED_EVENT: &str = "update-cleared";

#[derive(
    Clone, Debug, Default,
    serde::Serialize, serde::Deserialize,
    rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub struct UpdateReadyEvent {
    pub version: String,
}

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq,
    serde::Serialize, serde::Deserialize,
    rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub struct UpdateClearedEvent;

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq,
    serde::Serialize, serde::Deserialize,
    rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub struct RestartRequestEvent;

#[derive(
    Clone, Debug, Default,
    serde::Serialize, serde::Deserialize,
    rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub struct DebugUpdateReady {
    pub version: String,
}

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq,
    serde::Serialize, serde::Deserialize,
    rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub struct DebugUpdateClear;
```

- [ ] **Step 4: Run test to verify it passes**

Run: `bash -c "cargo test -p vmux_layout --lib update_event_tests 2>&1 | tail -20"`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_layout/src/event.rs
git commit -m "feat(updater): add update-notice IPC event types"
```

---

### Task 2: `StagedUpdate` resource

**Files:**
- Modify: `crates/vmux_layout/src/lib.rs`
- Modify: `crates/vmux_layout/src/plugin.rs:28-31` (the `app.register_type...init_resource` chain head)

- [ ] **Step 1: Add the resource**

In `crates/vmux_layout/src/lib.rs`, after the `SpaceFilePresent` resource (around line 109):

```rust
#[cfg(not(target_arch = "wasm32"))]
#[derive(Resource, Default)]
pub struct StagedUpdate(pub Option<String>);
```

- [ ] **Step 2: Init it in LayoutPlugin**

In `crates/vmux_layout/src/plugin.rs`, extend the existing builder chain (after `.init_resource::<settings::ConfirmCloseSettings>()` on line 30):

```rust
            .init_resource::<crate::StagedUpdate>()
```

- [ ] **Step 3: Verify it compiles**

Run: `bash -c "cargo check -p vmux_layout 2>&1 | tail -20"`
Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add crates/vmux_layout/src/lib.rs crates/vmux_layout/src/plugin.rs
git commit -m "feat(updater): add StagedUpdate resource"
```

---

### Task 3: Browser emit system (StagedUpdate → LayoutCef)

**Files:**
- Modify: `crates/vmux_browser/src/lib.rs` (add system + helper; register in the push-emit `add_systems` group at line ~145-155)

- [ ] **Step 1: Write the failing test**

Add to the `#[cfg(test)]` module in `crates/vmux_browser/src/lib.rs` (reuse the existing test module; if adding a new one, place at end of file):

```rust
#[cfg(test)]
mod update_notice_tests {
    use super::should_emit_update_notice;

    #[test]
    fn emits_on_change() {
        assert!(should_emit_update_notice(&Some("v2".into()), &None, false));
        assert!(should_emit_update_notice(&None, &Some("v2".into()), false));
    }

    #[test]
    fn no_emit_when_unchanged_and_no_page_ready() {
        assert!(!should_emit_update_notice(&None, &None, false));
        assert!(!should_emit_update_notice(&Some("v2".into()), &Some("v2".into()), false));
    }

    #[test]
    fn re_emits_staged_on_page_ready_but_not_idle() {
        assert!(should_emit_update_notice(&Some("v2".into()), &Some("v2".into()), true));
        assert!(!should_emit_update_notice(&None, &None, true));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `bash -c "cargo test -p vmux_browser --lib update_notice_tests 2>&1 | tail -20"`
Expected: FAIL — `cannot find function should_emit_update_notice`.

- [ ] **Step 3: Implement helper + system**

Add near the other `push_*_emit` systems in `crates/vmux_browser/src/lib.rs`:

```rust
fn should_emit_update_notice(
    current: &Option<String>,
    last: &Option<String>,
    page_ready_changed: bool,
) -> bool {
    current != last || (page_ready_changed && current.is_some())
}

fn push_update_notice_emit(
    mut commands: Commands,
    browsers: NonSend<Browsers>,
    cef_q: Query<(Entity, Ref<PageReady>), With<LayoutCef>>,
    staged: Res<vmux_layout::StagedUpdate>,
    mut last: Local<Option<String>>,
) {
    let Ok((cef_e, page_ready)) = cef_q.single() else {
        return;
    };
    if !browsers.has_browser(cef_e) || !browsers.host_emit_ready(&cef_e) {
        return;
    }
    if !should_emit_update_notice(&staged.0, &last, page_ready.is_changed()) {
        return;
    }
    match &staged.0 {
        Some(version) => commands.trigger(BinHostEmitEvent::from_rkyv(
            cef_e,
            vmux_layout::event::UPDATE_READY_EVENT,
            &vmux_layout::event::UpdateReadyEvent { version: version.clone() },
        )),
        None => commands.trigger(BinHostEmitEvent::from_rkyv(
            cef_e,
            vmux_layout::event::UPDATE_CLEARED_EVENT,
            &vmux_layout::event::UpdateClearedEvent,
        )),
    }
    *last = staged.0.clone();
}
```

Register it: in the `add_systems(Update, ( push_layout_state_emit, push_stacks_host_emit, push_pane_tree_emit, push_tabs_host_emit, ) ...)` group (line ~146-154), add `push_update_notice_emit,` to the tuple.

- [ ] **Step 4: Run test to verify it passes**

Run: `bash -c "cargo test -p vmux_browser --lib update_notice_tests 2>&1 | tail -20"`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_browser/src/lib.rs
git commit -m "feat(updater): emit update notice from StagedUpdate to layout webview"
```

---

### Task 4: Browser debug receive observers

**Files:**
- Modify: `crates/vmux_browser/src/lib.rs` (observers + plugin reg in BrowserPlugin)

- [ ] **Step 1: Write the failing test**

Add to `crates/vmux_browser/src/lib.rs` test section:

```rust
#[cfg(test)]
mod debug_update_observer_tests {
    use super::*;
    use bevy::prelude::*;
    use bevy_cef::prelude::BinReceive;
    use vmux_layout::event::{DebugUpdateClear, DebugUpdateReady};

    #[test]
    fn debug_ready_sets_staged_then_clear_resets() {
        let mut app = App::new();
        app.init_resource::<vmux_layout::StagedUpdate>()
            .add_observer(on_debug_update_ready)
            .add_observer(on_debug_update_clear);

        app.world_mut().trigger(BinReceive::<DebugUpdateReady> {
            webview: Entity::PLACEHOLDER,
            payload: DebugUpdateReady { version: "v9.0.0".into() },
        });
        assert_eq!(
            app.world().resource::<vmux_layout::StagedUpdate>().0.as_deref(),
            Some("v9.0.0")
        );

        app.world_mut().trigger(BinReceive::<DebugUpdateClear> {
            webview: Entity::PLACEHOLDER,
            payload: DebugUpdateClear,
        });
        assert_eq!(app.world().resource::<vmux_layout::StagedUpdate>().0, None);
    }
}
```

(If `BinReceive`'s field names differ from `webview`/`payload`, match the definition used at `on_header_command_emit` / `reset_spaces_sent_marker_on_page_ready` — `trigger.event().payload` and `trigger.event().webview` are both used in this file, confirming the names.)

- [ ] **Step 2: Run test to verify it fails**

Run: `bash -c "cargo test -p vmux_browser --lib debug_update_observer_tests 2>&1 | tail -20"`
Expected: FAIL — `cannot find function on_debug_update_ready`.

- [ ] **Step 3: Implement observers + register**

Add observers in `crates/vmux_browser/src/lib.rs`:

```rust
fn on_debug_update_ready(
    trigger: On<BinReceive<vmux_layout::event::DebugUpdateReady>>,
    mut staged: ResMut<vmux_layout::StagedUpdate>,
) {
    staged.0 = Some(trigger.event().payload.version.clone());
}

fn on_debug_update_clear(
    _trigger: On<BinReceive<vmux_layout::event::DebugUpdateClear>>,
    mut staged: ResMut<vmux_layout::StagedUpdate>,
) {
    staged.0 = None;
}
```

In `BrowserPlugin::build`, extend the existing `add_plugins((...))` that holds `BinEventEmitterPlugin::<(HeaderCommandEvent, SideSheetCommandEvent)>::default()` (line 87) — add a second plugin entry and two observers after the existing `.add_observer(...)` calls (lines 89-93):

```rust
                BinEventEmitterPlugin::<(
                    vmux_layout::event::DebugUpdateReady,
                    vmux_layout::event::DebugUpdateClear,
                )>::default(),
```
```rust
            .add_observer(on_debug_update_ready)
            .add_observer(on_debug_update_clear)
```

- [ ] **Step 4: Run test to verify it passes**

Run: `bash -c "cargo test -p vmux_browser --lib debug_update_observer_tests 2>&1 | tail -20"`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_browser/src/lib.rs
git commit -m "feat(updater): debug receive observers set StagedUpdate"
```

---

### Task 5: Updater — set StagedUpdate on install + restart/relaunch

**Files:**
- Modify: `crates/vmux_desktop/src/updater.rs`

- [ ] **Step 1: Write the failing test**

Add to the `#[cfg(test)] mod tests` in `crates/vmux_desktop/src/updater.rs`:

```rust
    #[test]
    fn relaunch_plan_targets_app_bundle() {
        let exe = std::path::Path::new(
            "/Applications/Vmux.app/Contents/MacOS/vmux_desktop",
        );
        let args = relaunch_plan(exe, 4242).expect("inside .app");
        assert_eq!(args[0], "-c");
        assert!(args[1].contains("kill -0 4242"));
        assert!(args[1].contains("open \"/Applications/Vmux.app\""));
    }

    #[test]
    fn relaunch_plan_none_outside_app() {
        let exe = std::path::Path::new("/tmp/target/debug/vmux_desktop");
        assert!(relaunch_plan(exe, 1).is_none());
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `bash -c "cargo test -p vmux_desktop --lib updater 2>&1 | tail -20"`
Expected: FAIL — `cannot find function relaunch_plan`.

- [ ] **Step 3: Implement seam + observer + register + install hook**

In `crates/vmux_desktop/src/updater.rs`:

Add imports at top:
```rust
use bevy_cef::prelude::{BinEventEmitterPlugin, BinReceive};
use vmux_layout::event::RestartRequestEvent;
```

Add the seam + observer (module level):
```rust
fn relaunch_plan(exe: &std::path::Path, pid: u32) -> Option<Vec<String>> {
    let app = exe.ancestors().nth(3)?;
    if app.extension().and_then(|e| e.to_str()) != Some("app") {
        return None;
    }
    let app = app.to_str()?;
    Some(vec![
        "-c".to_string(),
        format!("while kill -0 {pid} 2>/dev/null; do sleep 0.2; done; open \"{app}\""),
    ])
}

fn on_restart_request(
    _trigger: On<BinReceive<RestartRequestEvent>>,
    mut exit: MessageWriter<AppExit>,
) {
    let pid = std::process::id();
    match std::env::current_exe().ok().and_then(|exe| relaunch_plan(&exe, pid)) {
        Some(args) => {
            if let Err(e) = std::process::Command::new("sh").args(&args).spawn() {
                bevy::log::error!("failed to spawn relauncher: {e}");
                return;
            }
            bevy::log::info!("relaunching to apply update");
            exit.write(AppExit::Success);
        }
        None => {
            bevy::log::info!(
                "update restart requested but not running from .app; relaunch is a dev no-op"
            );
        }
    }
}
```

In `UpdatePlugin::build` (the `app.insert_resource(...)...add_systems(...)` chain), append:
```rust
        .add_plugins(BinEventEmitterPlugin::<(RestartRequestEvent,)>::default())
        .add_observer(on_restart_request);
```

In `poll_update_result`, add a param `mut staged: ResMut<vmux_layout::StagedUpdate>,` and in the `UpdateResult::Installed { version }` arm, before `checker.done = true;`:
```rust
                staged.0 = Some(version.clone());
```

Ensure `bevy::prelude::*` (already imported) brings `AppExit`, `MessageWriter`. If `AppExit` is unresolved, add `use bevy::app::AppExit;`.

- [ ] **Step 4: Run test to verify it passes**

Run: `bash -c "cargo test -p vmux_desktop --lib updater 2>&1 | tail -20"`
Expected: PASS (existing + 2 new).

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_desktop/src/updater.rs
git commit -m "feat(updater): stage update on install, relaunch on restart request"
```

---

### Task 6: Side-sheet footer UI

**Files:**
- Modify: `crates/vmux_layout/src/page.rs`

- [ ] **Step 1: Add the update-version listeners**

In `Page()` (after the `spaces_listener` block, ~line 61), add:

```rust
    let mut update_version = use_signal(|| None::<String>);
    let _update_ready_listener = use_bin_event_listener::<crate::event::UpdateReadyEvent, _>(
        crate::event::UPDATE_READY_EVENT,
        move |evt| update_version.set(Some(evt.version)),
    );
    let _update_cleared_listener = use_bin_event_listener::<crate::event::UpdateClearedEvent, _>(
        crate::event::UPDATE_CLEARED_EVENT,
        move |_| update_version.set(None),
    );
```

- [ ] **Step 2: Render the footer in the side sheet**

In the `aside` block, the inner container is `div { class: "flex h-full min-h-0 flex-col", SideSheetView { ... } }` (lines 115-121). Add the footer as a sibling **after** `SideSheetView { ... }`, still inside that `div`:

```rust
                        if let Some(v) = update_version() {
                            UpdateNoticeFooter { version: v }
                        }
```

- [ ] **Step 3: Add the component**

Add at the end of `crates/vmux_layout/src/page.rs` (before `#[cfg(test)]`):

```rust
#[component]
fn UpdateNoticeFooter(version: String) -> Element {
    rsx! {
        div {
            class: "shrink-0 mt-2 flex items-center gap-2 rounded-md glass px-3 py-2 text-foreground",
            span { class: "inline-block h-2 w-2 shrink-0 rounded-full bg-green-500" }
            div { class: "min-w-0 flex-1",
                div { class: "text-ui font-medium", "New version available" }
                div { class: "truncate text-xs text-muted-foreground", "{version}" }
            }
            button {
                r#type: "button",
                class: "shrink-0 cursor-pointer rounded-md bg-primary px-2.5 py-1 text-ui font-medium text-primary-foreground hover:opacity-90",
                onclick: move |_| {
                    let _ = try_cef_bin_emit_rkyv(&crate::event::RestartRequestEvent);
                },
                "Restart to update"
            }
        }
    }
}
```

- [ ] **Step 4: Verify the wasm page builds**

Run: `bash -c "cargo check -p vmux_layout --target wasm32-unknown-unknown --features web 2>&1 | tail -25"`
(If the crate has no `web` feature gate for the check, use the project's standard web build: `bash -c "cd website && dx build 2>&1 | tail -25"` or the Makefile target used for the embedded UI. Expected: compiles.)

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_layout/src/page.rs
git commit -m "feat(updater): side-sheet restart-to-update footer"
```

---

### Task 7: `vmux://debug` Dioxus page + router entry

**Files:**
- Create: `crates/vmux_layout/src/debug_page.rs`
- Modify: `crates/vmux_layout/src/lib.rs` (declare module)
- Modify: `crates/vmux_server/src/lib.rs` (web_pages! entry)

- [ ] **Step 1: Create the page**

`crates/vmux_layout/src/debug_page.rs`:

```rust
#![allow(non_snake_case)]

use crate::event::{DebugUpdateClear, DebugUpdateReady, RestartRequestEvent};
use dioxus::prelude::*;
use vmux_ui::hooks::{try_cef_bin_emit_rkyv, use_theme};

const BTN: &str = "cursor-pointer rounded-md border border-border bg-card px-3 py-1.5 text-sm text-foreground transition-colors hover:border-foreground/30 hover:bg-muted";

#[component]
pub fn Page() -> Element {
    use_theme();
    let mut version = use_signal(|| "v99.0.0".to_string());

    rsx! {
        div { class: "flex h-full min-h-0 flex-col gap-4 bg-background p-6 text-foreground",
            h1 { class: "text-lg font-semibold", "Debug" }
            section { class: "flex flex-col gap-2",
                h2 { class: "text-sm font-medium text-muted-foreground", "Auto-update" }
                input {
                    r#type: "text",
                    class: "rounded-md border border-border bg-card px-3 py-2 text-sm outline-none",
                    value: "{version}",
                    oninput: move |e| version.set(e.value()),
                }
                div { class: "flex flex-wrap gap-2",
                    button {
                        r#type: "button",
                        class: "{BTN}",
                        onclick: move |_| {
                            let _ = try_cef_bin_emit_rkyv(&DebugUpdateReady { version: version() });
                        },
                        "Simulate update available"
                    }
                    button {
                        r#type: "button",
                        class: "{BTN}",
                        onclick: move |_| { let _ = try_cef_bin_emit_rkyv(&DebugUpdateClear); },
                        "Clear update"
                    }
                    button {
                        r#type: "button",
                        class: "{BTN}",
                        onclick: move |_| { let _ = try_cef_bin_emit_rkyv(&RestartRequestEvent); },
                        "Trigger restart"
                    }
                }
            }
        }
    }
}
```

- [ ] **Step 2: Declare the module**

In `crates/vmux_layout/src/lib.rs`, next to `#[cfg(target_arch = "wasm32")] pub mod page;` (line 9-10):

```rust
#[cfg(target_arch = "wasm32")]
pub mod debug_page;
```

- [ ] **Step 3: Add the router entry**

In `crates/vmux_server/src/lib.rs`, inside the `web_pages! { ... }` block (after `render_spaces`):

```rust
    render_debug: "debug" => vmux_layout::debug_page::Page,
```

- [ ] **Step 4: Verify the wasm bundle builds**

Run the project's web build (same command as Task 6 Step 4). Expected: compiles, no "unknown host" issues.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_layout/src/debug_page.rs crates/vmux_layout/src/lib.rs crates/vmux_server/src/lib.rs
git commit -m "feat(debug): add vmux://debug page"
```

---

### Task 8: `vmux://debug` native open handler + manifest

**Files:**
- Create: `crates/vmux_layout/src/debug.rs`
- Modify: `crates/vmux_layout/src/lib.rs` (module + `DEBUG_PAGE_MANIFEST`)
- Modify: `crates/vmux_layout/src/cef.rs` (spawn manifest)
- Modify: `crates/vmux_layout/src/plugin.rs` (register handler)

- [ ] **Step 1: Write the failing test**

Add to `crates/vmux_layout/src/lib.rs` `#[cfg(test)] mod tests`:

```rust
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn debug_manifest_and_url_are_consistent() {
        assert_eq!(DEBUG_PAGE_MANIFEST.host, "debug");
        assert_eq!(crate::debug::DEBUG_PAGE_URL, "vmux://debug/");
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `bash -c "cargo test -p vmux_layout --lib debug_manifest_and_url 2>&1 | tail -20"`
Expected: FAIL — `DEBUG_PAGE_MANIFEST`/`debug` module not found.

- [ ] **Step 3: Create the native module**

`crates/vmux_layout/src/debug.rs`:

```rust
use bevy::picking::Pickable;
use bevy::prelude::*;
use bevy_cef::prelude::*;
use vmux_core::{PageMetadata, PageOpenError, PageOpenHandled, PageOpenTask};

use crate::cef::Browser;

pub const DEBUG_PAGE_URL: &str = "vmux://debug/";

#[derive(Component)]
pub struct DebugView;

impl DebugView {
    pub fn new(
        meshes: &mut ResMut<Assets<Mesh>>,
        webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
    ) -> impl Bundle {
        (
            (
                Self,
                Browser,
                WebviewSource::new(DEBUG_PAGE_URL),
                ResolvedWebviewUri(DEBUG_PAGE_URL.to_string()),
                PageMetadata {
                    title: "Debug".to_string(),
                    url: DEBUG_PAGE_URL.to_string(),
                    favicon_url: String::new(),
                    bg_color: None,
                },
                Mesh3d(meshes.add(bevy::math::primitives::Plane3d::new(
                    Vec3::Z,
                    Vec2::splat(0.5),
                ))),
            ),
            (
                MeshMaterial3d(webview_mt.add(WebviewExtendStandardMaterial::default())),
                WebviewSize(Vec2::new(1280.0, 720.0)),
                Transform::default(),
                GlobalTransform::default(),
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(0.0),
                    right: Val::Px(0.0),
                    top: Val::Px(0.0),
                    bottom: Val::Px(0.0),
                    ..default()
                },
                Visibility::Inherited,
                Pickable::default(),
            ),
        )
    }
}

type PendingPageOpen = (Without<PageOpenHandled>, Without<PageOpenError>);

pub fn handle_debug_page_open(
    tasks: Query<(Entity, &PageOpenTask), PendingPageOpen>,
    children_q: Query<&Children>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut webview_mt: ResMut<Assets<WebviewExtendStandardMaterial>>,
) {
    for (entity, task) in &tasks {
        if task.url != DEBUG_PAGE_URL {
            continue;
        }
        if let Ok(children) = children_q.get(task.stack) {
            for child in children.iter() {
                commands.entity(child).try_despawn();
            }
        }
        commands.entity(task.stack).insert(PageMetadata {
            title: "Debug".to_string(),
            url: DEBUG_PAGE_URL.to_string(),
            favicon_url: String::new(),
            bg_color: None,
        });
        commands.spawn((DebugView::new(&mut meshes, &mut webview_mt), ChildOf(task.stack)));
        commands.entity(entity).insert(PageOpenHandled);
    }
}
```

- [ ] **Step 4: Declare module + manifest + spawn + register**

In `crates/vmux_layout/src/lib.rs`:
- Add module (native): next to other `#[cfg(not(target_arch = "wasm32"))] pub mod ...`:
```rust
#[cfg(not(target_arch = "wasm32"))]
pub mod debug;
```
- Add the manifest const (after `COMMAND_BAR_PAGE_MANIFEST`, line 77):
```rust
#[cfg(not(target_arch = "wasm32"))]
pub const DEBUG_PAGE_MANIFEST: vmux_core::page::PageManifest =
    vmux_core::page::PageManifest { host: "debug" };
```

In `crates/vmux_layout/src/cef.rs`, in `LayoutCefPlugin::build` (after line 28):
```rust
        app.world_mut().spawn(crate::DEBUG_PAGE_MANIFEST);
```

In `crates/vmux_layout/src/plugin.rs`, add to the LayoutPlugin builder (a new `.add_systems` near line 54):
```rust
            .add_systems(
                Update,
                crate::debug::handle_debug_page_open
                    .in_set(vmux_core::PageOpenSet::HandleKnownPages),
            )
```

- [ ] **Step 5: Run test + check build**

Run: `bash -c "cargo test -p vmux_layout --lib debug_manifest_and_url 2>&1 | tail -20"`
Expected: PASS.
Run: `bash -c "cargo check -p vmux_layout 2>&1 | tail -20"`
Expected: no errors.

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_layout/src/debug.rs crates/vmux_layout/src/lib.rs crates/vmux_layout/src/cef.rs crates/vmux_layout/src/plugin.rs
git commit -m "feat(debug): claim and render vmux://debug page in a stack"
```

---

### Task 9: Build, wire-up check, manual end-to-end

- [ ] **Step 1: Full workspace build (warm target dir)**

Run: `bash -c "cargo build -p vmux_desktop 2>&1 | tail -30"`
Expected: builds. Fix any cfg-gated import ordering (rustfmt) or missing-use errors.

- [ ] **Step 2: Targeted checks**

Run: `bash -c "cargo test -p vmux_layout -p vmux_browser -p vmux_desktop --lib 2>&1 | tail -30"`
Expected: all green.
Run: `bash -c "cargo clippy -p vmux_layout -p vmux_browser -p vmux_desktop --all-targets 2>&1 | tail -20"`
Expected: no warnings (CI denies warnings).

- [ ] **Step 3: Manual — footer appears + restart**

Launch the app (dev). Open the side sheet. Open a stack and navigate to `vmux://debug`. Click **Simulate update available** → confirm the side-sheet footer shows `New version available · v99.0.0 · [Restart to update]`. Click **Clear update** → footer disappears. Click **Simulate update available** again, then **Restart to update** in the footer (or **Trigger restart** on the debug page) → confirm log `update restart requested but not running from .app; relaunch is a dev no-op` (dev) — and on an installed `.app`, confirm the app relaunches into the new version.

- [ ] **Step 4: Manual — real path sanity**

Confirm a real `Installed` path still works: the `staged.0 = Some(version)` line is hit on install (covered by Task 5 unit test for the seam; the install→resource wiring is verified by code review since triggering a real download in dev is impractical).

- [ ] **Step 5: Delete the plan file (per AGENTS.md) + final commit**

```bash
git rm docs/plans/2026-06-16-upgrade-button.md
git commit -m "chore: remove implemented plan"
```

---

## Self-Review

**Spec coverage:** silent install unchanged (Task 5 only adds a resource write) ✓; footer at side-sheet bottom (Task 6) ✓; events in `vmux_layout::event` (Task 1) ✓; `StagedUpdate` source of truth (Task 2, written by Task 5 + Task 4) ✓; emit to LayoutCef (Task 3) ✓; relaunch seam, dev-safe (Task 5) ✓; `vmux://debug` page + open + manifest + router (Tasks 7-8) ✓; debug simulate/clear/restart (Task 7 buttons → Task 4 observers + Task 5 restart) ✓.

**Type consistency:** `UpdateReadyEvent{version}`, `UpdateClearedEvent`, `RestartRequestEvent`, `DebugUpdateReady{version}`, `DebugUpdateClear` used identically across producer (Task 7), bridge (Tasks 3/4), consumer (Task 6), updater (Task 5). Const ids `UPDATE_READY_EVENT`/`UPDATE_CLEARED_EVENT` shared between emit (Task 3) and listen (Task 6). `StagedUpdate` field `.0` consistent.

**Open risk to confirm during execution:** `BinReceive<T>` field names (`webview`, `payload`) — both are referenced in existing `vmux_browser` code, so confirmed. The exact web build command (Task 6/7 Step 4) depends on the repo's wasm toolchain — use the Makefile target the project already uses for the embedded UI if the raw `cargo check --target wasm32` form is gated.
