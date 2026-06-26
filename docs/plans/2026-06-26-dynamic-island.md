# Dynamic Island (P1 shell + P2 search/unify) Implementation Plan

> **For agentic workers:** This plan is executed **inline by the primary session**, NOT via
> subagents — vmux's CEF builds are too large/long-running for subagent execution (sockets drop).
> Warm the CEF target dir once with a background `cargo build -p vmux_desktop`, then do incremental
> builds. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the in-window command bar with an always-on Dynamic Island — a glass pill pinned
top-center over all apps that morphs as actions/info flow and expands into the unified command bar.

**Architecture:** A persistent borderless nonactivating `NSPanel` (objc2) hosts a rounded
`NSGlassEffectView` backdrop with an OSR CEF webview composited above it (reuse the layout's
`NativeOverlayFrames` → `CALayer` pattern). A page-owned morph state machine is driven by typed
`IslandEvent`s from Rust; native animates the panel frame to the page's reported size. Search reuses
the existing command-bar page/handler/results; `Cmd+Shift+Space` (global, via `global-hotkey`) and
`Cmd+K` expand it. The in-window modal is removed.

**Tech Stack:** Rust, Bevy 0.19-rc.2 ECS, bevy_cef (OSR), objc2 / objc2-app-kit (NSPanel,
NSGlassEffectView, NSView, NSScreen, NSEvent), Dioxus→WASM pages, rkyv host-emit events,
`global-hotkey` crate.

**Testing approach (project-fit):** Unit/ECS tests where feasible (rkyv round-trips, pure
state-machine fns, payload-gatherer extraction, hotkey/settings parsing, source-scrape guards).
Native NSPanel/glass/OSR/key-forwarding are validated in **one runtime pass per phase** (finish then
test), not per-step. Commit after each task.

**Out of scope (own plan later):** P3 feeds (agent/terminal/notifications/browser-media adapters).
P1 ships with a temporary manual morph trigger; P2 ships the real search/unify.

---

## File structure

**New files**

- `crates/vmux_command/src/island.rs` — rkyv payload types sent to the island page
  (`IslandRenderEvent` with `IslandState`, `IslandActivity`, `IslandNotice`) + the
  `ISLAND_RENDER_EVENT` channel name. Lives beside the existing command-bar event types so both
  pages share the rkyv/host-emit contract.
- `crates/vmux_layout/src/island.rs` + `crates/vmux_layout/src/island/` — ECS island module:
  - `island/plugin.rs` — `IslandPlugin` (register types, messages, systems).
  - `island/event.rs` — Bevy message types: `IslandEvent` (ExpandSearch/Collapse/Activity/Notify),
    `IslandPanelShow`/`IslandPanelHide`/`IslandPanelResize`/`IslandPanelDismissed`, `SummonCommandBar`.
  - `island/state.rs` — pure morph state machine (`IslandState` reducer; unit-tested).
  - `island/handler.rs` — systems: apply `IslandEvent` → emit `ISLAND_RENDER_EVENT` to the island
    webview; consume page size events → `IslandPanelResize`; expand/collapse bookkeeping.
  - `island/page.rs` — Dioxus island page (idle pill + activity/notify states; search state renders
    the existing command-bar component).
- `crates/vmux_desktop/src/dynamic_island.rs` — native macOS: `NSPanel` + rounded
  `NSGlassEffectView` backdrop, top-center/main-display/notch-aware positioning,
  `sync_island_overlay` compositing of the OSR IOSurface above the glass, key first-responder
  `NSView` subclass + key forwarding, mouse-passthrough toggle, `resignKey` observer, animated
  frame resize, `global-hotkey` manager + waker thread.

**Modified files**

- `crates/vmux_layout/src/command_bar/handler.rs` — extract `CommandBarPayloadSources` (SystemParam)
  + `build_command_bar_open_payload(...)` from `handle_open_command_bar`.
- `crates/vmux_layout/src/window.rs` — spawn the `Island` OSR webview entity (P1); remove the
  `Modal` spawn (P2, ~lines 390-420).
- `crates/vmux_layout/src/lib.rs` — add `pub mod island;` and register `IslandPlugin`.
- `crates/vmux_desktop/src/glass.rs` — remove `sync_command_bar_overlay` + `CommandBarOverlay` (P2).
- `crates/vmux_desktop/src/lib.rs` — add `mod dynamic_island;` and the plugin to the app.
- `crates/vmux_desktop/Cargo.toml` — add `global-hotkey`; ensure objc2-app-kit features
  `NSGlassEffectView` (present), add `NSGlassEffectContainerView` if used later, `NSScreen` (present).
- `crates/vmux_setting/src/lib.rs` — add `command_bar.global_hotkey: Option<String>`.
- `crates/vmux_server/build.rs` — add the island page `src` to `track_manifest_rel_paths`.
- `crates/vmux_server/assets/index.css` — add the island crate/src to the `@source` globs.

---

# Phase P1 — Island shell

## Task 1: Island render-event payload types (rkyv)

**Files:**
- Create: `crates/vmux_command/src/island.rs`
- Modify: `crates/vmux_command/src/lib.rs` (add `pub mod island;`)
- Test: in `crates/vmux_command/src/island.rs` (`#[cfg(test)]`)

- [ ] **Step 1: Write the failing test** (mirror the existing command-event round-trip tests in
  `crates/vmux_command/src/event.rs`)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn island_render_event_roundtrips() {
        let ev = IslandRenderEvent {
            seq: 7,
            state: IslandState::Activity(IslandActivity {
                kind: IslandActivityKind::Agent,
                label: "vibe · editing".into(),
                progress: Some(0.62),
            }),
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&ev).unwrap();
        let back: IslandRenderEvent =
            rkyv::from_bytes::<IslandRenderEvent, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(back, ev);
        assert_eq!(ISLAND_RENDER_EVENT, "island-render");
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vmux_command island_render_event_roundtrips`
Expected: FAIL (module/type not found).

- [ ] **Step 3: Implement the types** (match the derive set used by `CommandBarOpenEvent` in
  `event.rs` — `Archive, Serialize, Deserialize`, `serde`, `PartialEq`, `Clone`, `Debug`)

```rust
use rkyv::{Archive, Deserialize, Serialize};

pub const ISLAND_RENDER_EVENT: &str = "island-render";

#[derive(Archive, Serialize, Deserialize, serde::Serialize, serde::Deserialize,
         Clone, Debug, PartialEq)]
pub struct IslandRenderEvent {
    pub seq: u64,
    pub state: IslandState,
}

#[derive(Archive, Serialize, Deserialize, serde::Serialize, serde::Deserialize,
         Clone, Debug, PartialEq)]
pub enum IslandState {
    Idle,
    Search,
    Activity(IslandActivity),
    Notify(IslandNotice),
}

#[derive(Archive, Serialize, Deserialize, serde::Serialize, serde::Deserialize,
         Clone, Debug, PartialEq)]
