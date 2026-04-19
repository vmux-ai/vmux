# Persistent Session Implementation Plan — Part 2

> Continuation of `docs/superpowers/plans/2026-04-19-persistent-session.md` (Tasks 1-4).
> Systems query `LastActivatedAt` directly (no FocusState resource). See Part 1 for helpers: `active_among()`, `focused_tab()`, `active_tab_in_pane()`, `active_pane_in_space()`.

---

### Task 5: PageMetadata — Reflect + sync from Browser to Tab

**Files:**
- Modify: `crates/vmux_header/Cargo.toml`
- Modify: `crates/vmux_header/src/system.rs`
- Modify: `crates/vmux_desktop/src/browser.rs`
- Modify: `crates/vmux_desktop/src/layout/tab.rs`
- Modify: `crates/vmux_desktop/src/lib.rs`

- [ ] **Step 1: Add moonshine-save to vmux_header (native only)**

In `crates/vmux_header/Cargo.toml`, add to native deps:

```toml
moonshine-save = { workspace = true }
```

- [ ] **Step 2: Add Reflect + Save to PageMetadata**

In `crates/vmux_header/src/system.rs`:

```rust
use moonshine_save::prelude::*;

#[derive(Component, Clone, Debug, Reflect, Default)]
#[reflect(Component)]
#[require(Save)]
pub struct PageMetadata {
    pub title: String,
    pub url: String,
    pub favicon_url: String,
}
```

- [ ] **Step 3: Register PageMetadata type**

In `crates/vmux_desktop/src/lib.rs`, add to type registrations:

```rust
.register_type::<vmux_header::PageMetadata>()
```

- [ ] **Step 4: Add PageMetadata to tab_bundle**

In `crates/vmux_desktop/src/layout/tab.rs`, update `tab_bundle()` to include a default PageMetadata:

```rust
pub(crate) fn tab_bundle() -> impl Bundle {
    (
        Tab::default(),
        vmux_header::PageMetadata::default(),
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
        ZIndex(0),
    )
}
```

- [ ] **Step 5: Add sync_page_metadata_to_tab system**

In `crates/vmux_desktop/src/browser.rs`, add a system that copies PageMetadata from Browser child to parent Tab:

```rust
fn sync_page_metadata_to_tab(
    browser_q: Query<(&PageMetadata, &ChildOf), (With<Browser>, Changed<PageMetadata>)>,
    tab_q: Query<(), With<Tab>>,
    status_q: Query<(), With<Header>>,
    side_sheet_q: Query<(), With<SideSheet>>,
    mut commands: Commands,
) {
    for (meta, child_of) in &browser_q {
        let parent = child_of.get();
        if !tab_q.contains(parent) || status_q.contains(parent) || side_sheet_q.contains(parent) {
            continue;
        }
        commands.entity(parent).insert(meta.clone());
    }
}
```

Register in `BrowserPlugin::build()`:

```rust
        .add_systems(
            Update,
            sync_page_metadata_to_tab
                .after(vmux_header::system::apply_chrome_state_from_cef),
        )
```

- [ ] **Step 6: Build, verify, commit**

```bash
cargo build -p vmux_desktop
git add -A && git commit -m "feat: add Reflect to PageMetadata, sync from Browser to Tab"
```

---

### Task 6: Save pipeline

**Files:**
- Create: `crates/vmux_desktop/src/persistence.rs`
- Modify: `crates/vmux_desktop/src/lib.rs`

- [ ] **Step 1: Create persistence.rs**

Create `crates/vmux_desktop/src/persistence.rs`:

