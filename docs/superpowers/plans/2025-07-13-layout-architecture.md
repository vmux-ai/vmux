# Layout Architecture Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Restructure the entity hierarchy from flat DisplayGlass-based layout to Window > Main > Space > Pane > Tab, with shared chrome at Window level and data scoped via Active chain.

**Architecture:** Rename DisplayGlass to Window, insert a Main flex container and a Space entity between Window and the pane tree root. Update focused_tab to walk 3 levels (Active Space > Active Pane > Active Tab). Add stub entities for BottomBar, Modal, and additional SideSheet positions. Add SpacePlugin for space switching.

**Tech Stack:** Bevy 0.18, bevy_cef, Dioxus webview apps

**Verify command:** `cargo check -p vmux_desktop out+err>| tail -20`

**Code style:** No comments in code.

---

## File Structure

| File | Action | Responsibility |
|------|--------|----------------|
| `crates/vmux_desktop/src/layout/window.rs` | Create (rename from display.rs) | Window component, setup hierarchy, fit_window_to_screen |
| `crates/vmux_desktop/src/layout/space.rs` | Create | Space component, SpacePlugin, handle_space_commands, sync_space_visibility |
| `crates/vmux_desktop/src/layout/display.rs` | Delete | Replaced by window.rs |
| `crates/vmux_desktop/src/layout.rs` | Modify | Module declarations, re-exports |
| `crates/vmux_desktop/src/layout/tab.rs` | Modify | focused_tab 3-level walk, Space import |
| `crates/vmux_desktop/src/layout/pane.rs` | Modify | on_pane_cycle filters to Active Space |
| `crates/vmux_desktop/src/layout/side_sheet.rs` | Modify | SideSheetPosition, query Window instead of DisplayGlass, margin on Main |
| `crates/vmux_desktop/src/layout/focus_ring.rs` | Modify | Query Window instead of DisplayGlass |
| `crates/vmux_desktop/src/browser.rs` | Modify | Import Window, push_pane_tree_emit filters to Active Space |
| `crates/vmux_desktop/src/command.rs` | Modify | Add SpaceCommand variants |
| `crates/vmux_desktop/src/lib.rs` | Modify | Re-export fit_window_to_screen |

---

### Task 1: Rename DisplayGlass to Window (component + module)

**Files:**
- Create: `crates/vmux_desktop/src/layout/window.rs` (copy from display.rs)
- Delete: `crates/vmux_desktop/src/layout/display.rs`
- Modify: `crates/vmux_desktop/src/layout.rs`
- Modify: `crates/vmux_desktop/src/browser.rs`
- Modify: `crates/vmux_desktop/src/layout/side_sheet.rs`
- Modify: `crates/vmux_desktop/src/layout/focus_ring.rs`
- Modify: `crates/vmux_desktop/src/lib.rs`

This task is purely mechanical renaming. No hierarchy changes.

- [ ] **Step 1: Create window.rs from display.rs with renames**

Copy `crates/vmux_desktop/src/layout/display.rs` to `crates/vmux_desktop/src/layout/window.rs`. Apply these renames throughout:
- `DisplayGlass` -> `Window` (the component, NOT `bevy::prelude::Window`)
- `DisplayGlassBundle` -> `WindowBundle`
- `DisplayPlugin` -> `WindowPlugin`
- `fit_display_glass_to_window` -> `fit_window_to_screen`

Because `bevy::prelude::Window` conflicts, import it as `use bevy::window::Window as BevyWindow;` and use `BevyWindow` where the Bevy window type is needed.

The full file content:

