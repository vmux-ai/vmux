# Loading Indicator Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show a loading indicator on the active pane when its browser is loading, with a thin indeterminate progress bar and faster focus ring animation.

**Architecture:** Drain the existing `WebviewLoadingStateReceiver` channel to insert/remove a `Loading` marker component on Browser entities. A UI node bar animates at the top of the active tab. The focus ring gradient speed multiplies by 3x when loading.

**Tech Stack:** Bevy 0.18, bevy_cef (existing CEF integration), WGSL shader (existing focus ring)

---

### Task 1: Add `Loading` component and drain system

**Files:**
- Modify: `crates/vmux_desktop/src/browser.rs`

- [ ] **Step 1: Add the `Loading` marker component**

After the existing `pub(crate) struct Browser;` line (~line 84), add:

```rust
#[derive(Component)]
pub(crate) struct Loading;
```

- [ ] **Step 2: Add the `drain_loading_state` system**

Add this function in `browser.rs`:

```rust
fn drain_loading_state(
    receiver: Res<WebviewLoadingStateReceiver>,
    mut commands: Commands,
) {
    while let Ok(ev) = receiver.0.try_recv() {
        if ev.is_loading {
            commands.entity(ev.webview).insert(Loading);
        } else {
            commands.entity(ev.webview).remove::<Loading>();
        }
    }
}
```