```rust
use bevy::prelude::*;
use moonshine_save::prelude::*;
use std::path::PathBuf;

pub(crate) struct PersistencePlugin;

impl Plugin for PersistencePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(AutoSave {
            debounce: Timer::from_seconds(0.5, TimerMode::Once),
            periodic: Timer::from_seconds(60.0, TimerMode::Repeating),
            dirty: false,
        })
        .add_observer(save_on_default_event)
        .add_observer(load_on_default_event)
        .add_systems(Update, (mark_dirty_on_change, auto_save_system).chain());
    }
}

#[derive(Resource)]
struct AutoSave {
    debounce: Timer,
    periodic: Timer,
    dirty: bool,
}

pub(crate) fn session_path() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var_os("HOME").expect("HOME not set");
        PathBuf::from(home).join("Library/Application Support/vmux/session.ron")
    }
    #[cfg(not(target_os = "macos"))]
    {
        std::env::temp_dir().join("vmux_cef/session.ron")
    }
}

fn mark_dirty_on_change(
    mut auto_save: ResMut<AutoSave>,
    added_tabs: Query<(), Added<crate::layout::tab::Tab>>,
    added_panes: Query<(), Added<crate::layout::pane::Pane>>,
    added_spaces: Query<(), Added<crate::layout::space::Space>>,
    removed_tabs: RemovedComponents<crate::layout::tab::Tab>,
    removed_panes: RemovedComponents<crate::layout::pane::Pane>,
    changed_meta: Query<(), (Changed<vmux_header::PageMetadata>, With<crate::layout::tab::Tab>)>,
) {
    if !added_tabs.is_empty()
        || !added_panes.is_empty()
        || !added_spaces.is_empty()
        || removed_tabs.len() > 0
        || removed_panes.len() > 0
        || !changed_meta.is_empty()
    {
        auto_save.dirty = true;
        auto_save.debounce.reset();
    }
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
            let path = session_path();
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            commands.trigger(SaveWorld::default_into_file(path));
            auto_save.dirty = false;
        }
    }

    if auto_save.periodic.just_finished() {
        let path = session_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        commands.trigger(SaveWorld::default_into_file(path));
    }
}

/// Check if a session file exists and trigger load on startup.
pub(crate) fn load_session_on_startup(mut commands: Commands) {
    let path = session_path();
    if path.exists() {
        info!("Loading session from {:?}", path);
        commands.trigger(LoadWorld::default_from_file(path));
    }
}

/// Returns true if a saved session file exists on disk.
pub(crate) fn has_saved_session() -> bool {
    session_path().exists()
}
```

- [ ] **Step 2: Register in lib.rs**

```rust
mod persistence;
use persistence::PersistencePlugin;
// Add PersistencePlugin to the plugin tuple in VmuxPlugin::build()
```

- [ ] **Step 3: Build, verify, commit**

```bash
cargo build -p vmux_desktop
git add -A && git commit -m "feat: add save pipeline with debounced + periodic auto-save"
```

---

### Task 7: Load pipeline and window.rs refactor

**Files:**
- Modify: `crates/vmux_desktop/src/persistence.rs`
- Modify: `crates/vmux_desktop/src/layout/window.rs`
- Modify: `crates/vmux_desktop/src/lib.rs`

- [ ] **Step 1: Add view rebuild observers to persistence.rs**

Append to `crates/vmux_desktop/src/persistence.rs`:

```rust
use crate::{
    browser::Browser,
    layout::pane::{PaneSplit, PaneSplitDirection},
    settings::AppSettings,
};
use bevy_cef::prelude::*;

/// When a Tab is restored from save (has no Browser child), spawn Browser view.
pub(crate) fn rebuild_tab_view(
    trigger: On<Add, crate::layout::tab::Tab>,
    children_q: Query<&Children>,
    browser_q: Query<(), With<Browser>>,
    meta_q: Query<&vmux_header::PageMetadata>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut webview_mt: ResMut<Assets<WebviewExtendStandardMaterial>>,
) {
    let entity = trigger.target();

    // If tab already has a Browser child (fresh spawn), skip
    if let Ok(children) = children_q.get(entity) {
        if children.iter().any(|e| browser_q.contains(e)) {
            return;
        }
    }

    // Restored from save — spawn Browser child
    let url = meta_q
        .get(entity)
        .ok()
        .map(|m| m.url.as_str())
        .filter(|u| !u.is_empty())
        .unwrap_or("about:blank");
    commands.spawn((
        Browser::new(&mut meshes, &mut webview_mt, url),
        ChildOf(entity),
    ));
}

/// When PaneSplit is restored, apply correct Node layout.
pub(crate) fn rebuild_pane_split_view(
    trigger: On<Add, PaneSplit>,
    pane_q: Query<&PaneSplit>,
    settings: Res<AppSettings>,
    mut commands: Commands,
) {
    let entity = trigger.target();
    let Ok(split) = pane_q.get(entity) else { return };
    let direction = match split.direction {
        PaneSplitDirection::Row => bevy::ui::FlexDirection::Row,
        PaneSplitDirection::Column => bevy::ui::FlexDirection::Column,
    };
    let gap = Val::Px(settings.layout.pane.gap);
    commands.entity(entity).insert(Node {
        flex_grow: 1.0,
        flex_direction: direction,
        column_gap: gap,
        row_gap: gap,
        align_items: AlignItems::Stretch,
        ..default()
    });
}
```