```rust
use crate::{
    browser::{browser_bundle, Browser},
    layout::pane::{Pane, PaneSplit, leaf_pane_bundle},
    layout::rounded::{RoundedCorners, RoundedMaterial},
    layout::side_sheet::SideSheet,
    layout::tab::{Active, tab_bundle},
    scene::MainCamera,
    settings::{AppSettings, load_settings},
    unit::{PIXELS_PER_METER, WindowExt},
};
use vmux_webview_app::WebviewAppEmbedSet;
use bevy::{
    prelude::*,
    render::alpha::AlphaMode,
    ui::{FlexDirection, UiTargetCamera},
    window::PrimaryWindow,
};
use bevy_cef::prelude::*;
use vmux_header::{HEADER_HEIGHT_PX, HEADER_WEBVIEW_URL, Header, HeaderBundle};

pub(crate) const WEBVIEW_Z_MAIN: f32 = 0.12;
pub(crate) const WEBVIEW_Z_FOCUS_RING: f32 = 0.13;
pub(crate) const WEBVIEW_Z_HEADER: f32 = 0.125;
pub(crate) const WEBVIEW_Z_SIDE_SHEET: f32 = 0.125;
pub(crate) const WEBVIEW_MESH_DEPTH_BIAS: f32 = -4.0;

pub(crate) struct WindowPlugin;

impl Plugin for WindowPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Startup,
            (setup, fit_window_to_screen)
                .chain()
                .after(load_settings)
                .after(crate::scene::setup)
                .after(WebviewAppEmbedSet),
        )
        .add_systems(PostUpdate, fit_window_to_screen);
    }
}

#[derive(Bundle)]
struct WindowBundle<M>
where
    M: Material,
{
    marker: VmuxWindow,
    mesh: Mesh3d,
    material: MeshMaterial3d<M>,
    transform: Transform,
    node: Node,
    ui_target: UiTargetCamera,
}

#[derive(Component)]
pub(crate) struct VmuxWindow;

fn setup(
    window: Single<&bevy::window::Window, With<PrimaryWindow>>,
    primary_window: Single<Entity, With<PrimaryWindow>>,
    main_camera: Single<Entity, With<MainCamera>>,
    mut commands: Commands,
    settings: Res<AppSettings>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<RoundedMaterial>>,
    mut webview_mt: ResMut<Assets<WebviewExtendStandardMaterial>>,
) {
    let m = window.meters();
    let pw = *primary_window;
    let startup_url = settings.browser.startup_url.as_str();

    commands.spawn((
        WindowBundle {
            marker: VmuxWindow,
            mesh: Mesh3d(meshes.add(Plane3d::new(Vec3::Z, Vec2::splat(0.5)))),
            material: MeshMaterial3d(materials.add(RoundedMaterial {
                base: StandardMaterial {
                    base_color: Color::srgba(0.08, 0.08, 0.08, 0.4),
                    alpha_mode: AlphaMode::Blend,
                    cull_mode: None,
                    perceptual_roughness: 0.12,
                    metallic: 0.0,
                    specular_transmission: 0.9,
                    diffuse_transmission: 1.0,
                    thickness: 0.1,
                    ior: 1.5,
                    ..default()
                },
                extension: RoundedCorners {
                    clip: Vec4::new(settings.layout.pane.radius, m.x, m.y, PIXELS_PER_METER),
                    ..default()
                },
            })),
            transform: Transform {
                translation: Vec3::new(0.0, m.y * 0.5, 0.0),
                scale: Vec3::new(m.x, m.y, 1.0),
                ..default()
            },
            node: Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                position_type: PositionType::Relative,
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(Val::Px(settings.layout.window.padding)),
                row_gap: Val::Px(settings.layout.pane.gap),
                ..default()
            },
            ui_target: UiTargetCamera(*main_camera),
        },
        children![
            (
                SideSheet,
                HostWindow(pw),
                Browser,
                Node {
                    width: Val::Px(settings.layout.side_sheet.width),
                    flex_shrink: 0.0,
                    display: Display::None,
                    position_type: PositionType::Absolute,
                    left: Val::Px(settings.layout.window.padding),
                    top: Val::Px(settings.layout.window.padding),
                    bottom: Val::Px(settings.layout.window.padding),
                    ..default()
                },
                ZIndex(2),
                WebviewSource::new("vmux://side-sheet/"),
                Mesh3d(meshes.add(Plane3d::new(Vec3::Z, Vec2::splat(0.5)))),
                MeshMaterial3d(webview_mt.add(WebviewExtendStandardMaterial {
                    base: StandardMaterial {
                        unlit: true,
                        alpha_mode: AlphaMode::Blend,
                        depth_bias: WEBVIEW_MESH_DEPTH_BIAS,
                        ..default()
                    },
                    ..default()
                })),
                WebviewSize(Vec2::new(settings.layout.side_sheet.width, 720.0)),
                Transform::default(),
                GlobalTransform::default(),
                Visibility::Hidden,
            ),
            (
                ZIndex(1),
                HostWindow(pw),
                Browser,
                Node {
                    height: Val::Px(HEADER_HEIGHT_PX),
                    flex_shrink: 0.0,
                    ..default()
                },
                HeaderBundle {
                    marker: Header,
                    source: WebviewSource::new(HEADER_WEBVIEW_URL),
                    mesh: Mesh3d(meshes.add(Plane3d::new(Vec3::Z, Vec2::splat(0.5)))),
                    material: MeshMaterial3d(webview_mt.add(
                        WebviewExtendStandardMaterial {
                            base: StandardMaterial {
                                unlit: true,
                                alpha_mode: AlphaMode::Blend,
                                depth_bias: WEBVIEW_MESH_DEPTH_BIAS,
                                ..default()
                            },
                            ..default()
                        },
                    )),
                    webview_size: WebviewSize(Vec2::new(1280.0, HEADER_HEIGHT_PX)),
                },
            ),
            (
                Pane,
                PaneSplit,
                HostWindow(pw),
                ZIndex(0),
                Transform::default(),
                GlobalTransform::default(),
                Node {
                    flex_grow: 1.0,
                    min_height: Val::Px(0.0),
                    column_gap: Val::Px(settings.layout.pane.gap),
                    row_gap: Val::Px(settings.layout.pane.gap),
                    ..default()
                },
                children![(
                    leaf_pane_bundle(),
                    Active,
                    children![(
                        tab_bundle(),
                        Active,
                        children![browser_bundle(
                            &mut meshes,
                            &mut webview_mt,
                            startup_url
                        )],
                    )],
                )],
            ),
        ],
    ));
}

pub(crate) fn fit_window_to_screen(
    window: Single<&bevy::window::Window, With<PrimaryWindow>>,
    settings: Res<AppSettings>,
    mut materials: ResMut<Assets<RoundedMaterial>>,
    mut last_size: Local<Vec2>,
    mut q: Query<(&mut Transform, &MeshMaterial3d<RoundedMaterial>), With<VmuxWindow>>,
) {
    let m = window.meters();
    if (m.x - last_size.x).abs() < 0.001 && (m.y - last_size.y).abs() < 0.001 {
        return;
    }
    *last_size = m;

    let r = settings.layout.pane.radius;

    for (mut tf, handle) in &mut q {
        tf.translation = Vec3::new(0.0, m.y * 0.5, 0.0);
        tf.scale = Vec3::new(m.x, m.y, 1.0);

        if let Some(mat) = materials.get_mut(handle) {
            mat.extension.clip = Vec4::new(r, m.x, m.y, PIXELS_PER_METER);
        }
    }
}
```

Note: We use `VmuxWindow` as the component name to avoid conflict with `bevy::window::Window`. The Bevy window type is referenced as `bevy::window::Window` in function signatures.

- [ ] **Step 2: Delete display.rs**

```bash
rm crates/vmux_desktop/src/layout/display.rs
```

- [ ] **Step 3: Update layout.rs module declarations**

Replace the full file content of `crates/vmux_desktop/src/layout.rs`:

```rust
pub(crate) mod tab;
mod focus_ring;
pub(crate) mod rounded;

pub(crate) mod window;
pub(crate) mod pane;
pub(crate) mod side_sheet;

use bevy::prelude::*;
use focus_ring::FocusRingPlugin;
use pane::PanePlugin;
use rounded::RoundedMaterialPlugin;
use side_sheet::SideSheetPlugin;
use tab::TabPlugin;
use vmux_webview_app::JsEmitUiReadyPlugin;
use window::WindowPlugin;

pub(crate) use window::fit_window_to_screen;

pub struct LayoutPlugin;

impl Plugin for LayoutPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            JsEmitUiReadyPlugin,
            WindowPlugin,
            PanePlugin,
            TabPlugin,
            FocusRingPlugin,
            RoundedMaterialPlugin,
            SideSheetPlugin,
        ));
    }
}
```

- [ ] **Step 4: Update browser.rs imports**

In `crates/vmux_desktop/src/browser.rs`, change:

```rust
// OLD
use crate::{
    command::{AppCommand, BrowserCommand, ReadAppCommands},
    layout::{
        display::{
            DisplayGlass, WEBVIEW_MESH_DEPTH_BIAS, WEBVIEW_Z_HEADER, WEBVIEW_Z_MAIN,
            WEBVIEW_Z_SIDE_SHEET,
        },
        ...
    },
    ...
};
```

to:

```rust
// NEW
use crate::{
    command::{AppCommand, BrowserCommand, ReadAppCommands},
    layout::{
        window::{
            VmuxWindow, WEBVIEW_MESH_DEPTH_BIAS, WEBVIEW_Z_HEADER, WEBVIEW_Z_MAIN,
            WEBVIEW_Z_SIDE_SHEET,
        },
        ...
    },
    ...
};
```

Then in `sync_children_to_ui`, change:

```rust
// OLD
    glass: Single<(Entity, &ComputedNode, &UiGlobalTransform), With<DisplayGlass>>,
// NEW
    glass: Single<(Entity, &ComputedNode, &UiGlobalTransform), With<VmuxWindow>>,
```

- [ ] **Step 5: Update side_sheet.rs imports**

In `crates/vmux_desktop/src/layout/side_sheet.rs`, change:

```rust
// OLD
use crate::{
    command::{AppCommand, ReadAppCommands, SideSheetCommand},
    layout::display::DisplayGlass,
    ...
};
// NEW
use crate::{
    command::{AppCommand, ReadAppCommands, SideSheetCommand},
    layout::window::VmuxWindow,
    ...
};
```

In `sync_side_sheet_visibility`, change:

```rust
// OLD
    glass_q: Query<Entity, With<DisplayGlass>>,
// NEW
    glass_q: Query<Entity, With<VmuxWindow>>,
```

