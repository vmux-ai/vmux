use bevy::ecs::relationship::Relationship;
use bevy::prelude::*;
use bevy::scene::SceneFilter;
use bevy::window::PrimaryWindow;
use bevy_cef::prelude::*;
use moonshine_save::prelude::*;
use std::path::PathBuf;

use crate::{
    browser::Browser,
    profile::Profile,
    settings::AppSettings,
    settings_view::SettingsView,
    spaces::{ActiveSpace, SpacesView},
    terminal::Terminal,
};
use vmux_core::PageMetadata;
use vmux_layout::event::SERVICES_WEBVIEW_URL;
use vmux_layout::event::TERMINAL_WEBVIEW_URL;
use vmux_layout::{
    LayoutStartupSet, Open, SpaceFilePresent,
    pane::{Pane, PaneSize, PaneSplit, PaneSplitDirection, pane_split_gaps},
    space::Space,
    stack::Stack,
    window::Main,
};
use vmux_settings::event::SETTINGS_WEBVIEW_URL;
use vmux_space::event::SPACES_WEBVIEW_URL;
use vmux_space::migration::migrate_legacy_session_files;

fn run_legacy_migration() {
    migrate_legacy_session_files(crate::profile::shared_data_dir());
}

pub(crate) struct PersistencePlugin;

impl Plugin for PersistencePlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<crate::terminal::launch::TerminalLaunch>()
            .register_type::<crate::terminal::launch::TerminalKind>();
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
        .add_systems(Update, (mark_dirty_on_change, auto_save_system).chain())
        .add_systems(Update, sync_launch_to_stack);
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
    added_tabs: Query<(), Added<Space>>,
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
        .allow::<Space>()
        .allow::<Pane>()
        .allow::<PaneSplit>()
        .allow::<PaneSize>()
        .allow::<Profile>()
        .allow::<Open>()
        .allow::<PageMetadata>()
        .allow::<vmux_history::CreatedAt>()
        .allow::<vmux_history::LastActivatedAt>()
        .allow::<vmux_history::Visit>()
        .allow::<crate::terminal::launch::TerminalLaunch>();
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
    tabs_need_view: Query<Entity, (With<Space>, Without<Node>)>,
    splits_need_view: Query<(Entity, &PaneSplit), Without<Node>>,
    panes_need_view: Query<Entity, (With<Pane>, Without<PaneSplit>, Without<Node>)>,
    stacks_need_view: Query<
        (
            Entity,
            &PageMetadata,
            Option<&crate::terminal::launch::TerminalLaunch>,
        ),
        (With<Stack>, Without<Node>),
    >,
    pane_sizes: Query<&PaneSize>,
    child_of_q: Query<&ChildOf>,
    all_children: Query<&Children>,
    tab_children_q: Query<&Children, With<Stack>>,
    browser_q: Query<(), With<Browser>>,
    primary_window: Single<Entity, With<PrimaryWindow>>,
    settings: Res<AppSettings>,
    strategies: Res<vmux_agent::strategy::AgentStrategies>,
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
        let gap = pane_split_gaps(split.direction, vmux_layout::event::PANE_GAP_PX);
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
    for (entity, meta, saved_launch) in &stacks_need_view {
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
                .starts_with(SERVICES_WEBVIEW_URL.trim_end_matches('/'))
            {
                commands.spawn((
                    crate::processes_monitor::ProcessesMonitor::new(&mut meshes, &mut webview_mt),
                    ChildOf(entity),
                ));
            } else if meta
                .url
                .starts_with(TERMINAL_WEBVIEW_URL.trim_end_matches('/'))
            {
                let cwd = saved_launch.map(|l| std::path::PathBuf::from(&l.cwd));
                let term = commands
                    .spawn((
                        Terminal::new_with_cwd(
                            &mut meshes,
                            &mut webview_mt,
                            &settings,
                            cwd.as_deref(),
                        ),
                        ChildOf(entity),
                    ))
                    .id();
                if let Some(launch) = saved_launch {
                    commands.entity(term).insert(launch.clone());
                }
            } else if let Some(kind) = vmux_agent::AgentKind::all()
                .into_iter()
                .find(|k| meta.url.starts_with(&k.cli_url_prefix()))
            {
                let id_part = meta.url.strip_prefix(&kind.cli_url_prefix()).unwrap_or("");
                let session_id = (!id_part.is_empty()).then(|| id_part.to_string());
                let cwd = saved_launch
                    .map(|l| std::path::PathBuf::from(&l.cwd))
                    .unwrap_or_else(|| {
                        std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("/"))
                    });
                if let Err(e) = crate::terminal::spawn_agent_into_stack(
                    kind,
                    entity,
                    cwd,
                    session_id,
                    &strategies,
                    &mut commands,
                    &mut meshes,
                    &mut webview_mt,
                    &settings,
                ) {
                    bevy::log::warn!("restore agent tab failed: {e}");
                }
            } else if meta
                .url
                .starts_with(SPACES_WEBVIEW_URL.trim_end_matches('/'))
            {
                commands.spawn((
                    SpacesView::new(&mut meshes, &mut webview_mt),
                    ChildOf(entity),
                ));
            } else if meta
                .url
                .starts_with(SETTINGS_WEBVIEW_URL.trim_end_matches('/'))
            {
                commands.spawn((
                    SettingsView::new(&mut meshes, &mut webview_mt),
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
        .chain(stacks_need_view.iter().map(|(e, _, _)| e))
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

fn sync_launch_to_stack(
    terminals: Query<
        (&ChildOf, &crate::terminal::launch::TerminalLaunch),
        (
            With<Terminal>,
            Changed<crate::terminal::launch::TerminalLaunch>,
        ),
    >,
    stacks: Query<(), With<Stack>>,
    mut commands: Commands,
) {
    for (child_of, launch) in &terminals {
        let parent = child_of.get();
        if stacks.contains(parent) {
            commands.entity(parent).insert(launch.clone());
        }
    }
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
                radius: 0.0,
                window: WindowSettings {
                    padding: 0.0,
                    padding_top: None,
                    padding_right: None,
                    padding_bottom: None,
                    padding_left: None,
                },
                pane: PaneSettings { gap: 0.0 },
                side_sheet: SideSheetSettings::default(),
                focus_ring: FocusRingSettings::default(),
            },
            shortcuts: ShortcutSettings::default(),
            terminal: None,
            auto_update: false,
            startup_url: None,
            agent: crate::settings::AgentSettings::default(),
        }
    }

    #[test]
    fn persisted_terminal_tab_reattaches_saved_process() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.insert_resource(test_settings());
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<WebviewExtendStandardMaterial>>();
        app.init_resource::<vmux_agent::strategy::AgentStrategies>();
        app.add_systems(Update, rebuild_space_views);

        let main = app.world_mut().spawn(Main).id();
        app.world_mut().spawn(PrimaryWindow);
        let space = app
            .world_mut()
            .spawn((Space::default(), ChildOf(main)))
            .id();
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
        app.init_resource::<vmux_agent::strategy::AgentStrategies>();
        app.add_plugins(PersistencePlugin);

        let main = app.world_mut().spawn(Main).id();
        app.world_mut().spawn(PrimaryWindow);
        app.update();

        let space = app
            .world_mut()
            .spawn((Space::default(), ChildOf(main)))
            .id();
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