Add the import for `WebviewLoadingStateReceiver` at the top of the file (it's re-exported from `bevy_cef::prelude`).

- [ ] **Step 3: Register the system in `BrowserPlugin::build`**

Add after the existing `handle_browser_commands` registration:

```rust
.add_systems(Update, drain_loading_state)
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo check -p vmux_desktop --lib`

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_desktop/src/browser.rs
git commit -m "feat: drain CEF loading state into Loading component"
```

---

### Task 2: Speed up focus ring gradient when loading

**Files:**
- Modify: `crates/vmux_desktop/src/layout/focus_ring.rs`

- [ ] **Step 1: Add imports for Loading and Browser query support**

Update the `use crate::` block at the top of `focus_ring.rs`:

```rust
use crate::{
    browser::{Browser, Loading},
    layout::{
        window::{VmuxWindow, WEBVIEW_Z_FOCUS_RING},
        pane::Pane,
        tab::{Active, Tab},
    },
    settings::{AppSettings, load_settings},
};
```

- [ ] **Step 2: Add `is_loading` parameter to `build_focus_ring_material`**

Change the function signature:

```rust
fn build_focus_ring_material(
    w_i: f32,
    h_i: f32,
    settings: &AppSettings,
    time_secs: f32,
    is_loading: bool,
) -> FocusRingMaterial {
```

Inside the function, change the `gradient_params` line from:

```rust
    let gradient_params = Vec4::new(grad_on, g.speed, g.cycles.max(0.01), time_secs);
```

to:

```rust
    let speed = if is_loading { g.speed * 3.0 } else { g.speed };
    let gradient_params = Vec4::new(grad_on, speed, g.cycles.max(0.01), time_secs);
```

- [ ] **Step 3: Add loading query to `sync_focus_ring_to_active_pane`**

Add these parameters to the system:

```rust
    pane_children: Query<&Children, With<Pane>>,
    tab_children: Query<&Children, With<Tab>>,
    active_tabs: Query<Entity, (With<Active>, With<Tab>)>,
    loading_q: Query<(), With<Loading>>,
```

After the `active_pane` query succeeds (after `let Ok((pane_computed, pane_ui_gt)) = active_pane.single()`), determine if the active browser is loading:

```rust
    let is_loading = pane_children
        .get(active_pane.single().unwrap())
        .ok()
        .and_then(|children| children.iter().find(|&e| active_tabs.contains(e)))
        .and_then(|tab| tab_children.get(tab).ok())
        .map(|children| children.iter().any(|e| loading_q.contains(e)))
        .unwrap_or(false);
```

Note: the `active_pane` entity is already obtained via `active_pane.single()` above. You'll need to extract the entity before the `let Ok((pane_computed, pane_ui_gt))` destructure. Restructure to:

```rust
    let Ok(active_entity) = active_pane.single().map(|(_, _, e)| e) else {
```

Actually, the existing query is `Query<(&ComputedNode, &UiGlobalTransform), (With<Active>, With<Pane>)>`. Add `Entity` to it:

Change:
```rust
    active_pane: Query<(&ComputedNode, &UiGlobalTransform), (With<Active>, With<Pane>)>,
```
to:
```rust
    active_pane: Query<(Entity, &ComputedNode, &UiGlobalTransform), (With<Active>, With<Pane>)>,
```

And update the destructure:
```rust
    let Ok((active_entity, pane_computed, pane_ui_gt)) = active_pane.single() else {
```

Then compute `is_loading`:

```rust
    let is_loading = pane_children
        .get(active_entity)
        .ok()
        .and_then(|children| children.iter().find(|&e| active_tabs.contains(e)))
        .and_then(|tab| tab_children.get(tab).ok())
        .map(|children| children.iter().any(|e| loading_q.contains(e)))
        .unwrap_or(false);
```

- [ ] **Step 4: Pass `is_loading` to `build_focus_ring_material`**

Update the call at the end of `sync_focus_ring_to_active_pane`:

```rust
    if let Some(m) = ring_materials.get_mut(&mat_h.0) {
        *m = build_focus_ring_material(w_i, h_i, &settings, time.elapsed_secs(), is_loading);
    }
```

Also update the call in `spawn_focus_ring`:

```rust
    let mat = build_focus_ring_material(800.0, 600.0, &settings, time.elapsed_secs(), false);
```

- [ ] **Step 5: Verify it compiles**

Run: `cargo check -p vmux_desktop --lib`

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_desktop/src/layout/focus_ring.rs
git commit -m "feat: speed up focus ring gradient 3x when active pane is loading"
```

---

### Task 3: Add indeterminate loading bar

**Files:**
- Modify: `crates/vmux_desktop/src/browser.rs`

- [ ] **Step 1: Add `LoadingBarRoot` marker component**

Near the `Loading` component:

```rust
#[derive(Component)]
struct LoadingBarRoot;

#[derive(Component)]
struct LoadingBarFill;
```

- [ ] **Step 2: Spawn loading bar as child of each Browser when it's created**

In `Browser::new()`, the bundle already includes `Node` with absolute positioning. Add a loading bar as a separate spawn. Since `Browser::new()` returns a bundle and doesn't have access to `Commands`, the loading bar should be spawned alongside the Browser in the call sites.

Instead, add a system `spawn_loading_bar_for_new_browsers` that observes new `Browser` entities and spawns the bar as a sibling inside the same Tab:

```rust
fn spawn_loading_bar_for_browsers(
    browsers: Query<(Entity, &ChildOf), (With<Browser>, Without<LoadingBarRoot>)>,
    existing_bars: Query<&ChildOf, With<LoadingBarRoot>>,
    mut commands: Commands,
    settings: Res<AppSettings>,
) {
    for (browser_entity, browser_co) in &browsers {
        let tab = browser_co.get();
        // Check if this tab already has a loading bar
        if existing_bars.iter().any(|co| co.get() == tab) {
            continue;
        }
        let accent = &settings.layout.pane.outline.gradient.accent;
        let bar_color = Color::srgb(accent.r, accent.g, accent.b);
        // Root: full-width container at top of tab, 2px tall
        let root = commands
            .spawn((
                LoadingBarRoot,
                Node {
                    position_type: PositionType::Absolute,
                    top: Val::Px(0.0),
                    left: Val::Px(0.0),
                    width: Val::Percent(100.0),
                    height: Val::Px(2.0),
                    overflow: Overflow::clip(),
                    ..default()
                },
                ZIndex(2),
                Visibility::Hidden,
                BackgroundColor(Color::NONE),
                ChildOf(tab),
            ))
            .id();
        // Fill: the animated sliding bar (30% width, moves left-to-right)
        commands.spawn((
            LoadingBarFill,
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(0.0),
                left: Val::Percent(0.0),
                width: Val::Percent(30.0),
                height: Val::Percent(100.0),
                ..default()
            },
            BackgroundColor(bar_color),
            ChildOf(root),
        ));
    }
}
```

- [ ] **Step 3: Add `sync_loading_bar` system**

```rust
fn sync_loading_bar(
    active_pane: Query<Entity, (With<Active>, With<Pane>)>,
    pane_children: Query<&Children, With<Pane>>,
    active_tabs: Query<Entity, (With<Active>, With<Tab>)>,
    tab_children: Query<&Children, With<Tab>>,
    loading_q: Query<(), With<Loading>>,
    mut bar_q: Query<(&ChildOf, &mut Visibility), With<LoadingBarRoot>>,
    mut fill_q: Query<(&ChildOf, &mut Node), With<LoadingBarFill>>,
    time: Res<Time>,
) {
    // Find the active tab entity
    let active_tab = active_pane
        .single()
        .ok()
        .and_then(|pane| pane_children.get(pane).ok())
        .and_then(|children| children.iter().find(|&e| active_tabs.contains(e)));

    // Check if the active tab's browser is loading
    let (is_loading, active_tab_entity) = if let Some(tab) = active_tab {
        let loading = tab_children
            .get(tab)
            .ok()
            .map(|children| children.iter().any(|e| loading_q.contains(e)))
            .unwrap_or(false);
        (loading, Some(tab))
    } else {
        (false, None)
    };

    // Show/hide loading bars
    for (co, mut vis) in &mut bar_q {
        if is_loading && active_tab_entity == Some(co.get()) {
            *vis = Visibility::Visible;
        } else {
            *vis = Visibility::Hidden;
        }
    }

    // Animate fill position (indeterminate sweep)
    if is_loading {
        let t = time.elapsed_secs();
        // Sweep from -30% to 100% over ~2 seconds, loop
        let progress = ((t * 0.5) % 1.3) / 1.3; // 0..1
        let left = -30.0 + progress * 130.0; // -30% to 100%
        for (co, mut node) in &mut fill_q {
            // Only animate fills whose parent bar is visible
            if active_tab_entity.map(|tab| {
                bar_q.iter().any(|(bar_co, vis)| bar_co.get() == tab && *vis == Visibility::Visible)
            }).unwrap_or(false) {
                node.left = Val::Percent(left);
            }
        }
    }
}
```

- [ ] **Step 4: Register systems in `BrowserPlugin::build`**

```rust
.add_systems(Update, (drain_loading_state, spawn_loading_bar_for_browsers))
.add_systems(PostUpdate, sync_loading_bar.after(UiSystems::Layout))
```

Remove the previous standalone `drain_loading_state` registration if already added.

- [ ] **Step 5: Add necessary imports**

Add to the imports at the top of `browser.rs`:

```rust
use crate::layout::tab::Tab;
use crate::layout::pane::Pane;
```

Verify `BackgroundColor`, `ZIndex`, `Overflow` are available from `bevy::prelude::*` or add explicit imports.

- [ ] **Step 6: Verify it compiles**

Run: `cargo check -p vmux_desktop --lib`

- [ ] **Step 7: Commit**

```bash
git add crates/vmux_desktop/src/browser.rs
git commit -m "feat: add indeterminate loading bar at top of active pane"
```