- [ ] **Step 6: Update focus_ring.rs imports**

In `crates/vmux_desktop/src/layout/focus_ring.rs`, change:

```rust
// OLD
use crate::{
    layout::{
        display::{DisplayGlass, WEBVIEW_Z_FOCUS_RING},
        ...
    },
    ...
};
// NEW
use crate::{
    layout::{
        window::{VmuxWindow, WEBVIEW_Z_FOCUS_RING},
        ...
    },
    ...
};
```

Update the query in `sync_focus_ring_to_active_pane`:

```rust
// OLD
    ... With<DisplayGlass> ...
// NEW
    ... With<VmuxWindow> ...
```

- [ ] **Step 7: Update lib.rs re-export**

In `crates/vmux_desktop/src/lib.rs`, change:

```rust
// OLD
use layout::fit_display_glass_to_window;
// (and any call sites)
// NEW
use layout::fit_window_to_screen;
// (update call sites)
```

Search for `fit_display_glass_to_window` in lib.rs and replace with `fit_window_to_screen`.

- [ ] **Step 8: Verify compilation**

Run: `cargo check -p vmux_desktop out+err>| tail -20`
Expected: No errors.

- [ ] **Step 9: Commit**

```bash
git add -A
git commit -m "refactor: rename DisplayGlass to VmuxWindow, display.rs to window.rs

Generated by Mistral Vibe.
Co-Authored-By: Mistral Vibe <vibe@mistral.ai>"
```

---

### Task 2: Add Main container entity

**Files:**
- Modify: `crates/vmux_desktop/src/layout/window.rs`
- Modify: `crates/vmux_desktop/src/layout/side_sheet.rs`

Insert a `Main` container entity between Window and the pane tree root. Main takes the flex_grow space. The pane root PaneSplit becomes a child of Main instead of Window.

- [ ] **Step 1: Add Main component and restructure setup() in window.rs**

Add the `Main` component definition after `VmuxWindow`:

```rust
#[derive(Component)]
pub(crate) struct Main;
```

In the `setup()` function, restructure `children![]` to wrap the pane tree root inside a Main entity. The SideSheet and Header remain direct children of Window. Main sits between Header and the pane root:

Change the `children![]` in the `commands.spawn(...)` call. The current structure is:

```
children![
    (SideSheet, ...),
    (Header, ...),
    (Pane, PaneSplit, ...),
]
```

Change to:

```
children![
    (SideSheet, ...),
    (Header, ...),
    (
        Main,
        Node {
            flex_grow: 1.0,
            min_height: Val::Px(0.0),
            ..default()
        },
        children![(
            Pane,
            PaneSplit,
            HostWindow(pw),
            ZIndex(0),
            Transform::default(),
            GlobalTransform::default(),
            Node {
                flex_grow: 1.0,
                min_height: Val::Px(0.0),
                column_gap: Val::Px(settings.layout.pane.gap),
                row_gap: Val::Px(settings.layout.pane.gap),
                ..default()
            },
            children![(
                leaf_pane_bundle(),
                Active,
                children![(
                    tab_bundle(),
                    Active,
                    children![browser_bundle(
                        &mut meshes,
                        &mut webview_mt,
                        startup_url
                    )],
                )],
            )],
        )],
    ),
]
```

The key change: the PaneSplit root was previously a direct child of Window with `ZIndex(0)`. Now it's a child of Main. The Main entity has `flex_grow: 1.0` and `min_height: 0` to fill remaining space.

- [ ] **Step 2: Update side_sheet.rs to adjust margin on Main instead of PaneSplit root**

In `sync_side_sheet_visibility`, the current code adjusts `margin.left` on the root PaneSplit that's a direct child of DisplayGlass (now VmuxWindow). With Main in between, we need to adjust margin on Main instead.

Replace the `pane_q` query and its usage:

```rust
// OLD
fn sync_side_sheet_visibility(
    open: Res<SideSheetOpen>,
    settings: Res<AppSettings>,
    mut side_sheet_q: Query<(&mut Visibility, &mut Node), With<SideSheet>>,
    mut header_q: Query<&mut Node, (With<Header>, Without<SideSheet>, Without<Pane>)>,
    glass_q: Query<Entity, With<VmuxWindow>>,
    mut pane_q: Query<
        (&mut Node, &ChildOf),
        (With<Pane>, With<PaneSplit>, Without<SideSheet>, Without<Header>),
    >,
) {
    if !open.is_changed() {
        return;
    }
    let sheet_total = settings.layout.side_sheet.width + settings.layout.pane.gap;
    for (mut vis, mut node) in &mut side_sheet_q {
        if open.0 {
            *vis = Visibility::Inherited;
            node.display = Display::Flex;
        } else {
            *vis = Visibility::Hidden;
            node.display = Display::None;
        }
    }
    for mut node in &mut header_q {
        node.margin.left = if open.0 {
            Val::Px(sheet_total)
        } else {
            Val::Px(0.0)
        };
    }
    let Ok(glass) = glass_q.single() else {
        return;
    };
    for (mut node, child_of) in &mut pane_q {
        if child_of.get() != glass {
            continue;
        }
        node.margin.left = if open.0 {
            Val::Px(sheet_total)
        } else {
            Val::Px(0.0)
        };
    }
}
```

```rust
// NEW
fn sync_side_sheet_visibility(
    open: Res<SideSheetOpen>,
    settings: Res<AppSettings>,
    mut side_sheet_q: Query<(&mut Visibility, &mut Node), With<SideSheet>>,
    mut header_q: Query<&mut Node, (With<Header>, Without<SideSheet>, Without<Main>)>,
    mut main_q: Query<&mut Node, (With<Main>, Without<SideSheet>, Without<Header>)>,
) {
    if !open.is_changed() {
        return;
    }
    let sheet_total = settings.layout.side_sheet.width + settings.layout.pane.gap;
    for (mut vis, mut node) in &mut side_sheet_q {
        if open.0 {
            *vis = Visibility::Inherited;
            node.display = Display::Flex;
        } else {
            *vis = Visibility::Hidden;
            node.display = Display::None;
        }
    }
    for mut node in &mut header_q {
        node.margin.left = if open.0 {
            Val::Px(sheet_total)
        } else {
            Val::Px(0.0)
        };
    }
    for mut node in &mut main_q {
        node.margin.left = if open.0 {
            Val::Px(sheet_total)
        } else {
            Val::Px(0.0)
        };
    }
}
```

Update imports in `side_sheet.rs`: add `Main` import from `window`:

```rust
use crate::{
    command::{AppCommand, ReadAppCommands, SideSheetCommand},
    layout::window::{Main, VmuxWindow},
    layout::pane::{Pane, PaneSplit},
    settings::AppSettings,
};
```

Note: `VmuxWindow` can be removed from imports now since it's no longer used in side_sheet.rs. Also remove the `Pane` and `PaneSplit` imports if they're no longer needed (the old `pane_q` query used them). The new code only queries `Main`, so:

```rust
use crate::{
    command::{AppCommand, ReadAppCommands, SideSheetCommand},
    layout::window::Main,
    settings::AppSettings,
};
use bevy::{prelude::*};
use vmux_header::Header;
```

