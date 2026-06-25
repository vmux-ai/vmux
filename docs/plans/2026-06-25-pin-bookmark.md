# Pins + Bookmarks (Interactive Feature) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add Dia-style pins (favicon grid) + bookmarks (entries, nestable in collapsible folders) to vmux: saved per-profile, rendered in the left chrome above the tabs, with a command-bar bookmark icon, Cmd+D toggle, and right-click context menus. (MCP control is a separate plan: `2026-06-25-pin-bookmark-mcp.md`.)

**Architecture:** Composition-first ECS. Pins/bookmarks/folders are entities composed from small shared components (`Pin`, `Bookmark`, `Folder`, `Collapsed` markers + `Uuid` + reused `PageMetadata`/`Order`/`Name`/`ChildOf`). Source of truth = ECS, mirroring tabs. Mutations flow through a `BookmarkOp` message applied by one system; a broadcast system derives a snapshot DTO sent to the wasm page over the existing rkyv bin-event bus. Persistence is a second moonshine-save pipeline scoped by a marker query to `profile_dir()/bookmarks.ron`, disjoint from `space.ron`.

**Tech Stack:** Rust, Bevy (ECS + messages), moonshine-save, CEF + Dioxus (wasm page), rkyv bin-event bus, `uuid` crate.

**Rendering rule (composition):** pins grid = entities `With<Pin>`; bookmark list = entities `With<Bookmark>, Without<Pin>`. "Pin" promotes a bookmark out of the list into the grid (adds `Pin`); Cmd+D / the bar icon toggle the `Bookmark` marker (entry-centric).

**Spec:** `docs/specs/2026-06-25-pin-bookmark-design.md`.

---

## Phase 1 — ECS components (`vmux_core`)

### Task 1: Add bookmark marker components + `Uuid`

**Files:**
- Modify: `crates/vmux_core/src/lib.rs` (component defs near the other reflected components ~line 128-200; register chain in `CorePlugin` ~line 44-57; test mod ~line 202)

- [ ] **Step 1: Write the failing test**

Add to the `#[cfg(test)] mod tests` in `crates/vmux_core/src/lib.rs`:

```rust
    #[test]
    fn registers_bookmark_components() {
        let mut app = App::new();
        app.add_plugins(CorePlugin);
        let registry = app.world().resource::<AppTypeRegistry>().read();
        assert!(registry.get(std::any::TypeId::of::<Pin>()).is_some());
        assert!(registry.get(std::any::TypeId::of::<Bookmark>()).is_some());
        assert!(registry.get(std::any::TypeId::of::<Folder>()).is_some());
        assert!(registry.get(std::any::TypeId::of::<Collapsed>()).is_some());
        assert!(registry.get(std::any::TypeId::of::<Uuid>()).is_some());
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vmux_core registers_bookmark_components`
Expected: FAIL — `cannot find type Pin in this scope`.

- [ ] **Step 3: Add the components**

Add near the other `#[type_path = "vmux_core"]` components in `crates/vmux_core/src/lib.rs` (e.g. after `Active` ~line 171). Note: markers are `Copy`; `Uuid` holds a `String` so it is not `Copy`. Do NOT add `#[require(Save)]` — the bookmarks persistence pipeline scopes entities by marker query, and `Save` would pull them into `space.ron`.

```rust
#[cfg(not(target_arch = "wasm32"))]
#[derive(Component, Clone, Copy, Debug, Reflect, Default, PartialEq, Eq)]
#[reflect(Component, Default)]
#[type_path = "vmux_core"]
pub struct Pin;

#[cfg(not(target_arch = "wasm32"))]
#[derive(Component, Clone, Copy, Debug, Reflect, Default, PartialEq, Eq)]
#[reflect(Component, Default)]
#[type_path = "vmux_core"]
pub struct Bookmark;

#[cfg(not(target_arch = "wasm32"))]
#[derive(Component, Clone, Copy, Debug, Reflect, Default, PartialEq, Eq)]
#[reflect(Component, Default)]
#[type_path = "vmux_core"]
pub struct Folder;

#[cfg(not(target_arch = "wasm32"))]
#[derive(Component, Clone, Copy, Debug, Reflect, Default, PartialEq, Eq)]
#[reflect(Component, Default)]
#[type_path = "vmux_core"]
pub struct Collapsed;

#[cfg(not(target_arch = "wasm32"))]
#[derive(Component, Clone, Debug, Reflect, Default, PartialEq, Eq)]
#[reflect(Component, Default)]
#[type_path = "vmux_core"]
pub struct Uuid(pub String);
```

- [ ] **Step 4: Register the types in `CorePlugin`**

In `crates/vmux_core/src/lib.rs`, extend the `register_type` chain (~line 56, after `.register_type::<Active>()`):

```rust
            .register_type::<Pin>()
            .register_type::<Bookmark>()
            .register_type::<Folder>()
            .register_type::<Collapsed>()
            .register_type::<Uuid>()
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test -p vmux_core registers_bookmark_components`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_core/src/lib.rs
git commit -m "feat(core): add Pin/Bookmark/Folder/Collapsed/Uuid components"
```

---

## Phase 2 — Host ops + apply system (`vmux_layout`)

This phase introduces the `BookmarkOp` message and the system that mutates the ECS. No UI / persistence yet — pure ECS logic, fully unit-testable.

### Task 2: `BookmarkOp` message + module skeleton

**Files:**
- Create: `crates/vmux_layout/src/bookmark.rs`
- Modify: `crates/vmux_layout/src/lib.rs` (add `pub mod bookmark;` and register `BookmarkPlugin` in `LayoutPlugin`)

- [ ] **Step 1: Create the module with the op enum + plugin**

Create `crates/vmux_layout/src/bookmark.rs`:

```rust
use bevy::prelude::*;
use vmux_core::{Bookmark, Collapsed, Folder, Order, PageMetadata, Pin, Uuid};

/// Data-carrying bookmark mutation. Emitted by the page (translated from
/// `BookmarksCommandEvent`), the Cmd+D adapter, and MCP.
#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub enum BookmarkOp {
    ToggleForUrl {
        url: String,
        title: String,
        favicon_url: String,
    },
    Add {
        url: String,
        title: String,
        favicon_url: String,
        folder: Option<String>,
    },
    Remove {
        uuid: String,
    },
    AddFolder {
        name: String,
    },
    RemoveFolder {
        uuid: String,
    },
    RenameFolder {
        uuid: String,
        name: String,
    },
    ToggleFolder {
        uuid: String,
    },
    Pin {
        uuid: String,
    },
    PinUrl {
        url: String,
        title: String,
        favicon_url: String,
    },
    Unpin {
        uuid: String,
    },
}

pub struct BookmarkPlugin;

impl Plugin for BookmarkPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<BookmarkOp>()
            .add_systems(Update, apply_bookmark_ops);
    }
}

fn new_uuid() -> Uuid {
    Uuid(uuid::Uuid::new_v4().to_string())
}

fn find_by_uuid(target: &str, q: &Query<(Entity, &Uuid)>) -> Option<Entity> {
    q.iter()
        .find(|(_, id)| id.0 == target)
        .map(|(entity, _)| entity)
}

fn next_top_order(orders: impl Iterator<Item = u32>) -> Order {
    Order(orders.max().map(|m| m + 1).unwrap_or(0))
}

