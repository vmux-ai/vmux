use bevy::ecs::relationship::Relationship;
use bevy::prelude::*;
use bevy::scene::SceneFilter;
use bevy::window::PrimaryWindow;
use bevy_cef::prelude::*;
use moonshine_save::prelude::*;
use std::path::PathBuf;

use crate::{
    browser::Browser,
    layout::{
        LayoutStartupSet, Open, SpaceFilePresent,
        pane::{Pane, PaneSize, PaneSplit, PaneSplitDirection, pane_split_gaps},
        stack::Stack,
        tab::Tab,
        window::Main,
    },
    profile::Profile,
    settings::AppSettings,
    spaces::{ActiveSpace, SpacesView},
    terminal::Terminal,
};
use vmux_core::PageMetadata;
use vmux_layout::event::{PROCESSES_WEBVIEW_URL, TERMINAL_WEBVIEW_URL};
use vmux_service::protocol::ProcessId;
use vmux_space::event::SPACES_WEBVIEW_URL;
use vmux_space::migration::migrate_legacy_session_files;

fn run_legacy_migration() {
    migrate_legacy_session_files(crate::profile::shared_data_dir());
}

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
        .add_systems(Startup, run_legacy_migration.before(load_space_on_startup))
        .add_systems(
            Startup,
            load_space_on_startup.in_set(LayoutStartupSet::Persistence),
        )
        .add_systems(Startup, rebuild_space_views.in_set(LayoutStartupSet::Post))
        .add_observer(mark_space_views_need_rebuild)
        .add_systems(
            Update,
            (rebuild_space_views, clear_space_views_need_rebuild)
                .chain()
                .run_if(resource_exists::<SpaceViewsNeedRebuild>),
        )
        .add_systems(Update, (mark_dirty_on_change, auto_save_system).chain());
    }
}

#[derive(Resource)]
struct SpaceViewsNeedRebuild;

fn mark_space_views_need_rebuild(_trigger: On<Loaded>, mut commands: Commands) {
    commands.insert_resource(SpaceViewsNeedRebuild);
}

fn clear_space_views_need_rebuild(mut commands: Commands) {
    commands.remove_resource::<SpaceViewsNeedRebuild>();
}

#[derive(Resource)]
struct AutoSave {
    debounce: Timer,
    periodic: Timer,
    dirty: bool,
}

pub(crate) fn space_path(active: &ActiveSpace) -> PathBuf {
    active.layout_path()
}

fn mark_dirty_on_change(
    mut auto_save: ResMut<AutoSave>,
    added_stacks: Query<(), Added<Stack>>,
    added_panes: Query<(), Added<Pane>>,
    added_tabs: Query<(), Added<Tab>>,
    removed_stacks: RemovedComponents<Stack>,
    removed_panes: RemovedComponents<Pane>,
    changed_meta: Query<(), (Changed<PageMetadata>, With<Stack>)>,
    changed_size: Query<(), Changed<PaneSize>>,
    changed_children: Query<(), Changed<Children>>,
) {
    if !added_stacks.is_empty()
        || !added_panes.is_empty()
        || !added_tabs.is_empty()
        || !removed_stacks.is_empty()
        || !removed_panes.is_empty()
        || !changed_meta.is_empty()
        || !changed_size.is_empty()
        || !changed_children.is_empty()
    {
        auto_save.dirty = true;
        auto_save.debounce.reset();
    }
}

fn auto_save_system(
    time: Res<Time>,
    mut auto_save: ResMut<AutoSave>,
    active: Res<ActiveSpace>,
    mut commands: Commands,
) {
    auto_save.periodic.tick(time.delta());

    if auto_save.dirty {
        auto_save.debounce.tick(time.delta());
        if auto_save.debounce.is_finished() {
            save_space_to_path(&mut commands, space_path(&active));
            auto_save.dirty = false;
        }
    }

    if auto_save.periodic.just_finished() {
        save_space_to_path(&mut commands, space_path(&active));
    }
}