Remove `use bevy::ecs::relationship::Relationship;` and `use bevy::ui::UiSystems;` only if no longer needed. Check: `sync_side_sheet_visibility` is in `PostUpdate.before(UiSystems::Layout)` — keep `UiSystems`. The `Relationship` import was for `child_of.get()` — no longer needed.

Final imports for side_sheet.rs:

```rust
use crate::{
    command::{AppCommand, ReadAppCommands, SideSheetCommand},
    layout::window::Main,
    settings::AppSettings,
};
use bevy::{prelude::*, ui::UiSystems};
use vmux_header::Header;
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p vmux_desktop out+err>| tail -20`
Expected: No errors.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat: add Main container between Window and pane tree

Generated by Mistral Vibe.
Co-Authored-By: Mistral Vibe <vibe@mistral.ai>"
```

---

### Task 3: Add Space entity between Main and pane root

**Files:**
- Modify: `crates/vmux_desktop/src/layout/window.rs`
- Create: `crates/vmux_desktop/src/layout/space.rs`
- Modify: `crates/vmux_desktop/src/layout.rs`

Insert a Space entity as child of Main, parent of the pane tree root. For now, only one Space is created (Active by default).

- [ ] **Step 1: Create space.rs with Space component and SpacePlugin**

Create `crates/vmux_desktop/src/layout/space.rs`:

```rust
use crate::{
    command::{AppCommand, ReadAppCommands, SpaceCommand},
    layout::tab::Active,
};
use bevy::prelude::*;

pub(crate) struct SpacePlugin;

impl Plugin for SpacePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, handle_space_commands.in_set(ReadAppCommands))
            .add_systems(PostUpdate, sync_space_visibility);
    }
}

#[derive(Component)]
pub(crate) struct Space;

pub(crate) fn space_bundle() -> impl Bundle {
    (
        Space,
        Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            right: Val::Px(0.0),
            top: Val::Px(0.0),
            bottom: Val::Px(0.0),
            ..default()
        },
    )
}

fn handle_space_commands(
    mut reader: MessageReader<AppCommand>,
) {
    for cmd in reader.read() {
        let AppCommand::Space(space_cmd) = *cmd else {
            continue;
        };
        match space_cmd {
            SpaceCommand::New => {}
        }
    }
}

fn sync_space_visibility(
    mut spaces: Query<(Has<Active>, &mut Node), With<Space>>,
) {
    for (is_active, mut node) in &mut spaces {
        let target = if is_active { Display::Flex } else { Display::None };
        if node.display != target {
            node.display = target;
        }
    }
}
```

- [ ] **Step 2: Register SpacePlugin in layout.rs**

In `crates/vmux_desktop/src/layout.rs`, add the module declaration and plugin:

```rust
pub(crate) mod tab;
mod focus_ring;
pub(crate) mod rounded;

pub(crate) mod window;
pub(crate) mod pane;
pub(crate) mod side_sheet;
pub(crate) mod space;

use bevy::prelude::*;
use focus_ring::FocusRingPlugin;
use pane::PanePlugin;
use rounded::RoundedMaterialPlugin;
use side_sheet::SideSheetPlugin;
use space::SpacePlugin;
use tab::TabPlugin;
use vmux_webview_app::JsEmitUiReadyPlugin;
use window::WindowPlugin;

pub(crate) use window::fit_window_to_screen;

pub struct LayoutPlugin;

impl Plugin for LayoutPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            JsEmitUiReadyPlugin,
            WindowPlugin,
            SpacePlugin,
            PanePlugin,
            TabPlugin,
            FocusRingPlugin,
            RoundedMaterialPlugin,
            SideSheetPlugin,
        ));
    }
}
```

- [ ] **Step 3: Restructure setup() in window.rs to include Space**

In `crates/vmux_desktop/src/layout/window.rs`, add imports:

```rust
use crate::{
    browser::{browser_bundle, Browser},
    layout::pane::{Pane, PaneSplit, leaf_pane_bundle},
    layout::rounded::{RoundedCorners, RoundedMaterial},
    layout::side_sheet::SideSheet,
    layout::space::space_bundle,
    layout::tab::{Active, tab_bundle},
    scene::MainCamera,
    settings::{AppSettings, load_settings},
    unit::{PIXELS_PER_METER, WindowExt},
};
```

Change the Main's `children![]` in setup(). Currently (from Task 2):

```rust
(
    Main,
    Node {
        flex_grow: 1.0,
        min_height: Val::Px(0.0),
        ..default()
    },
    children![(
        Pane,
        PaneSplit,
        ...pane tree root...
    )],
),
```

Change to:

```rust
(
    Main,
    Node {
        flex_grow: 1.0,
        min_height: Val::Px(0.0),
        ..default()
    },
    children![(
        space_bundle(),
        Active,
        children![(
            Pane,
            PaneSplit,
            HostWindow(pw),
            ZIndex(0),
            Transform::default(),
            GlobalTransform::default(),
            Node {
                flex_grow: 1.0,
                min_height: Val::Px(0.0),
                column_gap: Val::Px(settings.layout.pane.gap),
                row_gap: Val::Px(settings.layout.pane.gap),
                ..default()
            },
            children![(
                leaf_pane_bundle(),
                Active,
                children![(
                    tab_bundle(),
                    Active,
                    children![browser_bundle(
                        &mut meshes,
                        &mut webview_mt,
                        startup_url
                    )],
                )],
            )],
        )],
    )],
),
```

The Space entity now wraps the pane tree root. It has `Active` because it's the only space.

- [ ] **Step 4: Verify compilation**

Run: `cargo check -p vmux_desktop out+err>| tail -20`
Expected: No errors.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat: add Space entity between Main and pane root

Generated by Mistral Vibe.
Co-Authored-By: Mistral Vibe <vibe@mistral.ai>"
```

---

### Task 4: Update focused_tab to 3-level walk

**Files:**
- Modify: `crates/vmux_desktop/src/layout/tab.rs`
- Modify: `crates/vmux_desktop/src/browser.rs` (all focused_tab call sites)

The `focused_tab` helper currently walks 2 levels: Active Pane -> Active Tab. Update it to walk 3 levels: Active Space -> Active Pane -> Active Tab.

- [ ] **Step 1: Update focused_tab signature and implementation in tab.rs**

In `crates/vmux_desktop/src/layout/tab.rs`, add Space import and update `focused_tab`:

```rust
use crate::{
    browser::browser_bundle,
    command::{AppCommand, ReadAppCommands, TabCommand},
    layout::pane::Pane,
    layout::space::Space,
    settings::AppSettings,
};
use bevy::prelude::*;
use bevy_cef::prelude::*;
```

Replace the `focused_tab` function:

```rust
// OLD
pub(crate) fn focused_tab(
    active_pane: &Query<Entity, (With<Active>, With<Pane>)>,
    pane_children: &Query<&Children, With<Pane>>,
    active_tabs: &Query<Entity, (With<Active>, With<Tab>)>,
) -> Option<Entity> {
    let pane = active_pane.single().ok()?;
    let children = pane_children.get(pane).ok()?;
    children.iter().find(|&e| active_tabs.contains(e))
}
```