Register in `PersistencePlugin::build()`:

```rust
        .add_observer(rebuild_tab_view)
        .add_observer(rebuild_pane_split_view)
```

- [ ] **Step 2: Refactor window.rs — shell/session split**

In `crates/vmux_desktop/src/layout/window.rs`:

Remove the inline Space/Pane/Tab subtree from the `children![]` macro in `setup()`. Replace the `Main` entry with an empty container:

```rust
            (
                Main,
                Transform::default(),
                GlobalTransform::default(),
                Node {
                    flex_grow: 1.0,
                    min_height: Val::Px(0.0),
                    ..default()
                },
            ),
```

Add a new system `spawn_default_session` that creates the initial session if none was loaded:

```rust
use crate::profile::Profile;
use vmux_history::{CreatedAt, LastActivatedAt};

fn spawn_default_session(
    main_q: Query<Entity, With<Main>>,
    profile_q: Query<(), With<Profile>>,
    settings: Res<AppSettings>,
    primary_window: Single<Entity, With<PrimaryWindow>>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut webview_mt: ResMut<Assets<WebviewExtendStandardMaterial>>,
) {
    // If profiles exist (loaded from session.ron), skip
    if !profile_q.is_empty() {
        return;
    }

    let Ok(main) = main_q.single() else { return };
    let startup_url = settings.browser.startup_url.as_str();
    let pw = *primary_window;

    let profile = commands.spawn((
        Profile::default_profile(),
        CreatedAt::now(),
        LastActivatedAt::now(),
        ChildOf(main),
    )).id();

    let space = commands.spawn((
        space_bundle(),
        LastActivatedAt::now(),
        CreatedAt::now(),
        ChildOf(profile),
    )).id();

    let split_root = commands.spawn((
        Pane,
        PaneSplit { direction: crate::layout::pane::PaneSplitDirection::Row },
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
        ChildOf(space),
    )).id();

    let leaf = commands.spawn((
        leaf_pane_bundle(),
        LastActivatedAt::now(),
        ChildOf(split_root),
    )).id();

    let tab = commands.spawn((
        tab_bundle(),
        LastActivatedAt::now(),
        CreatedAt::now(),
        ChildOf(leaf),
    )).id();

    commands.spawn((
        Browser::new(&mut meshes, &mut webview_mt, startup_url),
        ChildOf(tab),
    ));
}
```

Register in WindowPlugin, chained after setup:

```rust
impl Plugin for WindowPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Startup,
            (setup, spawn_default_session, fit_window_to_screen)
                .chain()
                .after(load_settings)
                .after(crate::scene::setup)
                .after(WebviewAppEmbedSet),
        )
        .add_systems(PostUpdate, fit_window_to_screen);
    }
}
```

- [ ] **Step 3: Wire load trigger into startup**

In `crates/vmux_desktop/src/lib.rs`, add the load system to run before window setup:

```rust
        app.add_systems(
            PreStartup,
            persistence::load_session_on_startup,
        );
```

Using `PreStartup` ensures loading happens before `Startup` systems (setup, spawn_default_session). If moonshine-save processes load events synchronously in observers, entities will exist by the time `spawn_default_session` checks `profile_q.is_empty()`.

If the load is async, `spawn_default_session` may need a frame delay. Test and adjust.

- [ ] **Step 4: Build and verify**

Run: `cargo build -p vmux_desktop`

- [ ] **Step 5: Manual test**

1. Delete `~/Library/Application Support/vmux/session.ron` if it exists
2. Run the app — default layout should appear
3. Open tabs, split panes, navigate to URLs
4. Quit the app
5. Check `session.ron` was created
6. Relaunch — layout/tabs should restore

- [ ] **Step 6: Commit**

```bash
git add -A && git commit -m "feat: add load pipeline with view rebuild observers, split window setup"
```

---

### Task 8: Visit spawning and vmux_history refactor

**Files:**
- Modify: `crates/vmux_desktop/src/browser.rs`
- Modify: `crates/vmux_history/src/lib.rs`
- Modify: `crates/vmux_history/src/plugin.rs`
- Modify: `crates/vmux_history/Cargo.toml`

- [ ] **Step 1: Add vmux_header dep to vmux_history**

In `crates/vmux_history/Cargo.toml`, add to native deps:

```toml
vmux_header = { path = "../vmux_header" }
```

- [ ] **Step 2: Add spawn_visit_on_navigation system**

In `crates/vmux_desktop/src/browser.rs`, add:

```rust
use vmux_history::{CreatedAt, Visit};
use crate::profile::Profile;

fn spawn_visit_on_navigation(
    changed_tabs: Query<(Entity, &PageMetadata), (With<Tab>, Changed<PageMetadata>)>,
    profile_q: Query<Entity, With<Profile>>,
    mut last_urls: Local<std::collections::HashMap<u64, String>>,
    mut commands: Commands,
) {
    let Ok(profile) = profile_q.single() else { return };

    for (entity, meta) in &changed_tabs {
        if meta.url.is_empty() || meta.url == "about:blank" {
            continue;
        }

        let key = entity.to_bits();
        let is_new = last_urls
            .get(&key)
            .map(|prev| prev != &meta.url)
            .unwrap_or(true);

        if is_new {
            last_urls.insert(key, meta.url.clone());
            commands.spawn((
                Visit,
                meta.clone(),
                CreatedAt::now(),
                ChildOf(profile),
            ));
        }
    }
}
```

Register in `BrowserPlugin::build()`:

```rust
        .add_systems(
            Update,
            spawn_visit_on_navigation.after(sync_page_metadata_to_tab),
        )
```

- [ ] **Step 3: Refactor vmux_history plugin.rs**

Replace `crates/vmux_history/src/plugin.rs`:

```rust
use std::path::PathBuf;

use bevy_cef::prelude::*;
use vmux_webview_app::{UiReady, WebviewAppConfig, WebviewAppRegistry};

use crate::event::{HISTORY_EVENT, HistoryEvent};
use crate::{CreatedAt, Visit};
use vmux_header::PageMetadata;

pub struct HistoryPlugin;

impl Plugin for HistoryPlugin {
    fn build(&self, app: &mut App) {
        app.world_mut()
            .resource_mut::<WebviewAppRegistry>()
            .register(
                PathBuf::from(env!("CARGO_MANIFEST_DIR")),
                &WebviewAppConfig::with_custom_host("history"),
            );
        app.add_systems(Update, push_history_via_host_emit);
    }
}

#[derive(Component, Clone, Copy, Debug)]
struct Sent(i64);

fn push_history_via_host_emit(
    mut commands: Commands,
    browsers: NonSend<Browsers>,
    ready: Query<Entity, (With<WebviewSource>, With<UiReady>, Without<Sent>)>,
    history_q: Query<(&PageMetadata, &CreatedAt), With<Visit>>,
) {
    for wv in ready.iter() {
        if !browsers.has_browser(wv) || !browsers.host_emit_ready(&wv) {
            continue;
        }
        let mut rows: Vec<(&PageMetadata, &CreatedAt)> = history_q.iter().collect();
        rows.sort_by_key(|(_, created)| std::cmp::Reverse(created.0));
        let history: Vec<String> = rows
            .into_iter()
            .map(|(meta, _)| meta.url.clone())
            .collect();
        let url = history.join(", ");
        let payload = HistoryEvent { url, history };
        let ron_body = ron::ser::to_string(&payload).unwrap_or_default();
        commands.trigger(HostEmitEvent::new(wv, HISTORY_EVENT, &ron_body));
        commands.entity(wv).insert(Sent(crate::now_millis()));
    }
}
```

- [ ] **Step 4: Build and verify**

Run: `cargo build -p vmux_desktop`

- [ ] **Step 5: Manual test**

1. Run the app, navigate to a few URLs
2. Open vmux://history/ in the side sheet — should show real visited URLs
3. Quit and relaunch — history should persist
4. Check `session.ron` contains Visit entities

- [ ] **Step 6: Commit**

```bash
git add -A && git commit -m "feat: spawn Visit entities on navigation, refactor vmux_history"
```

---

## Post-Implementation Notes

- **History pruning**: Visit entities accumulate. Add a system to prune old Visits (e.g., keep last 10,000 or last 90 days).
- **Scroll position**: `Tab.scroll_x`/`scroll_y` fields exist but aren't populated. Needs CEF API to read/restore scroll position.
- **Profile UI**: Phase 1 has no visible profile indicator in the header. Phase 2 adds the profile avatar button.
- **Multi-profile (Phase 2)**: Multiple Profile entities, per-profile `RequestContext`, profile switching UI. See spec.