fn apply_bookmark_ops(
    mut reader: MessageReader<BookmarkOp>,
    ids: Query<(Entity, &Uuid)>,
    bookmarks: Query<(Entity, &PageMetadata), With<Bookmark>>,
    pinned: Query<(), With<Pin>>,
    folder_q: Query<(), With<Folder>>,
    collapsed_q: Query<(), With<Collapsed>>,
    orders: Query<&Order>,
    children_q: Query<&Children>,
    mut commands: Commands,
) {
    for op in reader.read() {
        match op {
            BookmarkOp::ToggleForUrl {
                url,
                title,
                favicon_url,
            } => {
                let existing = bookmarks
                    .iter()
                    .find(|(_, meta)| &meta.url == url)
                    .map(|(entity, _)| entity);
                if let Some(entity) = existing {
                    // Remove the Bookmark marker; despawn if not also a Pin.
                    if pinned.get(entity).is_ok() {
                        commands.entity(entity).remove::<Bookmark>();
                    } else {
                        commands.entity(entity).despawn();
                    }
                } else {
                    let order = next_top_order(orders.iter().map(|o| o.0));
                    commands.spawn((
                        Bookmark,
                        new_uuid(),
                        PageMetadata {
                            title: title.clone(),
                            url: url.clone(),
                            favicon_url: favicon_url.clone(),
                            bg_color: None,
                        },
                        order,
                    ));
                }
            }
            BookmarkOp::Add {
                url,
                title,
                favicon_url,
                folder,
            } => {
                let order = next_top_order(orders.iter().map(|o| o.0));
                let mut e = commands.spawn((
                    Bookmark,
                    new_uuid(),
                    PageMetadata {
                        title: title.clone(),
                        url: url.clone(),
                        favicon_url: favicon_url.clone(),
                        bg_color: None,
                    },
                    order,
                ));
                if let Some(folder_uuid) = folder
                    && let Some(folder_entity) = find_by_uuid(folder_uuid, &ids)
                    && folder_q.get(folder_entity).is_ok()
                {
                    e.insert(ChildOf(folder_entity));
                }
            }
            BookmarkOp::Remove { uuid } => {
                if let Some(entity) = find_by_uuid(uuid, &ids) {
                    commands.entity(entity).despawn();
                }
            }
            BookmarkOp::AddFolder { name } => {
                let order = next_top_order(orders.iter().map(|o| o.0));
                commands.spawn((Folder, new_uuid(), Name::new(name.clone()), order));
            }
            BookmarkOp::RemoveFolder { uuid } => {
                if let Some(folder_entity) = find_by_uuid(uuid, &ids) {
                    if let Ok(children) = children_q.get(folder_entity) {
                        for child in children.iter() {
                            commands.entity(child).remove::<ChildOf>();
                        }
                    }
                    commands.entity(folder_entity).despawn();
                }
            }
            BookmarkOp::RenameFolder { uuid, name } => {
                if let Some(folder_entity) = find_by_uuid(uuid, &ids) {
                    commands.entity(folder_entity).insert(Name::new(name.clone()));
                }
            }
            BookmarkOp::ToggleFolder { uuid } => {
                if let Some(folder_entity) = find_by_uuid(uuid, &ids) {
                    if collapsed_q.get(folder_entity).is_ok() {
                        commands.entity(folder_entity).remove::<Collapsed>();
                    } else {
                        commands.entity(folder_entity).insert(Collapsed);
                    }
                }
            }
            BookmarkOp::Pin { uuid } => {
                if let Some(entity) = find_by_uuid(uuid, &ids) {
                    commands.entity(entity).insert(Pin).remove::<ChildOf>();
                }
            }
            BookmarkOp::PinUrl {
                url,
                title,
                favicon_url,
            } => {
                let order = next_top_order(orders.iter().map(|o| o.0));
                commands.spawn((
                    Pin,
                    new_uuid(),
                    PageMetadata {
                        title: title.clone(),
                        url: url.clone(),
                        favicon_url: favicon_url.clone(),
                        bg_color: None,
                    },
                    order,
                ));
            }
            BookmarkOp::Unpin { uuid } => {
                if let Some(entity) = find_by_uuid(uuid, &ids) {
                    if bookmarks.get(entity).is_ok() {
                        commands.entity(entity).remove::<Pin>();
                    } else {
                        commands.entity(entity).despawn();
                    }
                }
            }
        }
    }
}
```

- [ ] **Step 2: Wire the module + plugin**

In `crates/vmux_layout/src/lib.rs`: add `pub mod bookmark;` with the other `pub mod` lines, and add `BookmarkPlugin` to the `LayoutPlugin` `build` (find where sibling plugins like `TabPlugin`, `TogglePlugin` are added via `.add_plugins((...))` and append `bookmark::BookmarkPlugin`).

- [ ] **Step 3: Build to verify it compiles**

Run: `cargo build -p vmux_layout`
Expected: compiles (warnings about unused `BookmarkOp` variants are fine until tests use them).

- [ ] **Step 4: Commit**

```bash
git add crates/vmux_layout/src/bookmark.rs crates/vmux_layout/src/lib.rs
git commit -m "feat(layout): add BookmarkOp message + apply_bookmark_ops system"
```

### Task 3: Unit-test the apply system (add / toggle / remove)

**Files:**
- Modify: `crates/vmux_layout/src/bookmark.rs` (append `#[cfg(test)] mod tests`)

- [ ] **Step 1: Write failing tests**

Append to `crates/vmux_layout/src/bookmark.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<BookmarkOp>()
            .add_systems(Update, apply_bookmark_ops);
        app
    }

    fn send(app: &mut App, op: BookmarkOp) {
        app.world_mut().resource_mut::<Messages<BookmarkOp>>().write(op);
        app.update();
    }

    #[test]
    fn add_creates_bookmark_entity() {
        let mut app = test_app();
        send(
            &mut app,
            BookmarkOp::Add {
                url: "https://a.test".into(),
                title: "A".into(),
                favicon_url: String::new(),
                folder: None,
            },
        );
        let count = app
            .world_mut()
            .query_filtered::<Entity, With<Bookmark>>()
            .iter(app.world())
            .count();
        assert_eq!(count, 1);
    }

    #[test]
    fn toggle_for_url_is_idempotent_add_then_remove() {
        let mut app = test_app();
        let op = || BookmarkOp::ToggleForUrl {
            url: "https://a.test".into(),
            title: "A".into(),
            favicon_url: String::new(),
        };
        send(&mut app, op());
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<Bookmark>>()
                .iter(app.world())
                .count(),
            1
        );
        send(&mut app, op());
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<Bookmark>>()
                .iter(app.world())
                .count(),
            0
        );
    }

    #[test]
    fn remove_despawns_by_uuid() {
        let mut app = test_app();
        send(
            &mut app,
            BookmarkOp::Add {
                url: "https://a.test".into(),
                title: "A".into(),
                favicon_url: String::new(),
                folder: None,
            },
        );
        let uuid = app
            .world_mut()
            .query_filtered::<&Uuid, With<Bookmark>>()
            .single(app.world())
            .unwrap()
            .0
            .clone();
        send(&mut app, BookmarkOp::Remove { uuid });
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<Bookmark>>()
                .iter(app.world())
                .count(),
            0
        );
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p vmux_layout bookmark::tests`
Expected: PASS (3 tests).

- [ ] **Step 3: Commit**

```bash
git add crates/vmux_layout/src/bookmark.rs
git commit -m "test(layout): cover bookmark add/toggle/remove ops"
```

### Task 4: Unit-test folders + pin/unpin composition

**Files:**
- Modify: `crates/vmux_layout/src/bookmark.rs` (extend `mod tests`)

- [ ] **Step 1: Write failing tests**

Add inside `mod tests`:

```rust
    fn folder_uuid(app: &mut App) -> String {
        app.world_mut()
            .query_filtered::<&Uuid, With<Folder>>()
            .single(app.world())
            .unwrap()
            .0
            .clone()
    }

    #[test]
    fn add_into_folder_sets_childof() {
        let mut app = test_app();
        send(&mut app, BookmarkOp::AddFolder { name: "PRs".into() });
        let fid = folder_uuid(&mut app);
        send(
            &mut app,
            BookmarkOp::Add {
                url: "https://a.test".into(),
                title: "A".into(),
                favicon_url: String::new(),
                folder: Some(fid),
            },
        );
        let has_parent = app
            .world_mut()
            .query_filtered::<&ChildOf, With<Bookmark>>()
            .iter(app.world())
            .count();
        assert_eq!(has_parent, 1);
    }

    #[test]
    fn remove_folder_reparents_children_to_top_level() {
        let mut app = test_app();
        send(&mut app, BookmarkOp::AddFolder { name: "PRs".into() });
        let fid = folder_uuid(&mut app);
        send(
            &mut app,
            BookmarkOp::Add {
                url: "https://a.test".into(),
                title: "A".into(),
                favicon_url: String::new(),
                folder: Some(fid.clone()),
            },
        );
        send(&mut app, BookmarkOp::RemoveFolder { uuid: fid });
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<Folder>>()
                .iter(app.world())
                .count(),
            0
        );
        // bookmark survived and is now top-level (no ChildOf)
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, (With<Bookmark>, Without<ChildOf>)>()
                .iter(app.world())
                .count(),
            1
        );
    }

    #[test]
    fn toggle_folder_adds_then_removes_collapsed() {
        let mut app = test_app();
        send(&mut app, BookmarkOp::AddFolder { name: "PRs".into() });
        let fid = folder_uuid(&mut app);
        send(&mut app, BookmarkOp::ToggleFolder { uuid: fid.clone() });
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<Collapsed>>()
                .iter(app.world())
                .count(),
            1
        );
        send(&mut app, BookmarkOp::ToggleFolder { uuid: fid });
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<Collapsed>>()
                .iter(app.world())
                .count(),
            0
        );
    }

    #[test]
    fn pin_promotes_bookmark_out_of_list_into_grid() {
        let mut app = test_app();
        send(
            &mut app,
            BookmarkOp::Add {
                url: "https://a.test".into(),
                title: "A".into(),
                favicon_url: String::new(),
                folder: None,
            },
        );
        let uuid = app
            .world_mut()
            .query_filtered::<&Uuid, With<Bookmark>>()
            .single(app.world())
            .unwrap()
            .0
            .clone();
        send(&mut app, BookmarkOp::Pin { uuid: uuid.clone() });
        // grid = With<Pin>
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<Pin>>()
                .iter(app.world())
                .count(),
            1
        );
        // list = With<Bookmark>, Without<Pin>  -> now empty
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, (With<Bookmark>, Without<Pin>)>()
                .iter(app.world())
                .count(),
            0
        );
        send(&mut app, BookmarkOp::Unpin { uuid });
        // returns to list
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, (With<Bookmark>, Without<Pin>)>()
                .iter(app.world())
                .count(),
            1
        );
    }
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p vmux_layout bookmark::tests`
Expected: PASS (7 tests total).

- [ ] **Step 3: Commit**

```bash
git add crates/vmux_layout/src/bookmark.rs
git commit -m "test(layout): cover bookmark folders + pin/unpin composition"
```

---

## Phase 3 — Persistence (`vmux_desktop`)

A second moonshine-save pipeline scoped by `Or<(With<Pin>, With<Bookmark>, With<Folder>)>` to `profile_dir()/bookmarks.ron`. Disjoint from `space.ron` because bookmark components do NOT carry `Save`.

### Task 5: Save/load functions + filter type alias

**Files:**
- Create: `crates/vmux_desktop/src/bookmark_persistence.rs`
- Modify: `crates/vmux_desktop/src/lib.rs` (add `mod bookmark_persistence;`)
- Modify: `crates/vmux_desktop/Cargo.toml` (ensure `vmux_core`, `moonshine-save` deps already present — they are; no change expected)

- [ ] **Step 1: Create the persistence module**

Create `crates/vmux_desktop/src/bookmark_persistence.rs`. Mirrors `persistence.rs::save_space_to_path`/`load_space_on_startup` but with a custom `SaveWorld<F>` / `LoadWorld<U>` filter (see the research: `SaveWorld::<F>::into_file`, register via `save_on::<...>`/`load_on::<...>`):

```rust
use bevy::prelude::*;
use moonshine_save::prelude::*;
use vmux_core::{Bookmark, Collapsed, Folder, Order, PageMetadata, Pin, Uuid};

type BookmarkFilter = Or<(With<Pin>, With<Bookmark>, With<Folder>)>;

pub fn bookmarks_path() -> std::path::PathBuf {
    vmux_core::profile::profile_dir().join("bookmarks.ron")
}

fn bookmark_scene_filter() -> SceneFilter {
    SceneFilter::deny_all()
        .allow::<ChildOf>()
        .allow::<Children>()
        .allow::<Name>()
        .allow::<Pin>()
        .allow::<Bookmark>()
        .allow::<Folder>()
        .allow::<Collapsed>()
        .allow::<Uuid>()
        .allow::<Order>()
        .allow::<PageMetadata>()
}

pub fn save_bookmarks(mut commands: Commands) {
    if vmux_core::profile::is_test_session() {
        return;
    }
    let path = bookmarks_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let mut save = SaveWorld::<BookmarkFilter>::into_file(path);
    save.components = bookmark_scene_filter();
    commands.trigger_save(save);
}

pub fn load_bookmarks_on_startup(mut commands: Commands) {
    if vmux_core::profile::is_test_session() {
        return;
    }
    let path = bookmarks_path();
    if !path.exists() {
        return;
    }
    commands.trigger_load(LoadWorld::<BookmarkFilter>::from_file(path));
}
```

- [ ] **Step 2: Add the module**

In `crates/vmux_desktop/src/lib.rs` add `mod bookmark_persistence;` near the other `mod` declarations.

- [ ] **Step 3: Build**

Run: `cargo build -p vmux_desktop`
Expected: compiles.

- [ ] **Step 4: Commit**

```bash
git add crates/vmux_desktop/src/bookmark_persistence.rs crates/vmux_desktop/src/lib.rs
git commit -m "feat(desktop): bookmarks save/load pipeline (profile-scoped)"
```

### Task 6: Round-trip test (save → load rebuilds entities; disjoint from space)

**Files:**
- Modify: `crates/vmux_desktop/src/bookmark_persistence.rs` (append `#[cfg(test)] mod tests`)

- [ ] **Step 1: Write the failing test**