```rust
// NEW
pub(crate) fn focused_tab(
    active_space: &Query<Entity, (With<Active>, With<Space>)>,
    space_children: &Query<&Children, With<Space>>,
    active_pane: &Query<Entity, (With<Active>, With<Pane>)>,
    pane_children: &Query<&Children, With<Pane>>,
    active_tabs: &Query<Entity, (With<Active>, With<Tab>)>,
) -> Option<Entity> {
    let space = active_space.single().ok()?;
    let space_kids = space_children.get(space).ok()?;
    let pane = space_kids.iter().find(|&e| active_pane.contains(e))
        .or_else(|| active_pane.single().ok())?;
    let children = pane_children.get(pane).ok()?;
    children.iter().find(|&e| active_tabs.contains(e))
}
```

Note: The space_children search finds the Active Pane that's a descendant of the Active Space. Since the Active Pane may be deeply nested inside PaneSplit nodes within the Space, we fall back to `active_pane.single()` when no direct child matches. This works because there's currently only one Active Pane globally. In the future when multiple spaces have their own Active Panes, a recursive descendant search will be needed — but for the initial implementation with one space, this is correct.

- [ ] **Step 2: Update all focused_tab call sites in browser.rs**

In `crates/vmux_desktop/src/browser.rs`, add the new query parameters to every system that calls `focused_tab`.

**sync_keyboard_target** — add `active_space` and `space_children` params:

```rust
// OLD
fn sync_keyboard_target(
    active_pane: Query<Entity, (With<Active>, With<Pane>)>,
    pane_children: Query<&Children, With<Pane>>,
    active_tabs: Query<Entity, (With<Active>, With<Tab>)>,
    child_of_q: Query<&ChildOf>,
    status_q: Query<(), With<Header>>,
    side_sheet_q: Query<(), With<SideSheet>>,
    browser_q: Query<(Entity, Has<CefKeyboardTarget>), With<Browser>>,
    mut commands: Commands,
) {
    let Some(active_tab_entity) = focused_tab(&active_pane, &pane_children, &active_tabs) else {
// NEW
fn sync_keyboard_target(
    active_space: Query<Entity, (With<Active>, With<Space>)>,
    space_children: Query<&Children, With<Space>>,
    active_pane: Query<Entity, (With<Active>, With<Pane>)>,
    pane_children: Query<&Children, With<Pane>>,
    active_tabs: Query<Entity, (With<Active>, With<Tab>)>,
    child_of_q: Query<&ChildOf>,
    status_q: Query<(), With<Header>>,
    side_sheet_q: Query<(), With<SideSheet>>,
    browser_q: Query<(Entity, Has<CefKeyboardTarget>), With<Browser>>,
    mut commands: Commands,
) {
    let Some(active_tab_entity) = focused_tab(&active_space, &space_children, &active_pane, &pane_children, &active_tabs) else {
```

**sync_osr_webview_focus** — add params:

```rust
// OLD
fn sync_osr_webview_focus(
    browsers: NonSend<Browsers>,
    webviews: Query<Entity, With<WebviewSource>>,
    active_pane: Query<Entity, (With<Active>, With<Pane>)>,
    pane_children_q: Query<&Children, With<Pane>>,
    active_tabs: Query<Entity, (With<Active>, With<Tab>)>,
    child_of_q: Query<&ChildOf>,
    ...
) {
    ...
    let active = focused_tab(&active_pane, &pane_children_q, &active_tabs)
// NEW
fn sync_osr_webview_focus(
    browsers: NonSend<Browsers>,
    webviews: Query<Entity, With<WebviewSource>>,
    active_space: Query<Entity, (With<Active>, With<Space>)>,
    space_children: Query<&Children, With<Space>>,
    active_pane: Query<Entity, (With<Active>, With<Pane>)>,
    pane_children_q: Query<&Children, With<Pane>>,
    active_tabs: Query<Entity, (With<Active>, With<Tab>)>,
    child_of_q: Query<&ChildOf>,
    ...
) {
    ...
    let active = focused_tab(&active_space, &space_children, &active_pane, &pane_children_q, &active_tabs)
```

**push_tabs_host_emit** — add params:

```rust
// OLD
fn push_tabs_host_emit(
    mut commands: Commands,
    browsers: NonSend<Browsers>,
    status: Single<Entity, (With<Header>, With<UiReady>)>,
    browser_q: Query<(&PageMetadata, &ChildOf), With<Browser>>,
    active_pane_q: Query<Entity, (With<Active>, With<Pane>)>,
    pane_children_q: Query<&Children, With<Pane>>,
    active_tabs: Query<Entity, (With<Active>, With<Tab>)>,
    child_of_q: Query<&ChildOf>,
    mut last: Local<String>,
) {
    ...
    let Some(active_tab_entity) = focused_tab(&active_pane_q, &pane_children_q, &active_tabs) else {
// NEW
fn push_tabs_host_emit(
    mut commands: Commands,
    browsers: NonSend<Browsers>,
    status: Single<Entity, (With<Header>, With<UiReady>)>,
    browser_q: Query<(&PageMetadata, &ChildOf), With<Browser>>,
    active_space: Query<Entity, (With<Active>, With<Space>)>,
    space_children: Query<&Children, With<Space>>,
    active_pane_q: Query<Entity, (With<Active>, With<Pane>)>,
    pane_children_q: Query<&Children, With<Pane>>,
    active_tabs: Query<Entity, (With<Active>, With<Tab>)>,
    child_of_q: Query<&ChildOf>,
    mut last: Local<String>,
) {
    ...
    let Some(active_tab_entity) = focused_tab(&active_space, &space_children, &active_pane_q, &pane_children_q, &active_tabs) else {
```

**handle_browser_commands** — add params:

```rust
// OLD
fn handle_browser_commands(
    mut reader: MessageReader<AppCommand>,
    active_pane: Query<Entity, (With<Active>, With<Pane>)>,
    pane_children: Query<&Children, With<Pane>>,
    active_tabs: Query<Entity, (With<Active>, With<Tab>)>,
    browsers: Query<(Entity, &ChildOf), (With<Browser>, Without<Header>, Without<SideSheet>)>,
    mut commands: Commands,
) {
    ...
        let Some(active) = focused_tab(&active_pane, &pane_children, &active_tabs) else {
// NEW
fn handle_browser_commands(
    mut reader: MessageReader<AppCommand>,
    active_space: Query<Entity, (With<Active>, With<Space>)>,
    space_children: Query<&Children, With<Space>>,
    active_pane: Query<Entity, (With<Active>, With<Pane>)>,
    pane_children: Query<&Children, With<Pane>>,
    active_tabs: Query<Entity, (With<Active>, With<Tab>)>,
    browsers: Query<(Entity, &ChildOf), (With<Browser>, Without<Header>, Without<SideSheet>)>,
    mut commands: Commands,
) {
    ...
        let Some(active) = focused_tab(&active_space, &space_children, &active_pane, &pane_children, &active_tabs) else {
```

Add `Space` import to browser.rs:

