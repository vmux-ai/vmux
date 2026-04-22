# Layout Architecture Design

## Goal

Restructure the vmux entity hierarchy from a flat DisplayGlass-based layout to a Window → Space → Pane → Tab model inspired by Arc Browser, supporting multiple workspaces within a single window.

## Entity Hierarchy

```
Window (replaces DisplayGlass, owns glass mesh + Node)
├── Header               (Browser, shared, data scoped to Active Space)
├── Main                 (layout container, flex_grow: 1)
│   └── Space (multiple children, Active = visible)
│       └── Pane+PaneSplit (root of split tree)
│           └── Pane (leaf, Active per-Space)
│               └── Tab (Active per-Pane)
│                   └── Browser
├── BottomBar            (stub, shared, Browser)
├── SideSheet::Left      (stub, shared, absolute, data scoped to Active Space)
├── SideSheet::Right     (stub, shared, absolute)
├── SideSheet::Bottom    (stub, shared, absolute)
└── Modal                (stub, shared, absolute, webview command bar)
```

Space is inside Main in the entity tree but logically scopes what all shared chrome displays. Data-push systems (`push_tabs_host_emit`, `push_pane_tree_emit`) filter by the Active chain: Active Space → Active Pane → Active Tab.

## Concepts

### Window

Replaces `DisplayGlass`. One per OS window. Owns:
- The 3D glass mesh (`Mesh3d` + `RoundedMaterial`)
- The UI root `Node` (100% width/height, flex column, padding)
- `UiTargetCamera` pointing at the main camera

Window is the visual shell. Its children are the shared chrome (Header, SideSheets, BottomBar, Modal) and `Main`.

### Main

A layout container that sits between the shared chrome and the Spaces. Its `Node` takes remaining flex space after Header and BottomBar. It holds all Space entities as children. Only the Active Space is visible — others get `Display::None`.

Main manages the pane tree root lifecycle. When switching spaces, Main hides the old Space and shows the new one.

### Space

Arc-style workspace. Multiple Spaces exist as children of Main. Each Space has:
- Its own pane split tree (Pane + PaneSplit hierarchy)
- Its own Active Pane (exactly 1 per Space)
- Independent tab state per pane

Switching spaces = move `Active` between Space siblings + toggle `Display::None`.

Spaces are purely logical containers for the pane tree. They have a `Node` that fills Main (flex_grow: 1, 100% width/height).

### Header, BottomBar

Window-level shared chrome. Browser-based webviews. Header shows tab bar for the Active Space's Active Pane. BottomBar is a webview status bar.

Both persist across Space switches. Their content updates reactively based on the Active chain.

### SideSheet (Left, Right, Bottom)

Window-level shared panels. Stub implementation — entity exists with a Browser component but no behavior beyond toggle visibility. Positioned absolutely with z-index layering.

The Left SideSheet replaces the current single SideSheet. It shows the space navigator (all spaces, their panes, tabs).

### Modal

Window-level overlay for command bar. Stub implementation — entity exists, no behavior. Will be a webview-based command bar (cmd+shift+p style) in the future.

## Active Component Pattern

`Active` is a shared marker component. Invariant: exactly 1 Active per parent scope.

| Entity type | Active cardinality | Scope |
|-------------|-------------------|-------|
| Space | 1 per Main (Window) | Visible workspace |
| Pane | 1 per Space | Focused pane in that workspace |
| Tab | 1 per Pane | Selected/visible tab in that pane |

### Active Chain (keyboard target derivation)

```
Main → Active Space → Active Pane → Active Tab → Browser
```

The `focused_tab` helper walks this chain:

```rust
pub(crate) fn focused_tab(...) -> Option<Entity> {
    let space = active_space.single().ok()?;        // Active Space in Main
    let pane = find_active_pane_in(space)?;          // Active Pane in that Space
    let tab = find_active_tab_in(pane)?;             // Active Tab in that Pane
    Some(tab)
}
```

This replaces the current 2-level walk (Active Pane → Active Tab) with a 3-level walk.

## Component Definitions

### New Components

```rust
#[derive(Component)]
pub(crate) struct Window;

#[derive(Component)]
pub(crate) struct Main;

#[derive(Component)]
pub(crate) struct Space;

#[derive(Component)]
pub(crate) struct BottomBar;

#[derive(Component)]
pub(crate) struct Modal;

#[derive(Component, PartialEq, Eq)]
pub(crate) enum SideSheetPosition {
    Left,
    Right,
    Bottom,
}
```