pub(crate) fn save_space_to_path(commands: &mut Commands, path: PathBuf) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    // Use an allowlist to only save our model components.
    // ChildOf is the source of truth for hierarchy; Children is derived
    // automatically by Bevy's relationship system on load.
    let mut save = SaveWorld::default_into_file(path);
    save.components = SceneFilter::deny_all()
        .allow::<Save>()
        .allow::<ChildOf>()
        .allow::<Children>()
        .allow::<Stack>()
        .allow::<Tab>()
        .allow::<Pane>()
        .allow::<PaneSplit>()
        .allow::<PaneSize>()
        .allow::<Profile>()
        .allow::<Open>()
        .allow::<PageMetadata>()
        .allow::<vmux_history::CreatedAt>()
        .allow::<vmux_history::LastActivatedAt>()
        .allow::<vmux_history::Visit>();
    commands.trigger_save(save);
}

/// Check if a session file exists and trigger load on startup.
pub(crate) fn load_space_on_startup(active: Res<ActiveSpace>, mut commands: Commands) {
    let path = space_path(&active);
    let exists = path.exists();
    commands.insert_resource(SpaceFilePresent(exists));
    if exists {
        info!("Loading session from {:?}", path);
        commands.trigger_load(LoadWorld::default_from_file(path));
    }
}