```rust
use crate::{
    ...
    layout::{
        window::{
            VmuxWindow, WEBVIEW_MESH_DEPTH_BIAS, WEBVIEW_Z_HEADER, WEBVIEW_Z_MAIN,
            WEBVIEW_Z_SIDE_SHEET,
        },
        pane::{Pane, PaneSplit},
        side_sheet::SideSheet,
        space::Space,
        tab::{Active, Tab, focused_tab},
    },
    ...
};
```

- [ ] **Step 3: Update focused_tab call in handle_tab_commands (tab.rs)**

The `handle_tab_commands` system in `tab.rs` calls `focused_tab` in the `TabCommand::Close` branch. Add `active_space` and `space_children` params:

```rust
// OLD
fn handle_tab_commands(
    mut reader: MessageReader<AppCommand>,
    active_pane: Query<Entity, (With<Active>, With<Pane>)>,
    pane_children: Query<&Children, With<Pane>>,
    active_tabs: Query<Entity, (With<Active>, With<Tab>)>,
    tab_q: Query<Entity, With<Tab>>,
    settings: Res<AppSettings>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut webview_mt: ResMut<Assets<WebviewExtendStandardMaterial>>,
) {
// NEW
fn handle_tab_commands(
    mut reader: MessageReader<AppCommand>,
    active_space: Query<Entity, (With<Active>, With<Space>)>,
    space_children: Query<&Children, With<Space>>,
    active_pane: Query<Entity, (With<Active>, With<Pane>)>,
    pane_children: Query<&Children, With<Pane>>,
    active_tabs: Query<Entity, (With<Active>, With<Tab>)>,
    tab_q: Query<Entity, With<Tab>>,
    settings: Res<AppSettings>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut webview_mt: ResMut<Assets<WebviewExtendStandardMaterial>>,
) {
```

And update the call:

```rust
// OLD
                let Some(active_tab) = focused_tab(&active_pane, &pane_children, &active_tabs) else {
// NEW
                let Some(active_tab) = focused_tab(&active_space, &space_children, &active_pane, &pane_children, &active_tabs) else {
```

- [ ] **Step 4: Verify compilation**

Run: `cargo check -p vmux_desktop out+err>| tail -20`
Expected: No errors.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat: update focused_tab to 3-level walk (Space > Pane > Tab)