The save/load observers must be registered for `trigger_save`/`trigger_load` to fire. Append:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_then_load_round_trips_bookmarks_and_excludes_save_entities() {
        let dir = std::env::temp_dir().join(format!("vmux-bm-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("bookmarks.ron");

        // --- save world: spawn 1 folder + 1 bookmark + 1 unrelated Save entity ---
        let mut save_app = App::new();
        save_app
            .add_plugins(MinimalPlugins)
            .add_plugins(bevy::app::ScheduleRunnerPlugin::default())
            .add_plugins(bevy::asset::AssetPlugin::default())
            .add_plugins(bevy::scene::ScenePlugin)
            .add_plugins(vmux_core::CorePlugin)
            .add_observer(save_on::<SaveWorld<BookmarkFilter>>);
        save_app
            .world_mut()
            .spawn((Folder, Uuid("f1".into()), Name::new("PRs"), Order(0)));
        save_app.world_mut().spawn((
            Bookmark,
            Uuid("b1".into()),
            PageMetadata {
                title: "A".into(),
                url: "https://a.test".into(),
                favicon_url: String::new(),
                bg_color: None,
            },
            Order(1),
        ));
        save_app.world_mut().spawn(Save); // must NOT land in bookmarks.ron
        let p = path.clone();
        save_app.add_systems(Update, move |mut c: Commands| {
            let mut s = SaveWorld::<BookmarkFilter>::into_file(p.clone());
            s.components = bookmark_scene_filter();
            c.trigger_save(s);
        });
        save_app.update();
        save_app.update();

        assert!(path.exists(), "bookmarks.ron written");
        let ron = std::fs::read_to_string(&path).unwrap();
        assert!(ron.contains("b1"), "bookmark uuid persisted");
        assert!(ron.contains("PRs"), "folder name persisted");

        // --- load world ---
        let mut load_app = App::new();
        load_app
            .add_plugins(MinimalPlugins)
            .add_plugins(bevy::asset::AssetPlugin::default())
            .add_plugins(bevy::scene::ScenePlugin)
            .add_plugins(vmux_core::CorePlugin)
            .add_observer(load_on::<LoadWorld<BookmarkFilter>>);
        let p2 = path.clone();
        load_app.add_systems(Update, move |mut c: Commands| {
            c.trigger_load(LoadWorld::<BookmarkFilter>::from_file(p2.clone()));
        });
        load_app.update();
        load_app.update();

        let bookmarks = load_app
            .world_mut()
            .query_filtered::<Entity, With<Bookmark>>()
            .iter(load_app.world())
            .count();
        let folders = load_app
            .world_mut()
            .query_filtered::<Entity, With<Folder>>()
            .iter(load_app.world())
            .count();
        assert_eq!(bookmarks, 1, "bookmark rebuilt");
        assert_eq!(folders, 1, "folder rebuilt");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
```

> NOTE for the implementer: the exact set of plugins moonshine-save needs for serialization in a test may differ slightly (it needs the type registry + scene support). If a plugin is missing, the failure message will name it — add it. Use `crates/vmux_desktop/src/persistence.rs` tests (if any) as the reference for the minimal plugin set. Do NOT add `Save` to bookmark entities.

- [ ] **Step 2: Run the test**

Run: `cargo test -p vmux_desktop save_then_load_round_trips`
Expected: initially may FAIL on a missing plugin; fix per the note, then PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/vmux_desktop/src/bookmark_persistence.rs
git commit -m "test(desktop): bookmarks save/load round-trip + Save disjointness"
```

### Task 7: Wire startup-load, save observers, and debounced autosave

**Files:**
- Modify: `crates/vmux_desktop/src/bookmark_persistence.rs` (add `BookmarkPersistencePlugin`)
- Modify: `crates/vmux_desktop/src/lib.rs` or wherever `PersistencePlugin` is added (register `BookmarkPersistencePlugin`)

- [ ] **Step 1: Add the plugin**

Append to `crates/vmux_desktop/src/bookmark_persistence.rs`:

```rust
#[derive(Resource)]
struct BookmarkAutoSave {
    debounce: Timer,
    dirty: bool,
}

impl Default for BookmarkAutoSave {
    fn default() -> Self {
        Self {
            debounce: Timer::from_seconds(0.5, TimerMode::Once),
            dirty: false,
        }
    }
}

fn mark_bookmarks_dirty(
    mut auto: ResMut<BookmarkAutoSave>,
    changed_markers: Query<
        (),
        Or<(
            Added<Pin>,
            Added<Bookmark>,
            Added<Folder>,
            Added<Collapsed>,
            Changed<Name>,
            Changed<Order>,
            Changed<PageMetadata>,
        )>,
    >,
    mut removed_pin: RemovedComponents<Pin>,
    mut removed_bookmark: RemovedComponents<Bookmark>,
    mut removed_folder: RemovedComponents<Folder>,
    mut removed_collapsed: RemovedComponents<Collapsed>,
) {
    let any_removed = removed_pin.read().next().is_some()
        | removed_bookmark.read().next().is_some()
        | removed_folder.read().next().is_some()
        | removed_collapsed.read().next().is_some();
    if any_removed || !changed_markers.is_empty() {
        auto.dirty = true;
        auto.debounce.reset();
    }
}

fn autosave_bookmarks(
    time: Res<Time>,
    mut auto: ResMut<BookmarkAutoSave>,
    commands: Commands,
) {
    if !auto.dirty {
        return;
    }
    auto.debounce.tick(time.delta());
    if auto.debounce.is_finished() {
        save_bookmarks(commands);
        auto.dirty = false;
    }
}

pub struct BookmarkPersistencePlugin;

impl Plugin for BookmarkPersistencePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BookmarkAutoSave>()
            .add_observer(save_on::<SaveWorld<BookmarkFilter>>)
            .add_observer(load_on::<LoadWorld<BookmarkFilter>>)
            .add_systems(Startup, load_bookmarks_on_startup)
            .add_systems(Update, (mark_bookmarks_dirty, autosave_bookmarks).chain());
    }
}
```

> NOTE: if `Timer::is_finished()` is named `finished()` in this Bevy version, match `persistence.rs`'s `auto_save_system` usage verbatim. Register `load_bookmarks_on_startup` in the same startup ordering set the space loader uses if load order matters (`LayoutStartupSet::Persistence`), but a plain `Startup` is acceptable since the sets are disjoint.

- [ ] **Step 2: Register the plugin**

Where `PersistencePlugin` (space) is added in `vmux_desktop`, add `BookmarkPersistencePlugin` alongside it.

- [ ] **Step 3: Build**

Run: `cargo build -p vmux_desktop`
Expected: compiles.

- [ ] **Step 4: Commit**

```bash
git add crates/vmux_desktop/src/bookmark_persistence.rs crates/vmux_desktop/src/lib.rs
git commit -m "feat(desktop): bookmarks startup-load + debounced autosave"
```

---

## Phase 4 — Snapshot broadcast + page command bus (`vmux_layout` + `vmux_browser`)

### Task 8: Event types + DTO (`vmux_layout/src/event.rs`)

**Files:**
- Modify: `crates/vmux_layout/src/event.rs` (add constant + types near `TABS_EVENT`/`TabRow` ~line 23/339)

- [ ] **Step 1: Add the event constant + DTO + command event**

Append to `crates/vmux_layout/src/event.rs` (mirror the `TabsHostEvent`/`TabsCommandEvent` derives exactly):

```rust
pub const BOOKMARKS_EVENT: &str = "bookmarks";

#[derive(
    Clone, Debug, Default, PartialEq, Eq,
    serde::Serialize, serde::Deserialize,
    rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub struct BookmarkRow {
    pub uuid: String,
    pub url: String,
    pub title: String,
    pub favicon_url: String,
}

#[derive(
    Clone, Debug, PartialEq, Eq,
    serde::Serialize, serde::Deserialize,
    rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub struct FolderRow {
    pub uuid: String,
    pub name: String,
    pub collapsed: bool,
    pub children: Vec<BookmarkRow>,
}

#[derive(
    Clone, Debug, PartialEq, Eq,
    serde::Serialize, serde::Deserialize,
    rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub enum BookmarkNode {
    Entry(BookmarkRow),
    Folder(FolderRow),
}

#[derive(
    Clone, Debug, Default, PartialEq, Eq,
    serde::Serialize, serde::Deserialize,
    rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub struct BookmarksHostEvent {
    pub pins: Vec<BookmarkRow>,
    pub roots: Vec<BookmarkNode>,
}

#[derive(
    Clone, Debug,
    serde::Serialize, serde::Deserialize,
    rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub struct BookmarksCommandEvent {
    pub command: String,
    #[serde(default)]
    pub uuid: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub favicon_url: Option<String>,
}
```

- [ ] **Step 2: Build**

Run: `cargo build -p vmux_layout`
Expected: compiles.

- [ ] **Step 3: Commit**

```bash
git add crates/vmux_layout/src/event.rs
git commit -m "feat(layout): add bookmarks rkyv event/DTO types"
```

### Task 9: Host observer — translate `BookmarksCommandEvent` → `BookmarkOp` / open

**Files:**
- Modify: `crates/vmux_layout/src/bookmark.rs` (add observer + register in `BookmarkPlugin`)

- [ ] **Step 1: Add the observer**

In `crates/vmux_layout/src/bookmark.rs`, add imports and the observer. The "open" command routes to a browser open `AppCommand` (mirror `on_tabs_command_emit`'s `messages.write(cmd)` pattern):

```rust
use crate::event::BookmarksCommandEvent;
use bevy_cef::prelude::{BinEventEmitterPlugin, BinReceive};
use vmux_command::{AppCommand, BrowserCommand, OpenCommand};

fn on_bookmarks_command_emit(
    trigger: On<BinReceive<BookmarksCommandEvent>>,
    mut ops: MessageWriter<BookmarkOp>,
    mut app_cmds: MessageWriter<AppCommand>,
) {
    let e = &trigger.event().payload;
    match e.command.as_str() {
        "toggle_active" => { /* handled by the Cmd+D adapter path; ignore here */ }
        "open" => {
            if let Some(url) = e.url.clone() {
                app_cmds.write(AppCommand::Browser(BrowserCommand::Open(
                    OpenCommand::InNewTab { url: Some(url) },
                )));
            }
        }
        "remove" => {
            if let Some(uuid) = e.uuid.clone() {
                ops.write(BookmarkOp::Remove { uuid });
            }
        }
        "pin" => {
            if let Some(uuid) = e.uuid.clone() {
                ops.write(BookmarkOp::Pin { uuid });
            }
        }
        "unpin" => {
            if let Some(uuid) = e.uuid.clone() {
                ops.write(BookmarkOp::Unpin { uuid });
            }
        }
        "toggle_folder" => {
            if let Some(uuid) = e.uuid.clone() {
                ops.write(BookmarkOp::ToggleFolder { uuid });
            }
        }
        "new_folder" => {
            if let Some(name) = e.name.clone() {
                ops.write(BookmarkOp::AddFolder { name });
            }
        }
        "rename_folder" => {
            if let (Some(uuid), Some(name)) = (e.uuid.clone(), e.name.clone()) {
                ops.write(BookmarkOp::RenameFolder { uuid, name });
            }
        }
        "remove_folder" => {
            if let Some(uuid) = e.uuid.clone() {
                ops.write(BookmarkOp::RemoveFolder { uuid });
            }
        }
        "add" => {
            if let Some(url) = e.url.clone() {
                ops.write(BookmarkOp::Add {
                    url,
                    title: e.title.clone().unwrap_or_default(),
                    favicon_url: e.favicon_url.clone().unwrap_or_default(),
                    folder: e.uuid.clone(),
                });
            }
        }
        _ => {}
    }
}
```

- [ ] **Step 2: Register in `BookmarkPlugin`**

Update `BookmarkPlugin::build` to add the bin-event emitter plugin + observer (mirror `TabPlugin`):

```rust
        app.add_message::<BookmarkOp>()
            .add_plugins(BinEventEmitterPlugin::<(BookmarksCommandEvent,)>::for_hosts(&["layout"]))
            .add_observer(on_bookmarks_command_emit)
            .add_systems(Update, apply_bookmark_ops);
```

- [ ] **Step 3: Build**

Run: `cargo build -p vmux_layout`
Expected: compiles.

- [ ] **Step 4: Commit**

```bash
git add crates/vmux_layout/src/bookmark.rs
git commit -m "feat(layout): translate BookmarksCommandEvent to BookmarkOp"
```

### Task 10: Broadcast system (`vmux_browser`)

**Files:**
- Modify: `crates/vmux_browser/src/lib.rs` (add `push_bookmarks_host_emit`; add it to the `push_*` tuple ~line 157-168)

- [ ] **Step 1: Add the broadcast system**

In `crates/vmux_browser/src/lib.rs`, add (mirror `push_tabs_host_emit`'s `LayoutCef`/`PageReady`/`Local<String>` RON-dedup + `commands.trigger(BinHostEmitEvent::from_rkyv(...))` shape):

```rust
fn push_bookmarks_host_emit(
    mut commands: Commands,
    browsers: NonSend<Browsers>,
    cef_q: Query<(Entity, Ref<PageReady>), With<LayoutCef>>,
    pins: Query<(&vmux_core::Uuid, &PageMetadata), With<vmux_core::Pin>>,
    folders: Query<
        (Entity, &vmux_core::Uuid, &Name, Option<&Children>, Has<vmux_core::Collapsed>, &vmux_core::Order),
        With<vmux_core::Folder>,
    >,
    top_bookmarks: Query<
        (&vmux_core::Uuid, &PageMetadata, &vmux_core::Order),
        (With<vmux_core::Bookmark>, Without<vmux_core::Pin>, Without<ChildOf>),
    >,
    child_bookmarks: Query<
        (&vmux_core::Uuid, &PageMetadata),
        (With<vmux_core::Bookmark>, Without<vmux_core::Pin>),
    >,
    mut last: Local<String>,
) {
    let Ok((cef_e, page_ready)) = cef_q.single() else {
        return;
    };
    if !browsers.has_browser(cef_e) || !browsers.host_emit_ready(&cef_e) {
        return;
    }

    let row = |uuid: &vmux_core::Uuid, meta: &PageMetadata| vmux_layout::event::BookmarkRow {
        uuid: uuid.0.clone(),
        url: meta.url.clone(),
        title: meta.title.clone(),
        favicon_url: meta.favicon_url.clone(),
    };

    let pin_rows: Vec<_> = pins.iter().map(|(u, m)| row(u, m)).collect();

    // ordered roots = folders + loose bookmarks, sorted by Order
    let mut roots: Vec<(u32, vmux_layout::event::BookmarkNode)> = Vec::new();
    for (entity, uuid, name, children, collapsed, order) in folders.iter() {
        let mut kids: Vec<vmux_layout::event::BookmarkRow> = Vec::new();
        if let Some(children) = children {
            for child in children.iter() {
                if let Ok((u, m)) = child_bookmarks.get(child) {
                    kids.push(row(u, m));
                }
            }
        }
        roots.push((
            order.0,
            vmux_layout::event::BookmarkNode::Folder(vmux_layout::event::FolderRow {
                uuid: uuid.0.clone(),
                name: name.as_str().to_string(),
                collapsed,
                children: kids,
            }),
        ));
    }
    for (uuid, meta, order) in top_bookmarks.iter() {
        roots.push((
            order.0,
            vmux_layout::event::BookmarkNode::Entry(row(uuid, meta)),
        ));
    }
    roots.sort_by_key(|(o, _)| *o);
    let roots: Vec<_> = roots.into_iter().map(|(_, n)| n).collect();

    let payload = vmux_layout::event::BookmarksHostEvent {
        pins: pin_rows,
        roots,
    };
    let body = ron::ser::to_string(&payload).unwrap_or_default();
    if !page_ready.is_changed() && body == *last {
        return;
    }
    commands.trigger(BinHostEmitEvent::from_rkyv(
        cef_e,
        vmux_layout::event::BOOKMARKS_EVENT,
        &payload,
    ));
    *last = body;
}
```

> NOTE: import `Has`, `ChildOf`, `Children`, `Name`, `PageMetadata` as already used in `lib.rs`. If `PageMetadata` is imported unqualified there, reuse it; otherwise use `vmux_core::PageMetadata`.

- [ ] **Step 2: Register in the `push_*` tuple**

Add `push_bookmarks_host_emit` to the tuple at `crates/vmux_browser/src/lib.rs:157-168` (next to `push_tabs_host_emit`).

- [ ] **Step 3: Build**

Run: `cargo build -p vmux_browser`
Expected: compiles.

- [ ] **Step 4: Commit**

```bash
git add crates/vmux_browser/src/lib.rs
git commit -m "feat(browser): broadcast bookmarks snapshot to layout page"
```

---

## Phase 5 — Side-sheet UI (`vmux_layout/src/page.rs`)

The vertical left chrome is `SideSheetView` (scrollable column at `page.rs:614`). Inject a pins grid + bookmarks tree as the FIRST child, above the existing space/pane content. WASM UI isn't unit-tested here; verify by build + manual test in the final phase (per the project's "implement all, test once at the end" workflow).

> Reminder (source-scrape tests): refactoring `page.rs`/`command_bar` markup can break `include_str!` text-assert tests in `style.rs` / `tests/page_source.rs`. Run `cargo test -p vmux_layout` after UI tasks and update any text assertions.

### Task 11: Subscribe to `BookmarksHostEvent` + thread into `SideSheetView`

**Files:**
- Modify: `crates/vmux_layout/src/page.rs` (`Page()` listeners ~line 16-159; `SideSheetView` ~line 607-648)

- [ ] **Step 1: Add the listener signal in `Page()`**

Mirror the tabs listener (`page.rs:34-39`). Near the other `use_bin_event_listener` calls add:

```rust
    let mut bookmarks_state = use_signal(crate::event::BookmarksHostEvent::default);
    let _bookmarks_listener = use_bin_event_listener::<crate::event::BookmarksHostEvent, _>(
        crate::event::BOOKMARKS_EVENT,
        move |data| {
            bookmarks_state.set(data);
        },
    );
```

- [ ] **Step 2: Pass it to `SideSheetView`**

Where `SideSheetView { ... }` is invoked in `Page()`, add a prop `bookmarks: bookmarks_state()`. Add the matching field to the `SideSheetView` component signature (it is a `#[component] fn SideSheetView(...)`). Import the DTO types: `use crate::event::{BookmarkNode, BookmarkRow, BookmarksHostEvent, BookmarksCommandEvent};`.

- [ ] **Step 3: Build**

Run: `cargo build -p vmux_layout`
Expected: compiles (the prop is unused until Task 12 — that's fine).

- [ ] **Step 4: Commit**

```bash
git add crates/vmux_layout/src/page.rs
git commit -m "feat(layout): subscribe to bookmarks snapshot in layout page"
```

### Task 12: Render pins grid + bookmark tree + divider

**Files:**
- Modify: `crates/vmux_layout/src/page.rs` (add `BookmarksSection`, `PinGrid`, `BookmarkEntry`, `BookmarkFolder` components; render `BookmarksSection` as first child of `SideSheetView`'s scroll column ~line 614)

- [ ] **Step 1: Add the components**

Add to `crates/vmux_layout/src/page.rs` (favicon usage mirrors `vmux_history/src/page.rs:150-155`; emit pattern mirrors `try_cef_bin_emit_rkyv`):

```rust
#[component]
fn BookmarksSection(bookmarks: BookmarksHostEvent) -> Element {
    let BookmarksHostEvent { pins, roots } = bookmarks;
    let has_any = !pins.is_empty() || !roots.is_empty();
    rsx! {
        div { class: "flex flex-col gap-2 px-1 pb-2",
            if !pins.is_empty() {
                div { class: "grid grid-cols-3 gap-2",
                    for p in pins.iter() {
                        PinTile { key: "{p.uuid}", row: p.clone() }
                    }
                }
            }
            for node in roots.iter() {
                match node {
                    BookmarkNode::Folder(f) => rsx! { BookmarkFolder { key: "{f.uuid}", folder: f.clone() } },
                    BookmarkNode::Entry(b) => rsx! { BookmarkEntry { key: "{b.uuid}", row: b.clone() } },
                }
            }
            if has_any {
                div { class: "mt-1 h-px w-full bg-border" }
            }
        }
    }
}

fn open_bookmark(url: String) {
    let _ = try_cef_bin_emit_rkyv(&BookmarksCommandEvent {
        command: "open".into(),
        url: Some(url),
        uuid: None,
        name: None,
        title: None,
        favicon_url: None,
    });
}

fn bookmark_cmd(command: &str, uuid: Option<String>) {
    let _ = try_cef_bin_emit_rkyv(&BookmarksCommandEvent {
        command: command.into(),
        uuid,
        name: None,
        url: None,
        title: None,
        favicon_url: None,
    });
}

#[component]
fn PinTile(row: BookmarkRow) -> Element {
    let url = row.url.clone();
    let uuid = row.uuid.clone();
    rsx! {
        div {
            class: "flex aspect-square cursor-pointer items-center justify-center rounded-lg bg-glass-hover hover:bg-foreground/10",
            onclick: move |_| open_bookmark(url.clone()),
            Favicon {
                favicon_url: row.favicon_url.clone(),
                url: row.url.clone(),
                class: "h-6 w-6 shrink-0 rounded-sm object-contain".to_string(),
                globe_class: "h-6 w-6 shrink-0 text-muted-foreground".to_string(),
            }
        }
        // context menu wired in Task 13 (Unpin/Open)
    }
}

#[component]
fn BookmarkEntry(row: BookmarkRow) -> Element {
    let url = row.url.clone();
    let title = if row.title.is_empty() { row.url.clone() } else { row.title.clone() };
    rsx! {
        div {
            class: "group flex cursor-pointer items-center gap-2 rounded-md px-2 py-1 text-ui text-muted-foreground hover:bg-glass-hover hover:text-foreground",
            onclick: move |_| open_bookmark(url.clone()),
            Favicon {
                favicon_url: row.favicon_url.clone(),
                url: row.url.clone(),
                class: "h-4 w-4 shrink-0 rounded-sm object-contain".to_string(),
                globe_class: "h-4 w-4 shrink-0 text-muted-foreground".to_string(),
            }
            span { class: "min-w-0 flex-1 truncate", "{title}" }
        }
    }
}

#[component]
fn BookmarkFolder(folder: crate::event::FolderRow) -> Element {
    let uuid_toggle = folder.uuid.clone();
    rsx! {
        div { class: "flex flex-col",
            div {
                class: "flex cursor-pointer items-center gap-1.5 rounded-md px-2 py-1 text-ui font-medium text-foreground hover:bg-glass-hover",
                onclick: move |_| bookmark_cmd("toggle_folder", Some(uuid_toggle.clone())),
                Icon { class: "h-3.5 w-3.5 shrink-0 text-muted-foreground transition-transform",
                    path { d: if folder.collapsed { "m9 18 6-6-6-6" } else { "m6 9 6 6 6-6" } }
                }
                span { class: "min-w-0 flex-1 truncate", "{folder.name}" }
            }
            if !folder.collapsed {
                div { class: "ml-3 flex flex-col",
                    for b in folder.children.iter() {
                        BookmarkEntry { key: "{b.uuid}", row: b.clone() }
                    }
                }
            }
        }
    }
}
```

- [ ] **Step 2: Render `BookmarksSection` in `SideSheetView`**

As the first child of the scroll column at `page.rs:614` (`div { class: "flex min-h-0 flex-1 flex-col overflow-y-auto ..." }`), add:

```rust
                BookmarksSection { bookmarks: bookmarks.clone() }
```

(`bookmarks` is the prop added in Task 11.)

- [ ] **Step 3: Build + check source-scrape tests**

Run: `cargo build -p vmux_layout && cargo test -p vmux_layout`
Expected: compiles; if any `page_source` text test fails, update it to include the new markup.

- [ ] **Step 4: Commit**

```bash
git add crates/vmux_layout/src/page.rs
git commit -m "feat(layout): render pins grid + bookmark folders/entries"
```

### Task 13: Right-click context menus (pin / entry / folder / tab)

**Files:**
- Modify: `crates/vmux_layout/src/page.rs` (wrap `PinTile`/`BookmarkEntry`/`BookmarkFolder` content + the existing `Tab` component ~line 404 in `ContextMenu`)

`ContextMenu` has no prior page usage (only a gallery demo). Use `dioxus_primitives::context_menu` directly to avoid the demo styling on `ContextMenuTrigger`. `ContextMenuItem` requires `index: usize`, `value: ReadSignal<String>`, `on_select`.

- [ ] **Step 1: Import**

Add: `use dioxus_primitives::context_menu::{ContextMenu, ContextMenuContent, ContextMenuItem, ContextMenuTrigger};`

- [ ] **Step 2: Wrap the bookmark entry with a menu (Open / Pin / Remove)**

Replace `BookmarkEntry`'s outer `div { ... }` with a `ContextMenu` wrapping a `ContextMenuTrigger` (the clickable row) + `ContextMenuContent` (Open/Pin/Remove items). Concrete shape:

```rust
#[component]
fn BookmarkEntry(row: BookmarkRow) -> Element {
    let url_open = row.url.clone();
    let uuid_pin = row.uuid.clone();
    let uuid_remove = row.uuid.clone();
    let title = if row.title.is_empty() { row.url.clone() } else { row.title.clone() };
    let menu_val = use_signal(|| row.uuid.clone());
    rsx! {
        ContextMenu { attributes: vec![],
            ContextMenuTrigger { attributes: vec![],
                div {
                    class: "group flex cursor-pointer items-center gap-2 rounded-md px-2 py-1 text-ui text-muted-foreground hover:bg-glass-hover hover:text-foreground",
                    onclick: {
                        let u = url_open.clone();
                        move |_| open_bookmark(u.clone())
                    },
                    Favicon {
                        favicon_url: row.favicon_url.clone(),
                        url: row.url.clone(),
                        class: "h-4 w-4 shrink-0 rounded-sm object-contain".to_string(),
                        globe_class: "h-4 w-4 shrink-0 text-muted-foreground".to_string(),
                    }
                    span { class: "min-w-0 flex-1 truncate", "{title}" }
                }
            }
            ContextMenuContent { attributes: vec![],
                ContextMenuItem {
                    index: 0usize, value: menu_val, attributes: vec![],
                    on_select: { let u = url_open.clone(); move |_: String| open_bookmark(u.clone()) },
                    "Open"
                }
                ContextMenuItem {
                    index: 1usize, value: menu_val, attributes: vec![],
                    on_select: { let id = uuid_pin.clone(); move |_: String| bookmark_cmd("pin", Some(id.clone())) },
                    "Pin"
                }
                ContextMenuItem {
                    index: 2usize, value: menu_val, attributes: vec![],
                    on_select: { let id = uuid_remove.clone(); move |_: String| bookmark_cmd("remove", Some(id.clone())) },
                    "Remove"
                }
            }
        }
    }
}
```

- [ ] **Step 3: Pin tile menu (Open / Unpin)**

Wrap `PinTile`'s tile `div` the same way with items: Open (`open_bookmark`) and Unpin (`bookmark_cmd("unpin", Some(uuid))`).

- [ ] **Step 4: Folder header menu (Rename / Remove / Collapse)**

Wrap `BookmarkFolder`'s header `div` with items: Collapse/Expand (`bookmark_cmd("toggle_folder", ...)`), Remove (`bookmark_cmd("remove_folder", ...)`). Rename needs text input — defer the rename UI to a follow-up; for now include a Remove + Collapse menu (rename is reachable via MCP/the separate plan). Add a "New folder" affordance: a small `+` button at the top of `BookmarksSection` that emits `BookmarksCommandEvent { command: "new_folder", name: Some("New Folder".into()), .. }`.

- [ ] **Step 5: Tab menu (Pin / Bookmark)**

Wrap the `Tab` component's outer `div` (`page.rs:451`) in a `ContextMenu`. Items emit from the tab's own `tab.url`/`tab.title`/`tab.favicon_url` (available in the `Tab` props):

```rust
                ContextMenuItem {
                    index: 0usize, value: menu_val, attributes: vec![],
                    on_select: {
                        let (u, t, f) = (tab.url.clone(), tab.title.clone(), tab.favicon_url.clone());
                        move |_: String| {
                            let _ = try_cef_bin_emit_rkyv(&BookmarksCommandEvent {
                                command: "add".into(), url: Some(u.clone()),
                                title: Some(t.clone()), favicon_url: Some(f.clone()),
                                uuid: None, name: None,
                            });
                        }
                    },
                    "Bookmark"
                }
                ContextMenuItem {
                    index: 1usize, value: menu_val, attributes: vec![],
                    on_select: {
                        let (u, t, f) = (tab.url.clone(), tab.title.clone(), tab.favicon_url.clone());
                        move |_: String| {
                            let _ = try_cef_bin_emit_rkyv(&BookmarksCommandEvent {
                                command: "add".into(), url: Some(u.clone()),
                                title: Some(t.clone()), favicon_url: Some(f.clone()),
                                uuid: None, name: None,
                            });
                            // pin variant: emit a "pin_url" — add to BookmarksCommandEvent handling
                        }
                    },
                    "Pin"
                }
```

> NOTE: for "Pin" from a tab (a page that isn't yet a bookmark), add a `"pin_url"` arm to `on_bookmarks_command_emit` (Task 9) that emits `BookmarkOp::PinUrl { url, title, favicon_url }`, and emit `command: "pin_url"` with url/title/favicon here. Keep "Bookmark" → `"add"`.

- [ ] **Step 6: Build + tests**

Run: `cargo build -p vmux_layout && cargo test -p vmux_layout`
Expected: compiles; update source-scrape tests if needed.

- [ ] **Step 7: Commit**

```bash
git add crates/vmux_layout/src/page.rs crates/vmux_layout/src/bookmark.rs
git commit -m "feat(layout): context menus for pins/bookmarks/folders/tabs"
```

---

## Phase 6 — Command-bar bookmark icon (`vmux_layout/src/command_bar/page.rs`)

### Task 14: Add the bookmark/label icon button (emits `toggle_active`)

**Files:**
- Modify: `crates/vmux_layout/src/command_bar/page.rs` (insert a button as the last child of the input row `div`, after the input-wrap closes ~line 473, before the row `div` closes ~line 474)

The command bar has no current-page state, so the button emits a `toggle_active` command; the host resolves the active tab (Phase 7 adapter handles `toggle_active` too). Filled/outline state is a follow-up (requires threading current url + is_bookmarked onto `CommandBarOpenEvent`).

- [ ] **Step 1: Add the button**

After the input-wrap `div` (closes ~`page.rs:473`), still inside the `command_bar_input_row_class()` row, add (label/ribbon glyph; `Icon` + `try_cef_bin_emit_rkyv` already imported in this file at `page.rs:23-25`):

```rust
                        button {
                            r#type: "button",
                            aria_label: "Bookmark this page",
                            title: "Bookmark this page (⌘D)",
                            class: "flex h-7 w-7 shrink-0 items-center justify-center rounded-md text-muted-foreground hover:bg-foreground/10 hover:text-foreground",
                            onmousedown: move |e| { e.prevent_default(); e.stop_propagation(); },
                            onclick: move |e| {
                                e.prevent_default();
                                e.stop_propagation();
                                let _ = try_cef_bin_emit_rkyv(&crate::event::BookmarksCommandEvent {
                                    command: "toggle_active".into(),
                                    uuid: None, name: None, url: None, title: None, favicon_url: None,
                                });
                            },
                            Icon { class: "h-4 w-4",
                                path { d: "M19 21l-7-5-7 5V5a2 2 0 0 1 2-2h10a2 2 0 0 1 2 2z" }
                            }
                        }
```

- [ ] **Step 2: Build + tests**

Run: `cargo build -p vmux_layout && cargo test -p vmux_layout`
Expected: compiles; update `command_bar` source-scrape tests in `style.rs`/`tests/page_source.rs` if they assert the row markup.

- [ ] **Step 3: Commit**

```bash
git add crates/vmux_layout/src/command_bar/page.rs
git commit -m "feat(layout): command-bar bookmark icon (toggle active page)"
```

---

## Phase 7 — Cmd+D shortcut + active-tab adapter

### Task 15: Add `AppCommand::Bookmark(BookmarkCommand)` + `Super+d`

**Files:**
- Modify: `crates/vmux_command/src/command.rs` (add the leaf enum + `Bookmark(BookmarkCommand)` group on `AppCommand` ~line 17-33; add a default-shortcuts test ~line 413)

- [ ] **Step 1: Write the failing shortcut test**

Add to the `#[cfg(test)] mod tests` in `crates/vmux_command/src/command.rs` (mirror `menu_accelerators_are_registered_as_global_shortcuts`):

```rust
    #[test]
    fn cmd_d_is_a_global_bookmark_shortcut() {
        let shortcuts = AppCommand::default_shortcuts();
        let has_super = |k: KeyCode| {
            shortcuts.iter().any(|(s, _)| {
                matches!(s, Shortcut::Direct(c) if c.key == k && c.modifiers.super_key
                    && !c.modifiers.shift && !c.modifiers.ctrl && !c.modifiers.alt)
            })
        };
        assert!(has_super(KeyCode::KeyD), "cmd+D must be a global shortcut");
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p vmux_command cmd_d_is_a_global_bookmark_shortcut`
Expected: FAIL (no Super+d binding yet).

- [ ] **Step 3: Add the leaf enum + group**

In `crates/vmux_command/src/command.rs`, add the leaf enum (mirror `ServiceCommand`/`BrowserBarCommand`; NOT deriving `McpTool` so it stays out of MCP — MCP uses dedicated tools in the MCP plan):

```rust
#[allow(dead_code)]
#[derive(OsSubMenu, DefaultShortcuts, CommandBar, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BookmarkCommand {
    #[default]
    #[menu(id = "bookmark_toggle_active", label = "Bookmark Page", accel = "super+d")]
    #[shortcut(direct = "Super+d")]
    ToggleActive,
    #[menu(id = "bookmark_pin_active", label = "Pin Page")]
    PinActive,
}
```

Add the group variant to `AppCommand` (after `Service(ServiceCommand)` ~line 32), with `#[mcp(skip)]` (like `Layout`):

```rust
    #[menu(label = "Bookmark")]
    #[mcp(skip)]
    Bookmark(BookmarkCommand),
```

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -p vmux_command cmd_d_is_a_global_bookmark_shortcut`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_command/src/command.rs
git commit -m "feat(command): add Bookmark(ToggleActive/PinActive) + cmd+d"
```

### Task 16: Active-tab adapter (`AppCommand::Bookmark` → `BookmarkOp`)

**Files:**
- Modify: `crates/vmux_layout/src/bookmark.rs` (add `handle_bookmark_app_commands` system + register; query the active stack's `PageMetadata`)

- [ ] **Step 1: Add the adapter system**

Uses `ActiveTabParam` + `focused_stack` (research §4) to read the active stack's `PageMetadata`, then emits `ToggleForUrl`/`PinUrl`:

```rust
use crate::stack::{focused_stack, ActiveTabParam};
use crate::pane::{Pane, PaneSplit};
use crate::stack::Stack;
use vmux_core::LastActivatedAt;
use vmux_command::{BookmarkCommand, ReadAppCommands};

#[allow(clippy::too_many_arguments)]
fn handle_bookmark_app_commands(
    mut reader: MessageReader<AppCommand>,
    active_tab_param: ActiveTabParam,
    all_children: Query<&Children>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_ts: Query<(Entity, &LastActivatedAt), With<Pane>>,
    pane_children: Query<&Children, With<Pane>>,
    stack_ts: Query<(Entity, &LastActivatedAt), With<Stack>>,
    stack_meta: Query<&PageMetadata, With<Stack>>,
    mut ops: MessageWriter<BookmarkOp>,
) {
    for cmd in reader.read() {
        let which = match cmd {
            AppCommand::Bookmark(BookmarkCommand::ToggleActive) => 0,
            AppCommand::Bookmark(BookmarkCommand::PinActive) => 1,
            _ => continue,
        };
        let (_, _, stack) = focused_stack(
            active_tab_param.get(),
            &all_children,
            &leaf_panes,
            &pane_ts,
            &pane_children,
            &stack_ts,
        );
        let Some(stack) = stack else { continue };
        let Ok(meta) = stack_meta.get(stack) else { continue };
        if meta.url.is_empty() {
            continue;
        }
        let (url, title, favicon_url) =
            (meta.url.clone(), meta.title.clone(), meta.favicon_url.clone());
        if which == 0 {
            ops.write(BookmarkOp::ToggleForUrl { url, title, favicon_url });
        } else {
            ops.write(BookmarkOp::PinUrl { url, title, favicon_url });
        }
    }
}
```

Also handle `"toggle_active"` from the command-bar icon: in `on_bookmarks_command_emit` (Task 9), the `"toggle_active"` arm currently does nothing. The command bar can't read the active stack, but the host can — route it through the same adapter by having the `"toggle_active"` arm write a synthetic `AppCommand::Bookmark(BookmarkCommand::ToggleActive)` via a `MessageWriter<AppCommand>`. Update Task 9's observer `"toggle_active"` arm to:

```rust
        "toggle_active" => {
            app_cmds.write(AppCommand::Bookmark(
                vmux_command::BookmarkCommand::ToggleActive,
            ));
        }
```

- [ ] **Step 2: Register the system**

In `BookmarkPlugin::build`, add `.add_systems(Update, handle_bookmark_app_commands.in_set(ReadAppCommands))`.

- [ ] **Step 3: Build**

Run: `cargo build -p vmux_layout`
Expected: compiles.

- [ ] **Step 4: Commit**

```bash
git add crates/vmux_layout/src/bookmark.rs
git commit -m "feat(layout): cmd+d/command-bar toggle bookmark on active tab"
```

---

## Phase 8 — Verification

### Task 17: Workspace checks + manual smoke test

- [ ] **Step 1: Format + clippy + tests**

Run:
```bash
cargo fmt
git checkout -- patches/   # cargo fmt may touch vendored patches; keep only crates/ changes
cargo clippy -p vmux_core -p vmux_command -p vmux_layout -p vmux_browser -p vmux_desktop --all-targets
cargo test -p vmux_core -p vmux_command -p vmux_layout -p vmux_desktop
```
Expected: clean.

- [ ] **Step 2: Manual smoke test (user-run)**

Launch vmux. Verify:
1. Cmd+D on an active browser tab → a bookmark entry appears in the left chrome above the divider; Cmd+D again removes it.
2. Command-bar bookmark icon toggles the same.
3. Right-click a bookmark entry → Open / Pin / Remove work; Pin moves it into the favicon grid; right-click pin → Unpin returns it to the list.
4. Right-click a tab → Bookmark / Pin work.
5. New Folder; right-click folder → Collapse/Expand, Remove (children re-parent to top level).
6. Restart vmux → pins/bookmarks/folders persist (per-profile `bookmarks.ron`).

- [ ] **Step 3: Open the PR** (use the `open-new-pr` skill).

---

## Self-Review (run before execution)

- **Spec coverage:** pins grid (Task 12) ✓; bookmarks entries + folders (12) ✓; collapsible (12, `Collapsed`) ✓; per-profile persistence (5–7) ✓; command-bar icon (14) ✓; Cmd+D (15–16) ✓; tab right-click Pin/Bookmark (13) ✓; context menus (13) ✓; composition markers (1) ✓. MCP is the separate plan.
- **Deferred/cut for v1 (matches spec non-goals):** folder rename UI (reachable via MCP plan), drag reorder, `Move` op, nested folders.
- **Type consistency:** `BookmarkOp` variants used in Tasks 3/4/9/16 match Task 2; DTO names (`BookmarkRow`/`FolderRow`/`BookmarkNode`/`BookmarksHostEvent`/`BookmarksCommandEvent`) consistent across Tasks 8/10/11/12/13/14; `Uuid`/`Pin`/`Bookmark`/`Folder`/`Collapsed` consistent with Task 1.
- **Known implementer adjustments (flagged inline):** exact moonshine test plugin set (Task 6); `Timer::is_finished`/`finished` naming (Task 7); `PageMetadata` import qualification (Task 10); `"pin_url"` arm wiring (Task 13 ↔ Task 9).