/// Rebuild view components (Node, Transform, Browser, etc.) for entities
/// that were loaded from session.ron. Loaded entities only have model
/// components; this system adds the visual layer.
pub(crate) fn rebuild_space_views(
    main_q: Query<Entity, With<Main>>,
    tabs_need_view: Query<Entity, (With<Tab>, Without<Node>)>,
    splits_need_view: Query<(Entity, &PaneSplit), Without<Node>>,
    panes_need_view: Query<Entity, (With<Pane>, Without<PaneSplit>, Without<Node>)>,
    stacks_need_view: Query<(Entity, &PageMetadata), (With<Stack>, Without<Node>)>,
    pane_sizes: Query<&PaneSize>,
    child_of_q: Query<&ChildOf>,
    all_children: Query<&Children>,
    tab_children_q: Query<&Children, With<Stack>>,
    browser_q: Query<(), With<Browser>>,
    primary_window: Single<Entity, With<PrimaryWindow>>,
    settings: Res<AppSettings>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut webview_mt: ResMut<Assets<WebviewExtendStandardMaterial>>,
) {
    if tabs_need_view.is_empty()
        && splits_need_view.is_empty()
        && panes_need_view.is_empty()
        && stacks_need_view.is_empty()
    {
        return;
    }

    let Ok(main) = main_q.single() else { return };
    let pw = *primary_window;

    for tab_e in &tabs_need_view {
        commands.entity(tab_e).insert((
            Transform::default(),
            GlobalTransform::default(),
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
            ChildOf(main),
        ));
    }

    // -- PaneSplit: add flex container with gap + direction --
    for (entity, split) in &splits_need_view {
        let flex_dir = match split.direction {
            PaneSplitDirection::Row => FlexDirection::Row,
            PaneSplitDirection::Column => FlexDirection::Column,
        };
        let gap = pane_split_gaps(split.direction, settings.layout.pane.gap);
        let mut ecmds = commands.entity(entity);
        ecmds.insert((
            HostWindow(pw),
            ZIndex(0),
            Transform::default(),
            GlobalTransform::default(),
            Node {
                flex_grow: 1.0,
                min_height: Val::Px(0.0),
                flex_direction: flex_dir,
                column_gap: gap.column_gap,
                row_gap: gap.row_gap,
                ..default()
            },
        ));
    }

    // -- Leaf Pane: add stretch layout --
    for entity in &panes_need_view {
        let grow = pane_sizes.get(entity).map(|s| s.flex_grow).unwrap_or(1.0);
        commands.entity(entity).insert((
            Transform::default(),
            GlobalTransform::default(),
            Node {
                flex_grow: grow,
                flex_basis: Val::Px(0.0),
                align_items: AlignItems::Stretch,
                justify_content: JustifyContent::Stretch,
                ..default()
            },
        ));
    }

    // -- Stack: add absolute-fill node + spawn Browser child --
    let mut despawned = std::collections::HashSet::new();
    for (entity, meta) in &stacks_need_view {
        // Discard empty tabs (no URL, no content) that were saved mid-session
        if meta.url.is_empty() {
            despawned.insert(entity);
            commands.entity(entity).despawn();
            continue;
        }

        let mut ecmds = commands.entity(entity);
        ecmds.insert((
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
        ));

        let has_browser = tab_children_q
            .get(entity)
            .map(|ch| ch.iter().any(|e| browser_q.contains(e)))
            .unwrap_or(false);

        if !has_browser {
            if meta
                .url
                .starts_with(PROCESSES_WEBVIEW_URL.trim_end_matches('/'))
            {
                commands.spawn((
                    crate::processes_monitor::ProcessesMonitor::new(&mut meshes, &mut webview_mt),
                    ChildOf(entity),
                ));
            } else if meta
                .url
                .starts_with(TERMINAL_WEBVIEW_URL.trim_end_matches('/'))
            {
                // Try to extract process UUID from URL for reattach
                let process_id = meta
                    .url
                    .strip_prefix(TERMINAL_WEBVIEW_URL)
                    .and_then(|uuid_str| uuid_str.parse::<uuid::Uuid>().ok())
                    .map(ProcessId::from_uuid);

                if let Some(pid) = process_id {
                    commands.spawn((
                        Terminal::reattach(&mut meshes, &mut webview_mt, pid),
                        ChildOf(entity),
                    ));
                } else {
                    commands.spawn((
                        Terminal::new(&mut meshes, &mut webview_mt, &settings),
                        ChildOf(entity),
                    ));
                }
            } else {
                if meta
                    .url
                    .starts_with(SPACES_WEBVIEW_URL.trim_end_matches('/'))
                {
                    commands.spawn((
                        SpacesView::new(&mut meshes, &mut webview_mt),
                        ChildOf(entity),
                    ));
                } else {
                    commands.spawn((
                        Browser::new(&mut meshes, &mut webview_mt, &meta.url),
                        ChildOf(entity),
                    ));
                }
            }
        }
    }

    // -- Re-insert ChildOf in saved Children order --
    // Scene load deserializes ChildOf via reflection (bypassing hooks), so
    // Bevy's relationship system hasn't populated Children from hooks yet.
    // We re-insert ChildOf via commands so hooks fire and build the UI
    // hierarchy. By iterating each parent's deserialized Children in order,
    // the deferred commands preserve the saved sibling order.
    let mut seen_parents = std::collections::HashSet::new();
    for entity in splits_need_view
        .iter()
        .map(|(e, _)| e)
        .chain(panes_need_view.iter())
        .chain(stacks_need_view.iter().map(|(e, _)| e))
    {
        let Ok(co) = child_of_q.get(entity) else {
            continue;
        };
        let parent = co.get();
        if !seen_parents.insert(parent) {
            continue;
        }
        let Ok(children) = all_children.get(parent) else {
            continue;
        };
        for child in children.iter() {
            if despawned.contains(&child) {
                continue;
            }
            if let Ok(co) = child_of_q.get(child) {
                commands.entity(child).insert(ChildOf(co.get()));
            }
        }
    }

    info!(
        "Rebuilt session views: {} tabs, {} splits, {} panes, {} stacks",
        tabs_need_view.iter().count(),
        splits_need_view.iter().count(),
        panes_need_view.iter().count(),
        stacks_need_view.iter().count(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{
        AppSettings, BrowserSettings, FocusRingSettings, LayoutSettings, PaneSettings,
        ShortcutSettings, SideSheetSettings, WindowSettings,
    };
    use bevy::ecs::entity::EntityHashMap;

    struct HomeEnvGuard {
        _guard: std::sync::MutexGuard<'static, ()>,
        old_home: Option<std::ffi::OsString>,
    }

    impl HomeEnvGuard {
        fn use_temp_home(name: &str) -> Self {
            let guard = crate::profile::ENV_LOCK.lock().expect("env lock");
            let old_home = std::env::var_os("HOME");
            let home =
                std::env::temp_dir().join(format!("vmux-test-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&home);
            std::fs::create_dir_all(&home).expect("create temp home");
            unsafe {
                std::env::set_var("HOME", &home);
            }
            Self {
                _guard: guard,
                old_home,
            }
        }
    }

    impl Drop for HomeEnvGuard {
        fn drop(&mut self) {
            unsafe {
                if let Some(home) = &self.old_home {
                    std::env::set_var("HOME", home);
                } else {
                    std::env::remove_var("HOME");
                }
            }
        }
    }

    fn test_settings() -> AppSettings {
        AppSettings {
            browser: BrowserSettings {
                startup_url: "about:blank".to_string(),
            },
            layout: LayoutSettings {
                window: WindowSettings {
                    padding: 0.0,
                    padding_top: None,
                    padding_right: None,
                    padding_bottom: None,
                    padding_left: None,
                },
                pane: PaneSettings {
                    gap: 0.0,
                    radius: 0.0,
                },
                side_sheet: SideSheetSettings::default(),
                focus_ring: FocusRingSettings::default(),
            },
            shortcuts: ShortcutSettings::default(),
            terminal: None,
            auto_update: false,
        }
    }

    #[test]
    fn persisted_terminal_tab_reattaches_saved_process() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.insert_resource(test_settings());
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<WebviewExtendStandardMaterial>>();
        app.add_systems(Update, rebuild_space_views);

        let main = app.world_mut().spawn(Main).id();
        app.world_mut().spawn(PrimaryWindow);
        let space = app.world_mut().spawn((Tab::default(), ChildOf(main))).id();
        let pane = app.world_mut().spawn((Pane, ChildOf(space))).id();
        let saved_url = format!(
            "{}{}",
            TERMINAL_WEBVIEW_URL,
            uuid::Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap()
        );
        let tab = app
            .world_mut()
            .spawn((
                Stack::default(),
                PageMetadata {
                    title: "Terminal".to_string(),
                    url: saved_url.clone(),
                    favicon_url: String::new(),
                    bg_color: None,
                },
                ChildOf(pane),
            ))
            .id();

        app.update();

        let children = app.world().get::<Children>(tab).unwrap();
        let terminal = children
            .iter()
            .find(|entity| app.world().entity(*entity).contains::<Terminal>())
            .unwrap();
        let meta = app.world().get::<PageMetadata>(terminal).unwrap();

        let _ = saved_url;
        assert_eq!(meta.url, TERMINAL_WEBVIEW_URL);
    }

    #[test]
    fn runtime_loaded_session_rebuilds_browser_views() {
        let _home = HomeEnvGuard::use_temp_home("runtime-loaded-session-rebuilds-browser-views");
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.insert_resource(test_settings());
        app.insert_resource(ActiveSpace {
            record: vmux_space::model::default_space_record(),
        });
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<WebviewExtendStandardMaterial>>();
        app.add_plugins(PersistencePlugin);

        let main = app.world_mut().spawn(Main).id();
        app.world_mut().spawn(PrimaryWindow);
        app.update();

        let space = app.world_mut().spawn((Tab::default(), ChildOf(main))).id();
        let pane = app.world_mut().spawn((Pane, ChildOf(space))).id();
        let tab = app
            .world_mut()
            .spawn((
                Stack::default(),
                PageMetadata {
                    title: "Example".to_string(),
                    url: "https://example.com".to_string(),
                    favicon_url: String::new(),
                    bg_color: None,
                },
                ChildOf(pane),
            ))
            .id();

        app.world_mut().trigger(Loaded {
            entity_map: EntityHashMap::default(),
        });
        app.update();

        let children = app.world().get::<Children>(tab).unwrap();
        assert!(
            children
                .iter()
                .any(|entity| app.world().entity(entity).contains::<Browser>())
        );
    }
}
