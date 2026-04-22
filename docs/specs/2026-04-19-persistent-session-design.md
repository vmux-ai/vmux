# Persistent Session & Profile Design

## Overview

Persist all vmux state (layout, tabs, browsing history) across restarts using moonshine-save at the component level. Introduce a Profile entity as the root of the entity tree, preparing for Arc-style multi-profile support.

## Phases

- **Phase 1**: Session persistence + history (this spec)
- **Phase 2**: Multi-profile support (outlined, not fully specified)

## Data Model

### Model/View Separation

Model components derive `Reflect` and `#[require(Save)]`. View components (meshes, materials, Node layout, Browser handles, WebviewSource) are NOT saved -- they are rebuilt on load by observers.

### Saveable Components

```rust
#[derive(Component, Reflect, Default)]
#[reflect(Component)]
#[require(Save)]
pub struct Profile {
    pub name: String,
    pub color: [f32; 4],
    pub icon: Option<String>,
}

#[derive(Component, Reflect, Default)]
#[reflect(Component)]
#[require(Save)]
pub struct Space {
    pub name: String,
}

#[derive(Component, Reflect, Default)]
#[reflect(Component)]
#[require(Save)]
pub struct Pane;

#[derive(Component, Reflect)]
#[reflect(Component)]
#[require(Save)]
pub struct PaneSplit {
    pub direction: PaneSplitDirection, // Row | Column
}

#[derive(Component, Reflect, Default)]
#[reflect(Component)]
#[require(Save)]
pub struct Tab {
    pub scroll_x: f32,
    pub scroll_y: f32,
}

#[derive(Component, Clone, Debug, Reflect, Default)]
#[reflect(Component)]
#[require(Save)]
pub struct PageMetadata {
    pub title: String,
    pub url: String,
    pub favicon_url: String,
}

#[derive(Component, Reflect, Default)]
#[reflect(Component)]
#[require(Save)]
pub struct Visit;

#[derive(Component, Clone, Debug, Reflect, Default)]
#[reflect(Component)]
#[require(Save)]
pub struct CreatedAt(pub i64); // unix millis

#[derive(Component, Clone, Debug, Reflect, Default)]
#[reflect(Component)]
#[require(Save)]
pub struct LastActivatedAt(pub i64); // unix millis
```

### Entity Tree (on disk)

```
Profile("default") + CreatedAt + LastActivatedAt
├── Space("main") + LastActivatedAt + CreatedAt
│   └── Pane + PaneSplit{Row}
│       ├── Pane + LastActivatedAt
│       │   ├── Tab{scroll} + PageMetadata + CreatedAt + LastActivatedAt
│       │   └── Tab{scroll} + PageMetadata + CreatedAt + LastActivatedAt
│       └── Pane + LastActivatedAt
│           └── Tab{scroll} + PageMetadata + CreatedAt + LastActivatedAt
├── Visit + PageMetadata + CreatedAt
├── Visit + PageMetadata + CreatedAt
└── ...
```

### View Components (not saved, rebuilt on load)

- `Mesh3d`, `MeshMaterial3d`, `Transform`, `GlobalTransform`
- `Node`, `ZIndex`
- `Browser` marker, `WebviewSource`, `WebviewSize`, `HostWindow`
- `Pickable`, `Visibility`

## LastActivatedAt Replaces Active

The `Active` marker component is removed entirely. Active state is determined by the entity with the maximum `LastActivatedAt` among siblings at each level (Profile, Space, Pane, Tab). Systems query `LastActivatedAt` directly -- no cached resource needed.

```rust
fn active_among<'a>(
    entities: impl Iterator<Item = (Entity, &'a LastActivatedAt)>,
) -> Option<Entity> {
    entities.max_by_key(|(_, ts)| ts.0).map(|(e, _)| e)
}
```

Activation changes from insert/remove `Active` to updating `LastActivatedAt`:

```rust
// Before:
// commands.entity(old).remove::<Active>();
// commands.entity(new).insert(Active);

// After (old entity retains its older timestamp -- no removal needed):
commands.entity(new).insert(LastActivatedAt::now());
```

### Affected Systems

| System | Change |
|--------|--------|
| `sync_space_visibility` | `Has<Active>` -> max `LastActivatedAt` among Space siblings |
| `sync_tab_picking` (ZIndex) | `Has<Active>` -> max `LastActivatedAt` among Tab siblings per pane |
| `focused_tab` | chain `active_among` at Space -> Pane -> Tab |
| `sync_keyboard_target` | delegates to `focused_tab` (no direct change) |
| `poll_cursor_pane_focus` | insert `LastActivatedAt::now()` instead of insert `Active` |
| `handle_tab_commands` | insert `LastActivatedAt::now()` instead of insert/remove `Active` |
| `handle_pane_commands` | insert `LastActivatedAt::now()` instead of insert/remove `Active` |
| `on_pane_select` | insert `LastActivatedAt::now()` instead of insert/remove `Active` |

## Save Pipeline

### Triggers

Structural changes fire a `SaveRequest` event:

- Tab: new, close, navigate (URL change)
- Pane: split, close
- Space: new, close, switch
- Visit spawned (navigation complete)

### Timing

Debounced save (500ms after last change) + periodic backup (60s).