### Retained Components (unchanged)

- `Pane`, `PaneSplit` — pane split tree
- `Tab`, `Active` — tab selection and focus
- `Browser` — webview entity marker
- `Header` — header chrome marker
- `SideSheet` — side sheet marker (now paired with `SideSheetPosition`)

### Removed

- `DisplayGlass` — replaced by `Window`

## Layout (Node hierarchy)

```
Window: Node { width: 100%, height: 100%, flex_direction: Column, padding }
├── Header: Node { height: HEADER_HEIGHT_PX, flex_shrink: 0 }
├── Main: Node { flex_grow: 1, min_height: 0 }
│   └── Space: Node { width: 100%, height: 100%, position: Absolute, inset: 0 }
│       └── Pane+PaneSplit: Node { flex_grow: 1, flex_direction: Row/Column }
│           └── Pane: Node { flex_grow: 1, flex_basis: 0 }
│               └── Tab: Node { position: Absolute, inset: 0 }
│                   └── Browser: Node { position: Absolute, inset: 0 }
├── BottomBar: Node { height: BOTTOM_BAR_HEIGHT_PX, flex_shrink: 0 }  (stub)
├── SideSheet(Left): Node { position: Absolute, left: 0, width: X }   (stub)
├── SideSheet(Right): Node { position: Absolute, right: 0, width: X } (stub)
├── SideSheet(Bottom): Node { position: Absolute, bottom: 0, height: X } (stub)
└── Modal: Node { position: Absolute, center }                        (stub)
```

Inactive Spaces get `Display::None`. Active Space gets `Display::Flex` (or default).

## System Changes

### display.rs → window.rs

Rename module. `setup` spawns the new hierarchy:
- `Window` entity with glass mesh (was `DisplayGlass`)
- `Header` browser as child
- `Main` container as child
- One initial `Space` with Active, containing one leaf Pane + Tab + Browser
- Stub entities for BottomBar, SideSheets, Modal

`fit_display_glass_to_window` → `fit_window_to_screen` (same logic, queries `Window` instead of `DisplayGlass`).

### browser.rs

- All queries referencing `DisplayGlass` → `Window`
- `sync_children_to_ui`: `glass` Single query uses `Window` instead of `DisplayGlass`
- `focused_tab` updated to walk 3 levels (Space → Pane → Tab)
- `push_pane_tree_emit`: iterate panes within the Active Space only

### space.rs (new)

- `SpacePlugin`: systems for space switching
- `handle_space_commands`: New/Close/Next/Previous space commands
- `sync_space_visibility`: set `Display::None` on inactive spaces, `Display::Flex` on active

### pane.rs

- Pane queries scoped to Active Space where needed
- `leaf_panes` query may need filtering to current space's subtree

### tab.rs

- `focused_tab` helper updated: Active Space → Active Pane → Active Tab
- Add `active_space` query parameter

### side_sheet.rs

- `SideSheet` component now paired with `SideSheetPosition`
- Current left SideSheet logic preserved, adapted for `SideSheetPosition::Left`
- Right/Bottom stubs: entities exist with `Display::None`

### focus_ring.rs

- Query `Window` instead of `DisplayGlass` for glass dimensions

## Scope for First Implementation

### Fully implemented
- Window entity (replaces DisplayGlass)
- Main container
- Space (create 1 default space, space switching infrastructure)
- Pane/Tab within Space (existing behavior preserved)
- Header (existing, re-parented under Window)
- focused_tab 3-level walk
- All browser.rs system updates

### Stub only (entity exists, no behavior)
- SideSheet::Left (current SideSheet behavior preserved)
- SideSheet::Right, SideSheet::Bottom (Display::None)
- Modal (Display::None)
- BottomBar (Display::None)
- Multiple space creation/switching commands (infrastructure ready, no keybindings)

## Migration Path

1. Rename `DisplayGlass` → `Window`, update all references
2. Add `Main` container between Window and pane tree
3. Add `Space` entity between Main and pane root
4. Update `focused_tab` to 3-level walk
5. Update all browser.rs systems for new hierarchy
6. Add stub entities for SideSheet positions, Modal, BottomBar
7. Add `SpacePlugin` with visibility sync