Generated by Mistral Vibe.
Co-Authored-By: Mistral Vibe <vibe@mistral.ai>"
```

---

### Task 5: Filter push_pane_tree_emit to Active Space

**Files:**
- Modify: `crates/vmux_desktop/src/browser.rs`

Currently `push_pane_tree_emit` iterates ALL leaf panes globally. It should only show panes from the Active Space's subtree.

- [ ] **Step 1: Update push_pane_tree_emit to filter by Active Space**

The system needs to walk the Active Space's descendant tree instead of querying all leaf panes globally. Replace the function:

```rust
// OLD
fn push_pane_tree_emit(
    mut commands: Commands,
    browsers: NonSend<Browsers>,
    side_sheet: Option<Single<Entity, (With<SideSheet>, With<UiReady>)>>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    active_pane_q: Query<Entity, (With<Active>, With<Pane>)>,
    active_tab_q: Query<Entity, (With<Active>, With<Tab>)>,
    pane_children: Query<&Children, With<Pane>>,
    tab_q: Query<Entity, With<Tab>>,
    tab_children: Query<&Children>,
    browser_meta: Query<&PageMetadata, With<Browser>>,
    mut last: Local<String>,
) {
    let Some(side_sheet) = side_sheet else {
        return;
    };
    let side_sheet_e = *side_sheet;
    if !browsers.has_browser(side_sheet_e) || !browsers.host_emit_ready(&side_sheet_e) {
        return;
    }
    let active_pane = active_pane_q.single().ok();

    let mut panes: Vec<PaneNode> = Vec::new();
    for pane_entity in &leaf_panes {
```

```rust
// NEW
fn push_pane_tree_emit(
    mut commands: Commands,
    browsers: NonSend<Browsers>,
    side_sheet: Option<Single<Entity, (With<SideSheet>, With<UiReady>)>>,
    active_space: Query<Entity, (With<Active>, With<Space>)>,
    all_children: Query<&Children>,
    leaf_pane_q: Query<(), (With<Pane>, Without<PaneSplit>)>,
    active_pane_q: Query<Entity, (With<Active>, With<Pane>)>,
    active_tab_q: Query<Entity, (With<Active>, With<Tab>)>,
    pane_children: Query<&Children, With<Pane>>,
    tab_q: Query<Entity, With<Tab>>,
    tab_children: Query<&Children>,
    browser_meta: Query<&PageMetadata, With<Browser>>,
    mut last: Local<String>,
) {
    let Some(side_sheet) = side_sheet else {
        return;
    };
    let side_sheet_e = *side_sheet;
    if !browsers.has_browser(side_sheet_e) || !browsers.host_emit_ready(&side_sheet_e) {
        return;
    }
    let active_pane = active_pane_q.single().ok();

    let Ok(space) = active_space.single() else {
        return;
    };
    let space_leaf_panes = collect_leaf_panes(space, &all_children, &leaf_pane_q);

    let mut panes: Vec<PaneNode> = Vec::new();
    for pane_entity in &space_leaf_panes {
        let pane_entity = *pane_entity;
```

Add a helper function (above `push_pane_tree_emit` or at module level):

```rust
fn collect_leaf_panes(
    root: Entity,
    all_children: &Query<&Children>,
    leaf_q: &Query<(), (With<Pane>, Without<PaneSplit>)>,
) -> Vec<Entity> {
    let mut result = Vec::new();
    let mut stack = vec![root];
    while let Some(entity) = stack.pop() {
        if leaf_q.contains(entity) {
            result.push(entity);
        }
        if let Ok(children) = all_children.get(entity) {
            for child in children.iter() {
                stack.push(child);
            }
        }
    }
    result
}
```

The rest of the loop body stays the same, but `pane_entity` is now dereferenced from the Vec iteration. Verify the `is_active` check still works: `active_pane == Some(pane_entity)`.

- [ ] **Step 2: Verify compilation**

Run: `cargo check -p vmux_desktop out+err>| tail -20`
Expected: No errors.

- [ ] **Step 3: Commit**

```bash
git add -A
git commit -m "feat: filter push_pane_tree_emit to Active Space subtree

Generated by Mistral Vibe.
Co-Authored-By: Mistral Vibe <vibe@mistral.ai>"
```

---

### Task 6: Add stub entities (BottomBar, Modal, SideSheet Right/Bottom)

**Files:**
- Modify: `crates/vmux_desktop/src/layout/window.rs`
- Modify: `crates/vmux_desktop/src/layout/side_sheet.rs`
- Modify: `crates/vmux_desktop/src/command.rs`

Add empty stub entities to the hierarchy. They exist as children of Window but have `Display::None`.

- [ ] **Step 1: Add BottomBar and Modal components to window.rs**

Add after the `Main` component definition in `window.rs`:

```rust
#[derive(Component)]
pub(crate) struct BottomBar;

#[derive(Component)]
pub(crate) struct Modal;
```

- [ ] **Step 2: Add SideSheetPosition to side_sheet.rs**

In `crates/vmux_desktop/src/layout/side_sheet.rs`, add a position enum:

```rust
#[derive(Component, PartialEq, Eq)]
pub(crate) enum SideSheetPosition {
    Left,
    Right,
    Bottom,
}
```

- [ ] **Step 3: Add stub entities to setup() children in window.rs**

In the `children![]` block of `setup()`, after the Main child, add stubs. The full children block becomes:

```rust
children![
    (
        SideSheet,
        SideSheetPosition::Left,
        HostWindow(pw),
        Browser,
        Node {
            width: Val::Px(settings.layout.side_sheet.width),
            flex_shrink: 0.0,
            display: Display::None,
            position_type: PositionType::Absolute,
            left: Val::Px(settings.layout.window.padding),
            top: Val::Px(settings.layout.window.padding),
            bottom: Val::Px(settings.layout.window.padding),
            ..default()
        },
        ZIndex(2),
        WebviewSource::new("vmux://side-sheet/"),
        Mesh3d(meshes.add(Plane3d::new(Vec3::Z, Vec2::splat(0.5)))),
        MeshMaterial3d(webview_mt.add(WebviewExtendStandardMaterial {
            base: StandardMaterial {
                unlit: true,
                alpha_mode: AlphaMode::Blend,
                depth_bias: WEBVIEW_MESH_DEPTH_BIAS,
                ..default()
            },
            ..default()
        })),
        WebviewSize(Vec2::new(settings.layout.side_sheet.width, 720.0)),
        Transform::default(),
        GlobalTransform::default(),
        Visibility::Hidden,
    ),
    (
        ZIndex(1),
        HostWindow(pw),
        Browser,
        Node {
            height: Val::Px(HEADER_HEIGHT_PX),
            flex_shrink: 0.0,
            ..default()
        },
        HeaderBundle {
            marker: Header,
            source: WebviewSource::new(HEADER_WEBVIEW_URL),
            mesh: Mesh3d(meshes.add(Plane3d::new(Vec3::Z, Vec2::splat(0.5)))),
            material: MeshMaterial3d(webview_mt.add(
                WebviewExtendStandardMaterial {
                    base: StandardMaterial {
                        unlit: true,
                        alpha_mode: AlphaMode::Blend,
                        depth_bias: WEBVIEW_MESH_DEPTH_BIAS,
                        ..default()
                    },
                    ..default()
                },
            )),
            webview_size: WebviewSize(Vec2::new(1280.0, HEADER_HEIGHT_PX)),
        },
    ),
    (
        Main,
        Node {
            flex_grow: 1.0,
            min_height: Val::Px(0.0),
            ..default()
        },
        children![(
            space_bundle(),
            Active,
            children![(
                Pane,
                PaneSplit,
                HostWindow(pw),
                ZIndex(0),
                Transform::default(),
                GlobalTransform::default(),
                Node {
                    flex_grow: 1.0,
                    min_height: Val::Px(0.0),
                    column_gap: Val::Px(settings.layout.pane.gap),
                    row_gap: Val::Px(settings.layout.pane.gap),
                    ..default()
                },
                children![(
                    leaf_pane_bundle(),
                    Active,
                    children![(
                        tab_bundle(),
                        Active,
                        children![browser_bundle(
                            &mut meshes,
                            &mut webview_mt,
                            startup_url
                        )],
                    )],
                )],
            )],
        )],
    ),
    (
        BottomBar,
        Node {
            height: Val::Px(0.0),
            display: Display::None,
            ..default()
        },
    ),
    (
        SideSheet,
        SideSheetPosition::Right,
        Node {
            width: Val::Px(280.0),
            position_type: PositionType::Absolute,
            right: Val::Px(settings.layout.window.padding),
            top: Val::Px(settings.layout.window.padding),
            bottom: Val::Px(settings.layout.window.padding),
            display: Display::None,
            ..default()
        },
    ),
    (
        SideSheet,
        SideSheetPosition::Bottom,
        Node {
            height: Val::Px(200.0),
            position_type: PositionType::Absolute,
            left: Val::Px(settings.layout.window.padding),
            right: Val::Px(settings.layout.window.padding),
            bottom: Val::Px(settings.layout.window.padding),
            display: Display::None,
            ..default()
        },
    ),
    (
        Modal,
        Node {
            width: Val::Px(600.0),
            height: Val::Px(400.0),
            position_type: PositionType::Absolute,
            left: Val::Percent(50.0),
            top: Val::Percent(50.0),
            margin: UiRect {
                left: Val::Px(-300.0),
                top: Val::Px(-200.0),
                ..default()
            },
            display: Display::None,
            ..default()
        },
    ),
]
```

Add the SideSheetPosition import to window.rs:

```rust
use crate::{
    ...
    layout::side_sheet::{SideSheet, SideSheetPosition},
    ...
};
```

- [ ] **Step 4: Update sync_side_sheet_visibility to only affect Left position**

In `side_sheet.rs`, the `sync_side_sheet_visibility` system's `side_sheet_q` query should filter to only `SideSheetPosition::Left`:

```rust
// OLD
    mut side_sheet_q: Query<(&mut Visibility, &mut Node), With<SideSheet>>,
// NEW
    mut side_sheet_q: Query<(&mut Visibility, &mut Node), (With<SideSheet>, With<SideSheetPosition>)>,
```

And inside the loop, only toggle visibility for `SideSheetPosition::Left`:

Actually, simpler approach — add `SideSheetPosition` to the query and filter:

```rust
fn sync_side_sheet_visibility(
    open: Res<SideSheetOpen>,
    settings: Res<AppSettings>,
    mut side_sheet_q: Query<(&SideSheetPosition, &mut Visibility, &mut Node), With<SideSheet>>,
    mut header_q: Query<&mut Node, (With<Header>, Without<SideSheet>, Without<Main>)>,
    mut main_q: Query<&mut Node, (With<Main>, Without<SideSheet>, Without<Header>)>,
) {
    if !open.is_changed() {
        return;
    }
    let sheet_total = settings.layout.side_sheet.width + settings.layout.pane.gap;
    for (pos, mut vis, mut node) in &mut side_sheet_q {
        if *pos != SideSheetPosition::Left {
            continue;
        }
        if open.0 {
            *vis = Visibility::Inherited;
            node.display = Display::Flex;
        } else {
            *vis = Visibility::Hidden;
            node.display = Display::None;
        }
    }
    for mut node in &mut header_q {
        node.margin.left = if open.0 {
            Val::Px(sheet_total)
        } else {
            Val::Px(0.0)
        };
    }
    for mut node in &mut main_q {
        node.margin.left = if open.0 {
            Val::Px(sheet_total)
        } else {
            Val::Px(0.0)
        };
    }
}
```

- [ ] **Step 5: Add SpaceCommand variants for future use**

In `crates/vmux_desktop/src/command.rs`, expand `SpaceCommand`:

```rust
// OLD
#[derive(OsSubMenu, DefaultKeyBindings, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SpaceCommand {
    #[default]
    #[menu(id = "new_space", label = "New Space")]
    New,
}
// NEW
#[derive(OsSubMenu, DefaultKeyBindings, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SpaceCommand {
    #[default]
    #[menu(id = "new_space", label = "New Space")]
    New,

    #[menu(id = "close_space", label = "Close Space")]
    Close,

    #[menu(id = "next_space", label = "Next Space")]
    Next,

    #[menu(id = "prev_space", label = "Previous Space")]
    Previous,
}
```

Update `handle_space_commands` in `space.rs` to handle all variants:

```rust
fn handle_space_commands(
    mut reader: MessageReader<AppCommand>,
) {
    for cmd in reader.read() {
        let AppCommand::Space(space_cmd) = *cmd else {
            continue;
        };
        match space_cmd {
            SpaceCommand::New => {}
            SpaceCommand::Close => {}
            SpaceCommand::Next => {}
            SpaceCommand::Previous => {}
        }
    }
}
```

- [ ] **Step 6: Verify compilation**

Run: `cargo check -p vmux_desktop out+err>| tail -20`
Expected: No errors.

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "feat: add stub entities for BottomBar, Modal, SideSheet Right/Bottom

Generated by Mistral Vibe.
Co-Authored-By: Mistral Vibe <vibe@mistral.ai>"
```

---

### Task 7: Scope on_pane_cycle to Active Space

**Files:**
- Modify: `crates/vmux_desktop/src/layout/pane.rs`

Currently `on_pane_cycle` iterates ALL leaf panes globally. It should only cycle between panes within the Active Space.

- [ ] **Step 1: Update on_pane_cycle to filter by Active Space descendant tree**

Add imports to pane.rs:

```rust
use crate::{
    browser::browser_bundle,
    command::{AppCommand, PaneCommand, ReadAppCommands, TabCommand},
    layout::space::Space,
    layout::tab::{Active, Tab, tab_bundle},
    settings::AppSettings,
};
```

Replace the `on_pane_cycle` function:

```rust
// OLD
fn on_pane_cycle(
    mut reader: MessageReader<AppCommand>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    active_pane: Query<Entity, (With<Active>, With<Pane>)>,
    mut commands: Commands,
) {
    for cmd in reader.read() {
        let delta: i32 = match cmd {
            AppCommand::Tab(TabCommand::Next) => 1,
            AppCommand::Tab(TabCommand::Previous) => -1,
            _ => continue,
        };
        let mut panes: Vec<Entity> = leaf_panes.iter().collect();
        if panes.len() < 2 {
            continue;
        }
        panes.sort_by_key(|e| e.to_bits());
        let Ok(current_pane) = active_pane.single() else {
            continue;
        };
        let Some(pos) = panes.iter().position(|&e| e == current_pane) else {
            continue;
        };
        let n = panes.len() as i32;
        let idx = (pos as i32 + delta).rem_euclid(n) as usize;
        let target_pane = panes[idx];

        commands.entity(current_pane).remove::<Active>();
        commands.entity(target_pane).insert(Active);
    }
}
```

```rust
// NEW
fn on_pane_cycle(
    mut reader: MessageReader<AppCommand>,
    active_space: Query<Entity, (With<Active>, With<Space>)>,
    all_children: Query<&Children>,
    leaf_pane_q: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    active_pane: Query<Entity, (With<Active>, With<Pane>)>,
    mut commands: Commands,
) {
    for cmd in reader.read() {
        let delta: i32 = match cmd {
            AppCommand::Tab(TabCommand::Next) => 1,
            AppCommand::Tab(TabCommand::Previous) => -1,
            _ => continue,
        };
        let Ok(space) = active_space.single() else {
            continue;
        };
        let mut panes = collect_space_leaf_panes(space, &all_children, &leaf_pane_q);
        if panes.len() < 2 {
            continue;
        }
        panes.sort_by_key(|e| e.to_bits());
        let Ok(current_pane) = active_pane.single() else {
            continue;
        };
        let Some(pos) = panes.iter().position(|&e| e == current_pane) else {
            continue;
        };
        let n = panes.len() as i32;
        let idx = (pos as i32 + delta).rem_euclid(n) as usize;
        let target_pane = panes[idx];

        commands.entity(current_pane).remove::<Active>();
        commands.entity(target_pane).insert(Active);
    }
}

fn collect_space_leaf_panes(
    root: Entity,
    all_children: &Query<&Children>,
    leaf_q: &Query<Entity, (With<Pane>, Without<PaneSplit>)>,
) -> Vec<Entity> {
    let mut result = Vec::new();
    let mut stack = vec![root];
    while let Some(entity) = stack.pop() {
        if leaf_q.contains(entity) {
            result.push(entity);
        }
        if let Ok(children) = all_children.get(entity) {
            for child in children.iter() {
                stack.push(child);
            }
        }
    }
    result
}
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check -p vmux_desktop out+err>| tail -20`
Expected: No errors.

- [ ] **Step 3: Commit**

```bash
git add -A
git commit -m "feat: scope on_pane_cycle to Active Space

Generated by Mistral Vibe.
Co-Authored-By: Mistral Vibe <vibe@mistral.ai>"
```

---

### Task 8: Final verification and cleanup

**Files:**
- All modified files (read-only verification)

- [ ] **Step 1: Full cargo check**

Run: `cargo check -p vmux_desktop out+err>| tail -30`
Expected: No errors, possibly some warnings.

- [ ] **Step 2: Check for remaining DisplayGlass references**

Run: `grep -r "DisplayGlass" crates/vmux_desktop/src/`
Expected: No matches.

Run: `grep -r "display::" crates/vmux_desktop/src/`
Expected: No matches referencing the old display module.

Run: `grep -r "fit_display_glass" crates/vmux_desktop/src/`
Expected: No matches.

- [ ] **Step 3: Check for remaining comments (code style rule)**

Run: `grep -rn "^\s*//" crates/vmux_desktop/src/layout/window.rs crates/vmux_desktop/src/layout/space.rs`
Expected: No matches (no comments in code).

- [ ] **Step 4: Commit if any cleanup was needed**

```bash
git add -A
git commit -m "chore: cleanup remaining DisplayGlass references

Generated by Mistral Vibe.
Co-Authored-By: Mistral Vibe <vibe@mistral.ai>"
```

---

## Summary of Entity Hierarchy After All Tasks

```
VmuxWindow (component, owns glass mesh + root Node)
├── SideSheet + SideSheetPosition::Left    (Browser, absolute, existing behavior)
├── Header                                  (Browser, ZIndex(1))
├── Main                                    (flex_grow: 1)
│   └── Space + Active                      (absolute fill, Display::Flex)
│       └── Pane + PaneSplit                (pane tree root)
│           └── Pane + Active               (leaf)
│               └── Tab + Active
│                   └── Browser
├── BottomBar                               (stub, Display::None)
├── SideSheet + SideSheetPosition::Right    (stub, Display::None)
├── SideSheet + SideSheetPosition::Bottom   (stub, Display::None)
└── Modal                                   (stub, Display::None)
```

Active chain: Active Space -> Active Pane -> Active Tab -> Browser