pub struct IslandActivity {
    pub kind: IslandActivityKind,
    pub label: String,
    pub progress: Option<f32>,
}

#[derive(Archive, Serialize, Deserialize, serde::Serialize, serde::Deserialize,
         Clone, Copy, Debug, PartialEq)]
pub enum IslandActivityKind { Agent, Terminal, Media, Download }

#[derive(Archive, Serialize, Deserialize, serde::Serialize, serde::Deserialize,
         Clone, Debug, PartialEq)]
pub struct IslandNotice { pub label: String, pub ttl_ms: u32 }
```

Add to `crates/vmux_command/src/lib.rs`: `pub mod island;`

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p vmux_command island_render_event_roundtrips`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_command/src/island.rs crates/vmux_command/src/lib.rs
git commit -m "feat(island): rkyv render-event payload types"
```

## Task 2: Morph state machine (pure reducer)

**Files:**
- Create: `crates/vmux_layout/src/island/state.rs`
- Test: same file (`#[cfg(test)]`)

- [ ] **Step 1: Write the failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use vmux_command::island::{IslandActivity, IslandActivityKind, IslandState};

    fn act(kind: IslandActivityKind) -> IslandActivity {
        IslandActivity { kind, label: "x".into(), progress: None }
    }

    #[test]
    fn search_overrides_activity_then_restores() {
        let mut m = IslandMachine::default();
        m.apply(IslandInput::Activity(act(IslandActivityKind::Agent)));
        assert!(matches!(m.render_state(), IslandState::Activity(_)));
        m.apply(IslandInput::ExpandSearch);
        assert!(matches!(m.render_state(), IslandState::Search)); // search wins
        m.apply(IslandInput::Collapse);
        assert!(matches!(m.render_state(), IslandState::Activity(_))); // restores activity
    }

    #[test]
    fn idle_when_nothing_active() {
        let mut m = IslandMachine::default();
        m.apply(IslandInput::Activity(act(IslandActivityKind::Terminal)));
        m.apply(IslandInput::ActivityEnded(IslandActivityKind::Terminal));
        assert!(matches!(m.render_state(), IslandState::Idle));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vmux_layout island::state`
Expected: FAIL (type not found).

- [ ] **Step 3: Implement the reducer** (search has top priority; otherwise the most recent live
  activity; else idle. Notices are transient and tracked separately by the handler's timer.)

```rust
use vmux_command::island::{IslandActivity, IslandActivityKind, IslandState};

#[derive(Clone, Debug)]
pub enum IslandInput {
    ExpandSearch,
    Collapse,
    Activity(IslandActivity),
    ActivityEnded(IslandActivityKind),
}

#[derive(Default)]
pub struct IslandMachine {
    searching: bool,
    activities: Vec<IslandActivity>, // most-recent last
}

impl IslandMachine {
    pub fn apply(&mut self, input: IslandInput) {
        match input {
            IslandInput::ExpandSearch => self.searching = true,
            IslandInput::Collapse => self.searching = false,
            IslandInput::Activity(a) => {
                self.activities.retain(|x| x.kind != a.kind);
                self.activities.push(a);
            }
            IslandInput::ActivityEnded(kind) => self.activities.retain(|x| x.kind != kind),
        }
    }

    pub fn render_state(&self) -> IslandState {
        if self.searching {
            IslandState::Search
        } else if let Some(a) = self.activities.last() {
            IslandState::Activity(a.clone())
        } else {
            IslandState::Idle
        }
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p vmux_layout island::state`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_layout/src/island/state.rs
git commit -m "feat(island): pure morph state machine"
```

## Task 3: Island ECS messages + plugin scaffold

**Files:**
- Create: `crates/vmux_layout/src/island/event.rs`, `crates/vmux_layout/src/island/plugin.rs`,
  `crates/vmux_layout/src/island.rs`
- Modify: `crates/vmux_layout/src/lib.rs`
- Test: `crates/vmux_layout/src/island/plugin.rs` (`#[cfg(test)]`)

- [ ] **Step 1: Write the failing test** (plugin registers message types; mirror existing
  message-registration tests in the layout crate)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::*;

    #[test]
    fn plugin_registers_island_messages() {
        let mut app = App::new();
        app.add_plugins(IslandPlugin);
        // Messages exist => sending compiles & does not panic.
        app.world_mut().resource_mut::<Messages<SummonCommandBar>>().send(SummonCommandBar);
        app.update();
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vmux_layout island::plugin`
Expected: FAIL (types not found).

- [ ] **Step 3: Implement messages, marker, and plugin**

`island/event.rs`:

```rust
use bevy::prelude::*;
use vmux_command::island::{IslandActivity, IslandActivityKind, IslandNotice};

#[derive(Component)]
pub struct Island; // marker on the island OSR webview entity

#[derive(Message, Clone)]
pub enum IslandEvent {
    ExpandSearch,
    Collapse,
    Activity(IslandActivity),
    ActivityEnded(IslandActivityKind),
    Notify(IslandNotice),
}

#[derive(Message, Clone)]
pub struct SummonCommandBar;
#[derive(Message, Clone)]
pub struct IslandPanelShow;
#[derive(Message, Clone)]
pub struct IslandPanelHide;
#[derive(Message, Clone, Copy)]
pub struct IslandPanelResize { pub width: f32, pub height: f32 }
#[derive(Message, Clone)]
pub struct IslandPanelDismissed;
```

`island/plugin.rs`:

```rust
use bevy::prelude::*;
use super::event::*;
use super::handler;

pub struct IslandPlugin;

impl Plugin for IslandPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<IslandEvent>()
            .add_message::<SummonCommandBar>()
            .add_message::<IslandPanelShow>()
            .add_message::<IslandPanelHide>()
            .add_message::<IslandPanelResize>()
            .add_message::<IslandPanelDismissed>()
            .add_systems(Update, (handler::drive_island_state, handler::summon_to_expand));
    }
}
```

`island.rs` (module aggregator — filename-module pattern, NOT mod.rs):

```rust
pub mod event;
pub mod handler;
pub mod page;
pub mod plugin;
pub mod state;

pub use event::Island;
pub use plugin::IslandPlugin;
```

In `crates/vmux_layout/src/lib.rs` add `pub mod island;` and register in the layout plugin set
where other sub-plugins are added: `.add_plugins(island::IslandPlugin)`.

- [ ] **Step 4: Implement the two handler systems referenced above** (so it compiles)

`island/handler.rs`:

```rust
use bevy::prelude::*;
use vmux_command::island::{IslandRenderEvent, ISLAND_RENDER_EVENT};
use vmux_ui::bin_ipc_envelope::BinHostEmitEvent; // path used by command_bar handler
use super::event::*;
use super::state::{IslandInput, IslandMachine};

#[derive(Resource, Default)]
pub struct IslandSeq(pub u64);

pub fn summon_to_expand(
    mut summon: MessageReader<SummonCommandBar>,
    mut out: MessageWriter<IslandEvent>,
) {
    if summon.read().next().is_some() {
        out.write(IslandEvent::ExpandSearch);
    }
}

pub fn drive_island_state(
    mut machine: Local<IslandMachine>,
    mut seq: Local<u64>,
    mut events: MessageReader<IslandEvent>,
    island_q: Query<Entity, With<Island>>,
    mut commands: Commands,
) {
    let mut changed = false;
    for ev in events.read() {
        match ev.clone() {
            IslandEvent::ExpandSearch => machine.apply(IslandInput::ExpandSearch),
            IslandEvent::Collapse => machine.apply(IslandInput::Collapse),
            IslandEvent::Activity(a) => machine.apply(IslandInput::Activity(a)),
            IslandEvent::ActivityEnded(k) => machine.apply(IslandInput::ActivityEnded(k)),
            IslandEvent::Notify(_n) => { /* notices handled in Task 11+; ignore in P1 */ }
        }
        changed = true;
    }
    if !changed { return; }
    let Ok(entity) = island_q.single() else { return; };
    *seq += 1;
    let payload = IslandRenderEvent { seq: *seq, state: machine.render_state() };
    commands.trigger(BinHostEmitEvent::from_rkyv(entity, ISLAND_RENDER_EVENT, &payload));
}
```

> Note: confirm the `BinHostEmitEvent` import path against `command_bar/handler.rs` (it imports it
> there); reuse the exact path. `add_message`/`Messages`/`MessageReader` are the project's Bevy 0.19
> message API as used across the layout crate.

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test -p vmux_layout island::plugin`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_layout/src/island.rs crates/vmux_layout/src/island/ crates/vmux_layout/src/lib.rs
git commit -m "feat(island): ECS messages, marker, plugin, state-drive system"
```

## Task 4: Island Dioxus page (idle + activity states)

**Files:**
- Create: `crates/vmux_layout/src/island/page.rs`
- Modify: `crates/vmux_server/build.rs` (track new src), `crates/vmux_server/assets/index.css`
  (`@source`)
- Test: `crates/vmux_layout/src/island/page.rs` (`#[cfg(test)]` source-scrape, mirroring
  `command_bar/style.rs` tests)

- [ ] **Step 1: Write the failing test** (source-scrape guards, the project's page-test style)

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn island_page_listens_for_render_event_and_has_states() {
        let src = include_str!("page.rs");
        assert!(src.contains("island-render"));
        assert!(src.contains("fn IslandRoot"));
        assert!(src.contains("IslandState::Idle"));
        assert!(src.contains("IslandState::Search"));
        assert!(src.contains("IslandState::Activity"));
    }

    #[test]
    fn idle_pill_is_static_no_animation_classes() {
        let src = include_str!("page.rs");
        // idle must not animate (zero idle repaint budget)
        assert!(src.contains("ISLAND_IDLE_CLASS"));
        assert!(!src.contains("animate-")); // no tailwind animation utilities on idle
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vmux_layout island::page`
Expected: FAIL.

- [ ] **Step 3: Implement the page** (Dioxus component reusing the command-bar component for the
  Search state; follow the rsx/class pattern from `command_bar/page.rs`). Keep transparent body so
  the native glass shows through; idle pill static.

```rust
use dioxus::prelude::*;
use vmux_command::island::{IslandRenderEvent, IslandState, ISLAND_RENDER_EVENT};
use crate::command_bar::page::CommandBar; // reuse search UI

pub const ISLAND_IDLE_CLASS: &str =
    "inline-flex items-center gap-2 rounded-full px-3.5 py-1.5 text-sm text-foreground";

#[component]
pub fn IslandRoot() -> Element {
    let mut state = use_signal(|| IslandState::Idle);

    // Subscribe to host render events (same host-emit bridge the command bar uses).
    use_island_render_subscription(move |ev: IslandRenderEvent| state.set(ev.state));

    // Body transparent; the native NSGlassEffectView is the backdrop.
    rsx! {
        div { class: "flex items-center justify-center bg-transparent",
            match &*state.read() {
                IslandState::Idle => rsx!{ div { class: ISLAND_IDLE_CLASS, span { "vmux" } } },
                IslandState::Search => rsx!{ CommandBar { native_windowed: true } },
                IslandState::Activity(a) => rsx!{
                    div { class: "inline-flex items-center gap-2 rounded-full px-3.5 py-1.5 text-sm",
                        span { class: "h-2 w-2 rounded-full bg-accent" }
                        span { "{a.label}" }
                    }
                },
                IslandState::Notify(n) => rsx!{
                    div { class: "inline-flex items-center gap-2 rounded-full px-3.5 py-1.5 text-sm",
                        span { "{n.label}" }
                    }
                },
            }
        }
    }
}
```

> The exact host-event subscription helper (`use_island_render_subscription`) mirrors the
> command-bar page's existing render-event hook. Reuse that hook's implementation pattern from
> `command_bar/page.rs` (search for where it installs the `COMMAND_BAR_OPEN_EVENT` observer) and
> parameterize it for `ISLAND_RENDER_EVENT`. After P2, `CommandBar` is the shared search component.

Register the island page route in the page server the same way the command-bar page is registered
(find `COMMAND_BAR_PAGE_URL` wiring in `vmux_server`/page registry and add an `ISLAND_PAGE_URL`).

- [ ] **Step 4: Wire build tracking**

In `crates/vmux_server/build.rs` add the island page src path to `track_manifest_rel_paths` (per the
WASM-rebuild-tracking requirement). In `crates/vmux_server/assets/index.css` ensure the island
crate/src is covered by the `@source` globs so Tailwind picks up its classes.

- [ ] **Step 5: Run test + a wasm typecheck**

Run: `cargo test -p vmux_layout island::page`
Expected: PASS.
Run: `cargo check -p vmux_layout --target wasm32-unknown-unknown`
Expected: compiles (page builds for WASM).

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_layout/src/island/page.rs crates/vmux_server/build.rs crates/vmux_server/assets/index.css
git commit -m "feat(island): dioxus page (idle/activity states, search reuse)"
```

## Task 5: Island OSR webview entity

**Files:**
- Modify: `crates/vmux_layout/src/window.rs` (spawn the `Island` webview near the `Modal` spawn,
  ~line 390)
- Test: `crates/vmux_layout/src/window.rs` (`#[cfg(test)]`, mirror `command_bar_modal_*` tests)

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn island_webview_is_osr_native_overlay() {
    let mut app = setup_window_app();
    app.update();
    let island = app.world_mut()
        .query_filtered::<Entity, With<crate::island::Island>>()
        .single(app.world())
        .expect("island");
    use bevy_cef::prelude::{WebviewNativeOverlay, WebviewWindowed};
    assert!(app.world().get::<WebviewNativeOverlay>(island).is_some()); // OSR overlay
    assert!(app.world().get::<WebviewWindowed>(island).is_none());      // not windowed
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vmux_layout island_webview_is_osr_native_overlay`
Expected: FAIL.

- [ ] **Step 3: Spawn the island webview** (in `setup`, alongside the existing `Modal` spawn; copy
  the OSR/native-overlay component set from `Modal` at window.rs:390-420, swap the marker + URL, and
  do NOT make it a `ChildOf` the layout root — it renders into the island panel, composited
  natively, so it does not participate in the Bevy UI tree layout). Keep `Visibility::Hidden`/zero
  size until first render.

```rust
commands.spawn((
    (
        crate::island::Island,
        HostWindow(pw),
        Browser,
        WebviewTransparent,
        bevy_cef::prelude::WebviewNativeOverlay,
        bevy_cef::prelude::CefIgnorePinchZoom,
    ),
    WebviewSource::new(ISLAND_PAGE_URL),
    WebviewSize(Vec2::new(360.0, 44.0)), // idle pill size; resized by the page
    Mesh3d(meshes.add(Plane3d::new(Vec3::Z, Vec2::splat(0.5)))),
    MeshMaterial3d(webview_mt.add(WebviewExtendStandardMaterial::default())),
    Transform::default(),
    GlobalTransform::default(),
    Visibility::Hidden,
    Pickable::IGNORE,
));
```

Import `ISLAND_PAGE_URL` from the page registry. (Note: unlike the `Modal`, the island webview is
NOT `WebviewNativeLiquidGlass` — the glass is the native `NSGlassEffectView` backdrop, not a
per-webview effect.)

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p vmux_layout island_webview_is_osr_native_overlay`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_layout/src/window.rs
git commit -m "feat(island): spawn OSR island webview entity"
```

## Task 6: Native island panel + glass backdrop + OSR compositing

**Files:**
- Create: `crates/vmux_desktop/src/dynamic_island.rs`
- Modify: `crates/vmux_desktop/src/lib.rs` (add `mod dynamic_island;` + plugin)
- Test: `crates/vmux_desktop/src/dynamic_island.rs` (`#[cfg(test)]` source-scrape guards, mirroring
  `glass.rs` tests)

- [ ] **Step 1: Write the failing guard tests** (the project's native-code test style — assert the
  source uses the right primitives + the idle-CPU invariant)

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn island_panel_is_nonactivating_all_spaces_floating() {
        let src = include_str!("dynamic_island.rs");
        assert!(src.contains("NSWindowStyleMask::NonactivatingPanel"));
        assert!(src.contains("CanJoinAllSpaces"));
        assert!(src.contains("FullScreenAuxiliary"));
        assert!(src.contains("NSGlassEffectView"));
    }

    #[test]
    fn island_does_not_force_continuous_update_mode() {
        let src = include_str!("dynamic_island.rs");
        assert!(!src.contains("UpdateMode::Continuous"));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p vmux_desktop dynamic_island`
Expected: FAIL.

- [ ] **Step 3: Implement the panel + glass + compositing** (model on `glass.rs::install_window_glass`
  for the panel/glass, and `glass.rs::sync_layout_overlay` for the OSR `CALayer` compositing). State
  in a `NonSend` resource.

```rust
use bevy::prelude::*;
use vmux_layout::island::Island;

#[derive(Default)]
struct IslandPanel {
    panel: Option<objc2::rc::Retained<objc2_app_kit::NSPanel>>,
    glass: Option<objc2::rc::Retained<objc2_app_kit::NSGlassEffectView>>,
    content_layer: Option<objc2::rc::Retained<objc2_quartz_core::CALayer>>,
    held: Option<bevy_cef_core::prelude::AcceleratedFrame>,
}

pub(crate) struct DynamicIslandPlugin;
impl Plugin for DynamicIslandPlugin {
    fn build(&self, app: &mut App) {
        app.init_non_send::<IslandPanel>()
            .add_systems(Startup, install_island_panel)
            .add_systems(Last, (sync_island_overlay, apply_island_resize).chain());
    }
}

#[cfg(target_os = "macos")]
fn install_island_panel(mut state: NonSendMut<IslandPanel>) {
    use objc2::{MainThreadMarker, MainThreadOnly, rc::Retained};
    use objc2_app_kit::{
        NSBackingStoreType, NSColor, NSGlassEffectView, NSPanel, NSScreen, NSView,
        NSWindowCollectionBehavior, NSWindowStyleMask,
    };
    use objc2_foundation::{NSPoint, NSRect, NSSize};
    use objc2_quartz_core::CALayer;

    if state.panel.is_some() { return; }
    let Some(mtm) = MainThreadMarker::new() else { return; };

    let w = 360.0; let h = 44.0; let radius = 22.0;
    let panel = NSPanel::initWithContentRect_styleMask_backing_defer(
        NSPanel::alloc(mtm),
        NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(w, h)),
        NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel,
        NSBackingStoreType::Buffered,
        false,
    );
    let win: &objc2_app_kit::NSWindow = panel.as_super();
    win.setOpaque(false);
    win.setBackgroundColor(Some(&NSColor::clearColor()));
    win.setHasShadow(true);
    win.setLevel(objc2_app_kit::NSFloatingWindowLevel);
    win.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::FullScreenAuxiliary
            | NSWindowCollectionBehavior::IgnoresCycle,
    );
    win.setIgnoresMouseEvents(true); // idle: click-through
    panel.setBecomesKeyOnlyIfNeeded(true);

    // contentView (clear, layer-backed)
    let content = NSView::initWithFrame(
        NSView::alloc(mtm),
        NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(w, h)),
    );
    content.setWantsLayer(true);

    // glass backdrop (rounded). Default to the dark tint (mockup variant C) for P1.
    // The appearance-mode binding (Light->A / Dark->C / Device->OS) depends on PR #172's
    // CefColorScheme; add a `sync_island_glass` system to drive style/tint once #172 lands.
    let glass: Retained<NSGlassEffectView> = NSGlassEffectView::new(mtm);
    glass.setStyle(objc2_app_kit::NSGlassEffectViewStyle::Clear);
    glass.setTintColor(Some(&NSColor::colorWithWhite_alpha(0.0, 0.45)));
    let glass_view: &NSView = &glass;
    glass_view.setFrame(NSRect::new(NSPoint::new(0.0,0.0), NSSize::new(w,h)));
    if let Some(layer) = glass_view.layer() {
        layer.setCornerRadius(radius);
        layer.setMasksToBounds(true);
    }
    content.addSubview(glass_view);

    // OSR content layer (above glass), masked to the same rounding
    let layer: Retained<CALayer> = CALayer::new();
    layer.setOpaque(false);
    layer.setCornerRadius(radius);
    layer.setMasksToBounds(true);
    layer.setZPosition(10.0);
    if let Some(host) = content.layer() { host.addSublayer(&layer); }

    win.setContentView(Some(&content));
    position_top_center(win, &NSScreen::mainScreen(mtm), w, h);
    win.orderFrontRegardless();

    state.glass = Some(glass);
    state.content_layer = Some(layer);
    state.panel = Some(panel);
}

#[cfg(target_os = "macos")]
fn position_top_center(win: &objc2_app_kit::NSWindow, screen: &Option<objc2::rc::Retained<objc2_app_kit::NSScreen>>, w: f64, h: f64) {
    use objc2_foundation::{NSPoint, NSRect};
    let Some(screen) = screen else { return; };
    let vf: NSRect = screen.visibleFrame();
    let x = vf.origin.x + (vf.size.width - w) / 2.0;
    let y = vf.origin.y + vf.size.height - h - 8.0; // 8pt below top
    win.setFrameOrigin(NSPoint::new(x, y));
}

#[cfg(target_os = "macos")]
fn sync_island_overlay(
    mut state: NonSendMut<IslandPanel>,
    island_q: Query<Entity, With<Island>>,
    windows: Query<&bevy::window::Window>,
    overlay_frames: Res<bevy_cef::prelude::NativeOverlayFrames>,
) {
    use objc2::runtime::AnyObject;
    let Ok(island_e) = island_q.single() else { return; };
    let next = overlay_frames.0.lock().ok().and_then(|mut m| m.remove(&island_e));
    if next.is_none() && state.held.is_none() { return; }
    let Some(layer) = state.content_layer.clone() else { return; };
    if let Some(frame) = next {
        let io = frame.io_surface as *mut AnyObject;
        if !io.is_null() {
            let scale = windows.iter().next().map(|w| w.resolution.scale_factor() as f64).unwrap_or(2.0);
            layer.setContentsScale(scale);
            unsafe { layer.setContents(Some(&*io)) };
            state.held = Some(frame);
        }
    }
}

#[cfg(target_os = "macos")]
fn apply_island_resize(/* fills in Task 8 */) {}
```

In `crates/vmux_desktop/src/lib.rs`: `mod dynamic_island;` and add
`.add_plugins(dynamic_island::DynamicIslandPlugin)` where the other desktop plugins are registered.
Verify objc2-app-kit feature flags include `NSPanel`, `NSGlassEffectView`, `NSScreen` (add any
missing to `crates/vmux_desktop/Cargo.toml`).

- [ ] **Step 4: Warm-build then verify it compiles**

Run (background, once): `cargo build -p vmux_desktop`
Then: `cargo test -p vmux_desktop dynamic_island`
Expected: PASS (guard tests) and the crate compiles.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_desktop/src/dynamic_island.rs crates/vmux_desktop/src/lib.rs crates/vmux_desktop/Cargo.toml
git commit -m "feat(island): native NSPanel + glass backdrop + OSR compositing"
```

## Task 7: Animated frame resize from page size

**Files:**
- Modify: `crates/vmux_layout/src/island/handler.rs` (emit `IslandPanelResize` from the page size
  event), `crates/vmux_desktop/src/dynamic_island.rs` (`apply_island_resize`)
- Test: `crates/vmux_layout/src/island/handler.rs` (`#[cfg(test)]`)

- [ ] **Step 1: Write the failing test** (the island size observer maps the page size event to an
  `IslandPanelResize`; reuse the command-bar size-event mechanism)

```rust
#[test]
fn size_event_emits_panel_resize() {
    use bevy::prelude::*;
    let mut app = App::new();
    app.add_plugins(crate::island::IslandPlugin);
    app.add_systems(Update, crate::island::handler::island_size_to_resize);
    // simulate the island webview reporting a size (mechanism mirrors CommandBarNativeSize)
    // ... send the size message ...
    app.update();
    let resizes = app.world().resource::<Messages<crate::island::event::IslandPanelResize>>();
    assert!(resizes.len() >= 1);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vmux_layout size_event_emits_panel_resize`
Expected: FAIL.

- [ ] **Step 3: Implement `island_size_to_resize`** (subscribe to the same size event the command
  bar uses — `CommandBarNativeSize`/the size observer in `command_bar/handler.rs:389-404` — but for
  the `Island` entity; emit `IslandPanelResize`). Register it in `IslandPlugin`.

```rust
pub fn island_size_to_resize(
    island_q: Query<&CommandBarNativeSize, (With<Island>, Changed<CommandBarNativeSize>)>,
    mut out: MessageWriter<IslandPanelResize>,
) {
    if let Ok(size) = island_q.single() {
        out.write(IslandPanelResize { width: size.width, height: size.height });
    }
}
```

> Reuse the existing `CommandBarNativeSize` component + the page-side size emitter (the island page
> already emits sizes because it embeds the command-bar size hook). Confirm the component path from
> `command_bar/handler.rs:404`.

- [ ] **Step 4: Implement `apply_island_resize`** (native, animate the frame, keep top-center)

```rust
#[cfg(target_os = "macos")]
fn apply_island_resize(
    state: NonSend<IslandPanel>,
    mut resizes: MessageReader<vmux_layout::island::event::IslandPanelResize>,
) {
    use objc2_app_kit::{NSAnimationContext, NSScreen};
    use objc2_foundation::{NSPoint, NSRect, NSSize};
    let Some(panel) = &state.panel else { return; };
    let Some(r) = resizes.read().last().copied() else { return; };
    let win: &objc2_app_kit::NSWindow = panel.as_super();
    let mtm = objc2::MainThreadMarker::new().unwrap();
    let vf = NSScreen::mainScreen(mtm).map(|s| s.visibleFrame());
    let (w, h) = (r.width as f64, r.height as f64);
    let (x, y) = match vf {
        Some(vf) => (vf.origin.x + (vf.size.width - w)/2.0, vf.origin.y + vf.size.height - h - 8.0),
        None => { let f = win.frame(); (f.origin.x, f.origin.y) }
    };
    NSAnimationContext::runAnimationGroup(&objc2::rc::autoreleasepool(|_| {
        // 0.18s ease; setFrame animated
    }));
    win.setFrame_display_animate(NSRect::new(NSPoint::new(x,y), NSSize::new(w,h)), true, true);
    if let Some(layer) = &state.content_layer {
        layer.setFrame(NSRect::new(NSPoint::new(0.0,0.0), NSSize::new(w,h)));
    }
    if let Some(glass) = &state.glass {
        let v: &objc2_app_kit::NSView = glass; v.setFrame(NSRect::new(NSPoint::new(0.0,0.0), NSSize::new(w,h)));
    }
}
```

> Simplify the animation call if `runAnimationGroup` ergonomics are awkward — `setFrame_display_animate`
> already animates. The key invariant: re-anchor top-center on every resize.

- [ ] **Step 5: Run the layout test + build desktop**

Run: `cargo test -p vmux_layout size_event_emits_panel_resize`
Expected: PASS.
Run: `cargo build -p vmux_desktop`
Expected: compiles.

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_layout/src/island/handler.rs crates/vmux_desktop/src/dynamic_island.rs
git commit -m "feat(island): animate panel frame to page-reported size"
```

## Task 8: P1 temporary morph trigger + runtime verification

**Files:**
- Modify: `crates/vmux_desktop/src/dynamic_island.rs` (temporary dev keybind → `IslandEvent`)

- [ ] **Step 1: Add a temporary trigger** (UNCONDITIONAL, default-on per the debugging rule — a key
  combo that cycles Idle → Activity → Search → Collapse by sending `IslandEvent`s). Mark it clearly
  with `// TEMP P1 trigger — removed in Task 9`.

```rust
fn temp_island_trigger(
    keys: Res<ButtonInput<KeyCode>>,
    mut out: MessageWriter<vmux_layout::island::event::IslandEvent>,
) {
    use vmux_command::island::{IslandActivity, IslandActivityKind};
    if keys.just_pressed(KeyCode::F8) {
        out.write(vmux_layout::island::event::IslandEvent::Activity(IslandActivity {
            kind: IslandActivityKind::Agent, label: "vibe · editing 3 files".into(), progress: Some(0.6),
        }));
    }
    if keys.just_pressed(KeyCode::F9) {
        out.write(vmux_layout::island::event::IslandEvent::ExpandSearch);
    }
    if keys.just_pressed(KeyCode::F10) {
        out.write(vmux_layout::island::event::IslandEvent::Collapse);
    }
}
```

Register in `DynamicIslandPlugin` (`Update`).

- [ ] **Step 2: Build and run the app** (warm target already built)

Run: `cargo run -p vmux_desktop` (or the project's `make dev` if that's the entry — but do NOT spawn
unbounded loops; a single run instance)

- [ ] **Step 3: Runtime verification (P1 acceptance)** — observe, with the app running:
  - Idle glass pill visible top-center on the main display.
  - It stays visible over another app and on a second Space / over a fullscreen app.
  - `F8` morphs to the agent activity pill (frame animates); `F9` expands; `F10` collapses to idle.
  - With the pill idle, confirm idle CPU is unchanged vs. baseline (Activity Monitor / `top`): no
    continuous repaint.

- [ ] **Step 4: Commit** (trigger is temporary but committed so P1 is reproducible; removed in T9)

```bash
git add crates/vmux_desktop/src/dynamic_island.rs
git commit -m "feat(island): temporary P1 morph trigger + shell runtime-verified"
```

---

# Phase P2 — Search & unify

## Task 9: Extract the command-bar open-payload gatherer

**Files:**
- Modify: `crates/vmux_layout/src/command_bar/handler.rs`
- Test: existing `cargo test -p vmux_layout` (behavior-preserving refactor)

- [ ] **Step 1: Run the existing command-bar tests to capture the green baseline**

Run: `cargo test -p vmux_layout command_bar`
Expected: PASS (record the count).

- [ ] **Step 2: Extract `CommandBarPayloadSources` (SystemParam) + `build_command_bar_open_payload`**
  from `handle_open_command_bar` (the spaces/tabs/commands/pages/url gathering, ~handler.rs:600-919).
  The free fn returns `CommandBarOpenEvent`; `handle_open_command_bar` calls it and keeps all its
  `Modal`-entity reveal/display logic. Replace the thin `command_bar_open_payload` (handler.rs:962).

```rust
#[derive(SystemParam)]
pub(crate) struct CommandBarPayloadSources<'w, 's> {
    // move the queries/resources handle_open_command_bar uses for data gathering here
    // (spaces snapshot, tabs, command_list inputs, pages, current url, ...)
    // -- exact fields copied from the current system signature
}

pub(crate) fn build_command_bar_open_payload(
    open_id: u64,
    native_windowed: bool,
    target: Option<vmux_command::open_target::OpenTarget>,
    sources: &CommandBarPayloadSources,
) -> CommandBarOpenEvent {
    // body moved verbatim from handle_open_command_bar's gathering + the old command_bar_open_payload
}
```

- [ ] **Step 3: Run the tests again — must match the baseline exactly**

Run: `cargo test -p vmux_layout command_bar`
Expected: PASS, same count (no behavior change). Also run the source-scrape suites:
`cargo test -p vmux_layout` and confirm `tests/page_source.rs` passes.

- [ ] **Step 4: Commit**

```bash
git add crates/vmux_layout/src/command_bar/handler.rs
git commit -m "refactor(command-bar): extract reusable open-payload gatherer"
```

## Task 10: Island Search state emits the open payload

**Files:**
- Modify: `crates/vmux_layout/src/island/handler.rs`
- Test: `crates/vmux_layout/src/island/handler.rs` (`#[cfg(test)]`)

- [ ] **Step 1: Write the failing test** (on `ExpandSearch`, the island emits a
  `COMMAND_BAR_OPEN_EVENT` to the `Island` entity with `native_windowed = true`)

```rust
#[test]
fn expand_search_emits_open_event_to_island() {
    // build app with IslandPlugin + a stub Island entity + minimal payload sources,
    // send IslandEvent::ExpandSearch, run, assert a BinHostEmitEvent targeting the Island
    // entity with channel COMMAND_BAR_OPEN_EVENT was triggered.
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vmux_layout expand_search_emits_open_event_to_island`
Expected: FAIL.

- [ ] **Step 3: Implement `expand_search_open` system** (on `IslandEvent::ExpandSearch`, build the
  payload via `build_command_bar_open_payload(open_id, true, None, &sources)` and trigger
  `BinHostEmitEvent::from_rkyv(island_e, COMMAND_BAR_OPEN_EVENT, &payload)`). Register in
  `IslandPlugin`.

```rust
use crate::command_bar::handler::{build_command_bar_open_payload, CommandBarPayloadSources};
use vmux_command::event::COMMAND_BAR_OPEN_EVENT;

pub fn expand_search_open(
    mut events: MessageReader<IslandEvent>,
    island_q: Query<Entity, With<Island>>,
    sources: CommandBarPayloadSources,
    mut commands: Commands,
) {
    if !events.read().any(|e| matches!(e, IslandEvent::ExpandSearch)) { return; }
    let Ok(island_e) = island_q.single() else { return; };
    let open_id = /* now_millis() as u64, as in handler.rs:898 */ 0;
    let payload = build_command_bar_open_payload(open_id, true, None, &sources);
    commands.trigger(BinHostEmitEvent::from_rkyv(island_e, COMMAND_BAR_OPEN_EVENT, &payload));
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p vmux_layout expand_search_emits_open_event_to_island`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_layout/src/island/handler.rs
git commit -m "feat(island): expand-search emits command-bar open payload"
```

## Task 11: Global hotkey + waker + settings

**Files:**
- Modify: `crates/vmux_desktop/Cargo.toml` (add `global-hotkey`), `crates/vmux_setting/src/lib.rs`
  (setting), `crates/vmux_desktop/src/dynamic_island.rs` (manager + waker + poll)
- Test: `crates/vmux_setting/src/lib.rs` (parse) + a hotkey-parse unit test in `dynamic_island.rs`

- [ ] **Step 1: Write the failing tests**

```rust
// in dynamic_island.rs
#[test]
fn default_hotkey_parses() {
    use global_hotkey::hotkey::HotKey;
    let hk: HotKey = "super+shift+Space".parse().unwrap();
    let _ = hk.id();
}
// in vmux_setting
#[test]
fn global_hotkey_setting_defaults_to_none_then_code_default() {
    let s = Settings::default();
    assert!(s.command_bar.global_hotkey.is_none()); // absent => code default applies
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p vmux_desktop default_hotkey_parses` and
`cargo test -p vmux_setting global_hotkey_setting_defaults`
Expected: FAIL.

- [ ] **Step 3: Add the setting** (no auto-seed — `Option<String>`, absent means code default)

```rust
// vmux_setting: in the command_bar settings struct
pub global_hotkey: Option<String>,
```

- [ ] **Step 4: Add `global-hotkey` dep + manager + waker thread + poll system**

`Cargo.toml`: `global-hotkey = "0.6"` (pin to the current published version).

```rust
use global_hotkey::{GlobalHotKeyManager, GlobalHotKeyEvent, HotKeyState, hotkey::HotKey};

struct IslandHotkey { _mgr: GlobalHotKeyManager, id: u32 }

#[cfg(target_os = "macos")]
fn install_global_hotkey(world: &mut World) {
    // main thread (Startup runs on main). Parse from settings or default.
    let chord = /* settings.command_bar.global_hotkey */ None
        .unwrap_or_else(|| "super+shift+Space".to_string());
    let hk: HotKey = chord.parse().unwrap_or_else(|_| "super+shift+Space".parse().unwrap());
    let mgr = GlobalHotKeyManager::new().expect("hotkey mgr");
    mgr.register(hk).expect("register hotkey");
    let id = hk.id();

    // Waker thread: block on recv, wake the loop + enqueue.
    let proxy = world.get_resource::<bevy::winit::EventLoopProxyWrapper>().cloned();
    std::thread::spawn(move || {
        let rx = GlobalHotKeyEvent::receiver();
        while let Ok(ev) = rx.recv() {
            if ev.id == id && ev.state == HotKeyState::Pressed {
                HOTKEY_PENDING.store(true, std::sync::atomic::Ordering::SeqCst);
                if let Some(p) = &proxy { let _ = p.send_event(bevy::winit::WinitUserEvent::WakeUp); }
            }
        }
    });
    world.insert_non_send_resource(IslandHotkey { _mgr: mgr, id });
}

static HOTKEY_PENDING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn drain_global_hotkey(mut out: MessageWriter<vmux_layout::island::event::SummonCommandBar>) {
    if HOTKEY_PENDING.swap(false, std::sync::atomic::Ordering::SeqCst) {
        out.write(vmux_layout::island::event::SummonCommandBar);
    }
}
```

Register `drain_global_hotkey` in `Update`; call `install_global_hotkey` in `Startup` (exclusive
system so it can take `&mut World` for the proxy + NonSend insert). Confirm the
`EventLoopProxyWrapper`/`WinitUserEvent::WakeUp` paths against `background_lifecycle.rs:4`.

- [ ] **Step 5: Run tests + build**

Run: `cargo test -p vmux_setting global_hotkey_setting_defaults` and
`cargo test -p vmux_desktop default_hotkey_parses`
Expected: PASS.
Run: `cargo build -p vmux_desktop`
Expected: compiles.

- [ ] **Step 6: Remove the temporary P1 trigger** (`temp_island_trigger` from Task 8) and its
  registration.

- [ ] **Step 7: Commit**

```bash
git add crates/vmux_desktop/Cargo.toml crates/vmux_setting/src/lib.rs crates/vmux_desktop/src/dynamic_island.rs
git commit -m "feat(island): global hotkey (Cmd+Shift+Space) + waker; drop temp trigger"
```

## Task 12: Cmd+K → island expand; native key forwarding + mouse passthrough toggle

**Files:**
- Modify: `crates/vmux_layout/src/command_bar/handler.rs` (repoint the open command to
  `SummonCommandBar`/`IslandEvent::ExpandSearch`), `crates/vmux_desktop/src/dynamic_island.rs`
  (key NSView subclass + forwarding, makeKey + mouse passthrough on expand/collapse)
- Test: source-scrape guard in `dynamic_island.rs`

- [ ] **Step 1: Write the failing guard test**

```rust
#[test]
fn island_forwards_keys_and_toggles_mouse() {
    let src = include_str!("dynamic_island.rs");
    assert!(src.contains("keyDown"));
    assert!(src.contains("send_key")); // forwards to CEF
    assert!(src.contains("setIgnoresMouseEvents")); // toggled on expand/collapse
    assert!(src.contains("makeKeyWindow"));
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p vmux_desktop island_forwards_keys_and_toggles_mouse`
Expected: FAIL.

- [ ] **Step 3: Repoint `Cmd+K`** — where `command_bar/handler.rs:command_bar_open_request` handles
  `BrowserBarCommand::OpenCommandBar`, instead of toggling the `Modal`, send
  `IslandEvent::ExpandSearch` (write a `SummonCommandBar`/`IslandEvent`). Keep the command id
  (`browser_open_command_bar`, `command.rs:194`).

- [ ] **Step 4: Implement the key first-responder NSView subclass + forwarding** (objc2
  `define_class!`; on `keyDown`/`keyUp`/`flagsChanged`, translate and forward to CEF `send_key` for
  the `Island` webview — reuse the translation in `bevy_cef` keyboard path / `native_keyboard.rs`).
  On `IslandEvent::ExpandSearch`: `setIgnoresMouseEvents(false)`, `makeKeyWindow` +
  `orderFrontRegardless`, install the subclass view as first responder. On `Collapse`:
  `setIgnoresMouseEvents(true)`, resign key.

```rust
// define_class! IslandKeyView: NSView { acceptsFirstResponder = YES;
//   keyDown:/keyUp:/flagsChanged: -> push to a queue drained by a Bevy system that calls
//   browsers.send_key(island_entity, ...) }
```

> The CEF `send_key` entry point is the same one the OSR command bar uses
> (`bevy_cef` `keyboard.rs` `send_key_event` → `browsers.send_key`). Because the island is OSR
> (manual key forwarding), this matches the existing model — the difference is the *source* of the
> key events is the panel's first-responder view, not the primary window.

- [ ] **Step 5: Run guard test + build + targeted layout test**

Run: `cargo test -p vmux_desktop island_forwards_keys_and_toggles_mouse`
Expected: PASS.
Run: `cargo build -p vmux_desktop`
Expected: compiles.

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_layout/src/command_bar/handler.rs crates/vmux_desktop/src/dynamic_island.rs
git commit -m "feat(island): Cmd+K expands island; native key forwarding + mouse toggle"
```

## Task 13: Dismiss on blur/Esc + accept-activates-vmux

**Files:**
- Modify: `crates/vmux_desktop/src/dynamic_island.rs` (`resignKey` observer → `IslandPanelDismissed`),
  `crates/vmux_layout/src/island/handler.rs` (dismiss → `Collapse`; accept → activate main window)
- Test: `crates/vmux_layout/src/island/handler.rs` (`#[cfg(test)]`)

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn dismissed_message_collapses_island() {
    use bevy::prelude::*;
    let mut app = App::new();
    app.add_plugins(crate::island::IslandPlugin);
    app.world_mut().resource_mut::<Messages<crate::island::event::IslandPanelDismissed>>()
        .send(crate::island::event::IslandPanelDismissed);
    app.update();
    // collapse should have been emitted
    let ev = app.world().resource::<Messages<crate::island::event::IslandEvent>>();
    assert!(ev.iter_current_update_events().any(|e| matches!(e, IslandEvent::Collapse)));
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p vmux_layout dismissed_message_collapses_island`
Expected: FAIL.

- [ ] **Step 3: Implement** — `resignKey` NSWindow notification observer (native) sends
  `IslandPanelDismissed`; a layout system maps `IslandPanelDismissed` → `IslandEvent::Collapse`.
  Esc is already emitted by the command-bar page (close action) — ensure it routes to `Collapse`.
  When an accepted action targets the layout, call
  `crate::background_lifecycle::ensure_native_window_active(main_window_entity)` (the existing
  activation path) before/with running the action.

```rust
pub fn dismissed_to_collapse(
    mut dismissed: MessageReader<IslandPanelDismissed>,
    mut out: MessageWriter<IslandEvent>,
) {
    if dismissed.read().next().is_some() { out.write(IslandEvent::Collapse); }
}
```

Register `dismissed_to_collapse` in `IslandPlugin`. Add the `resignKey` observer in
`install_island_panel` (block-based `NSNotificationCenter` observer like
`background_lifecycle.rs:323-336` live-resize observers).

- [ ] **Step 4: Run test + build**

Run: `cargo test -p vmux_layout dismissed_to_collapse` / `dismissed_message_collapses_island`
Expected: PASS.
Run: `cargo build -p vmux_desktop`
Expected: compiles.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_layout/src/island/handler.rs crates/vmux_desktop/src/dynamic_island.rs
git commit -m "feat(island): dismiss on blur/Esc; accept activates vmux"
```

## Task 14: Remove the in-window modal

**Files:**
- Modify: `crates/vmux_layout/src/window.rs` (remove `Modal` spawn, ~390-420),
  `crates/vmux_desktop/src/glass.rs` (remove `sync_command_bar_overlay` + `CommandBarOverlay` +
  registration), and any `Modal`-only systems in `command_bar/handler.rs` that are now dead.
- Test: existing suites + the `command_bar_modal_*` tests in `window.rs` are removed/replaced.

- [ ] **Step 1: Remove the modal spawn** in `window.rs:setup` and delete/replace the now-invalid
  `command_bar_modal_backend_is_mode_driven`, `layout_uses_transparent_osr_native_overlay` (modal
  parts), and `command_bar_modal_allows_windowed_native_focus` tests (window.rs:1088-1163). Keep the
  layout-shell assertions.

- [ ] **Step 2: Remove `sync_command_bar_overlay` + `CommandBarOverlay`** from `glass.rs`
  (struct at :341-347, fn at :469-548, registration at :37) and the `init_non_send::<CommandBarOverlay>()`.

- [ ] **Step 3: Remove now-dead `Modal` open/reveal systems** in `command_bar/handler.rs` that only
  served the in-window modal (the `is_command_bar_open(Modal)` consumers, `prewarm_command_bar_modal`,
  `handle_open_command_bar`'s `Modal` display toggling). The data gatherer (Task 9) stays; the island
  path uses it. Update `glass.rs::sync_window_glass_visibility` which reads `Modal` for
  `command_bar_open` — switch it to the island's open state (a small `IslandOpen` resource/flag set
  by the expand/collapse systems) or drop that branch if no longer needed.

- [ ] **Step 4: Build + full layout/desktop tests**

Run: `cargo test -p vmux_layout`
Expected: PASS (modal tests gone, island tests green).
Run: `cargo build -p vmux_desktop`
Expected: compiles.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_layout/src/window.rs crates/vmux_desktop/src/glass.rs crates/vmux_layout/src/command_bar/handler.rs
git commit -m "refactor(island): remove in-window command-bar modal (unified on island)"
```

## Task 15: P2 runtime verification + checks

- [ ] **Step 1: fmt + clippy + tests** (CI parity; restore patched crates after fmt per project rule)

Run: `cargo fmt --all` then `git checkout -- patches/` (fmt rewrites vendored patches).
Run: `cargo clippy -p vmux_layout -p vmux_desktop -p vmux_command -p vmux_setting --all-targets`
Expected: no warnings (fix any).
Run: `cargo test -p vmux_layout -p vmux_command -p vmux_setting -p vmux_desktop`
Expected: PASS.

- [ ] **Step 2: Build + run the app**

Run: `cargo run -p vmux_desktop`

- [ ] **Step 3: Runtime acceptance (P2)** — with the app running:
  - `Cmd+Shift+Space` from a *different* frontmost app expands the island into search; typing works
    (keys reach CEF while vmux is unfocused); results filter; Enter accepts and opens in the layout,
    activating the main window.
  - `Cmd+K` inside vmux expands the island (the old in-window modal is gone).
  - Clicking another window (blur) collapses the island; `Esc` collapses it.
  - Idle pill remains static; idle CPU unchanged.

- [ ] **Step 4: Commit any fixes** from clippy/runtime.

```bash
git add -A && git commit -m "chore(island): fmt/clippy + P2 runtime fixes"
```

---

## Self-review checklist (done while writing)

- **Spec coverage:** panel/glass (T6), OSR composite (T6), morph state machine (T2/T3), page (T4),
  island webview (T5), animated resize (T7), global hotkey + waker (T11), Cmd+K unify (T12), native
  key forwarding (T12), dismiss-on-blur/Esc + activate (T13), modal removal (T14), idle budget guard
  (T6), settings (T11), build/@source tracking (T4). P3 feeds intentionally deferred.
- **Type consistency:** `IslandRenderEvent`/`IslandState`/`IslandActivity(Kind)`/`IslandNotice`
  (vmux_command) used identically in state.rs, handler.rs, page.rs; `Island` marker, `IslandEvent`,
  `IslandPanel*` messages consistent across layout + desktop; `build_command_bar_open_payload` +
  `CommandBarPayloadSources` defined in T9, used in T10.
- **Confirm-before-coding pointers** (paths to verify at execution time, flagged inline): exact
  `BinHostEmitEvent` import path, `CommandBarNativeSize` component path, the page render-event hook,
  `EventLoopProxyWrapper`/`WinitUserEvent` path, `global-hotkey` version, and the `NSGlassEffectView`
  /`setFrame_display_animate` objc2 signatures.

## Risks (carried from spec)

- Always-on idle cost (static idle pill; guard test); CEF painting while vmux hidden (power-mode
  wake on expand/activity); first-responder key forwarding while unfocused (validate in T12 runtime);
  payload-gatherer extraction regressions (T9 baseline); vendored patch fmt churn (T15).