```rust
#[derive(Resource)]
struct AutoSave {
    debounce: Timer,  // 500ms, resets on each trigger
    periodic: Timer,  // 60s, repeating
    dirty: bool,
}

fn trigger_save_on_change(/* detect Added/Removed/Changed on Save-marked components */) {
    // set dirty = true, reset debounce
}

fn auto_save_system(
    time: Res<Time>,
    mut auto_save: ResMut<AutoSave>,
    mut commands: Commands,
) {
    auto_save.periodic.tick(time.delta());
    if auto_save.dirty {
        auto_save.debounce.tick(time.delta());
        if auto_save.debounce.finished() {
            commands.trigger(SaveRequest);
            auto_save.dirty = false;
        }
    }
    if auto_save.periodic.just_finished() {
        commands.trigger(SaveRequest);
    }
}
```

### Output

```
~/Library/Application Support/vmux/session.ron
```

## Load Pipeline (Startup)

```
App startup
    |
    v
session.ron exists?
    | no --> spawn default layout (current window.rs setup())
    | yes
    v
moonshine-save load_from_file("session.ron")
    |
    v
Entities restored: Profile, Space, Pane, Tab, Visit
(model only -- no meshes, no Browser, no Node)
    |
    v
Rebuild observers fire on restored components:
    |
    |-- on Tab + PageMetadata added:
    |     -> spawn child Browser with view bundle (mesh, material, WebviewSource(url))
    |     -> attach Node layout
    |
    |-- on Pane added:
    |     -> attach Node layout (leaf or split based on PaneSplit presence)
    |
    |-- on Space added:
    |     -> attach Node layout, sync visibility from LastActivatedAt
    |
    +-- on Profile added:
          -> configure CEF cache path from profile name
```

### Shell vs Session Entities

The VmuxWindow shell (header, side sheets, bottom bar, modal) is NOT part of the saved session. These are chrome entities spawned unconditionally at startup by `window.rs::setup()`. The saved Profile tree is parented under the `Main` container inside VmuxWindow, replacing the default Space/Pane/Tab subtree that `setup()` currently spawns inline.

On load: `setup()` spawns the VmuxWindow shell but skips the default Space/Pane/Tab children. Instead, restored Profile/Space/Pane/Tab entities are inserted under `Main`.

On fresh start (no session.ron): `setup()` spawns the shell AND a default Profile + Space + Pane + Tab subtree as the initial session.

### Startup Behavior

Auto-restore silently. No prompt. If `session.ron` is missing or corrupt, fall back to spawning a default single-space layout.

## PageMetadata Sync

```
CEF chrome state update
    |
    v
apply_chrome_state_from_cef (existing)
    updates PageMetadata on Browser entity
    |
    v
sync_page_metadata_to_tab (new)
    copies PageMetadata from Browser up to parent Tab
    |
    v
on URL change detected:
    clone Tab's PageMetadata -> spawn Visit + PageMetadata + CreatedAt under Profile
```

## History (vmux://history)

Visit entities live under the Profile entity. The existing `vmux_history` crate is refactored:

- Remove `spawn_sample_history_visits` -- visits are real entities from save file or spawned on navigation
- Remove duplicate `PageMetadata` definition -- reuse from `vmux_header`
- `push_history_via_host_emit` reads Visit + PageMetadata entities as before

## CEF Cache Restructure

Current: `~/Library/Application Support/vmux/` (flat, single cookie jar)

New: `~/Library/Application Support/vmux/profiles/default/`

The `cef_root_cache_path()` function changes to return the profile-specific subdirectory. Phase 1 hardcodes `"default"`. Phase 2 derives the path from `Profile.name`.

```
~/Library/Application Support/vmux/
├── session.ron
└── profiles/
    └── default/
        ├── Cache/
        ├── Cookies
        └── ...
```

## Profile Plugin (Phase 2 Outline)

### Concept

Arc-style profiles. Each Profile is an isolated browsing identity with its own cookies, storage, layout, and history. A Space is bound to exactly one Profile.

### Multi-Profile Entity Tree

```
Profile("work") + CreatedAt + LastActivatedAt
├── Space("dev") + ...
│   └── ... (tabs with work logins)
├── Visit + ...
└── CEF cache: profiles/work/

Profile("personal") + CreatedAt + LastActivatedAt
├── Space("browse") + ...
│   └── ... (tabs with personal logins)
├── Visit + ...
└── CEF cache: profiles/personal/
```

### CEF Isolation

Each Profile gets a separate `RequestContext` with its own `cache_path`. When spawning a Browser, the system walks up to find the ancestor Profile and uses its cache dir.

### Profile Switching

Switching profiles = updating `LastActivatedAt` on the target Profile. All Spaces under the previous Profile become hidden; all Spaces under the new Profile become visible.

### Profile UI

Profile indicator shown in the header bar (right side), matching Arc/Chrome's profile avatar button. Implemented in `vmux_header` Dioxus app, receives profile data via `HostEmitEvent`.

- Phase 1: header shows default profile indicator (static)
- Phase 2: header shows active profile avatar, click to switch/manage

### Out of Scope

- Backend auth (profiles are local-only)
- Import/export profiles
- Per-tab profile override

## Phase 1 Deliverables

1. Model/view separation -- `Reflect` + `Save` on model components
2. `LastActivatedAt` replaces `Active` everywhere
3. Save pipeline -- debounced + periodic, writes `session.ron`
4. Load pipeline -- restore from `session.ron`, observers rebuild view
5. `PageMetadata` synced from Browser to Tab, cloned to Visit on navigation
6. Visit entities spawned under Profile on navigation complete
7. CEF cache restructured to `profiles/default/`
8. `vmux_history` refactored to read real Visit entities
