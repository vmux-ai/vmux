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
        HeaderState, Open, SideSheetState,
        pane::{Pane, PaneSize, PaneSplit, PaneSplitDirection},
        side_sheet::{SideSheet, SideSheetPosition},
        space::Space,
        tab::Tab,
        window::Main,
    },
    profile::Profile,
    settings::AppSettings,
    terminal::Terminal,
};
use vmux_header::PageMetadata;
use vmux_processes::event::PROCESSES_WEBVIEW_URL;
use vmux_service::protocol::ProcessId;
use vmux_terminal::event::TERMINAL_WEBVIEW_URL;

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
    crate::profile::session_path()
}

fn mark_dirty_on_change(
    mut auto_save: ResMut<AutoSave>,
    added_tabs: Query<(), Added<Tab>>,
    added_panes: Query<(), Added<Pane>>,
    added_spaces: Query<(), Added<Space>>,
    removed_tabs: RemovedComponents<Tab>,
    removed_panes: RemovedComponents<Pane>,
    changed_meta: Query<(), (Changed<PageMetadata>, With<Tab>)>,
    changed_size: Query<(), Changed<PaneSize>>,
    changed_children: Query<(), Changed<Children>>,
    open_on_state: Query<
        (),
        (
            Or<(With<HeaderState>, With<SideSheetState>)>,
            Or<(Added<Open>, Changed<Open>)>,
        ),
    >,
    mut removed_open: RemovedComponents<Open>,
    state_entities: Query<Entity, Or<(With<HeaderState>, With<SideSheetState>)>>,
) {
    let open_state_changed =
        !open_on_state.is_empty() || removed_open.read().any(|e| state_entities.contains(e));

    if !added_tabs.is_empty()
        || !added_panes.is_empty()
        || !added_spaces.is_empty()
        || !removed_tabs.is_empty()
        || !removed_panes.is_empty()
        || !changed_meta.is_empty()
        || !changed_size.is_empty()
        || !changed_children.is_empty()
        || open_state_changed
    {
        auto_save.dirty = true;
        auto_save.debounce.reset();
    }
}

fn auto_save_system(time: Res<Time>, mut auto_save: ResMut<AutoSave>, mut commands: Commands) {
    auto_save.periodic.tick(time.delta());

    if auto_save.dirty {
        auto_save.debounce.tick(time.delta());
        if auto_save.debounce.is_finished() {
            do_save(&mut commands);
            auto_save.dirty = false;
        }
    }

    if auto_save.periodic.just_finished() {
        do_save(&mut commands);
    }
}

fn do_save(commands: &mut Commands) {
    let path = session_path();
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
        .allow::<Tab>()
        .allow::<Space>()
        .allow::<Pane>()
        .allow::<PaneSplit>()
        .allow::<PaneSize>()
        .allow::<Profile>()
        .allow::<Open>()
        .allow::<HeaderState>()
        .allow::<SideSheetState>()
        .allow::<PageMetadata>()
        .allow::<vmux_history::CreatedAt>()
        .allow::<vmux_history::LastActivatedAt>()
        .allow::<vmux_history::Visit>();
    commands.trigger_save(save);
}

/// Check if a session file exists and trigger load on startup.
pub(crate) fn load_session_on_startup(mut commands: Commands) {
    let path = session_path();
    if path.exists() {
        info!("Loading session from {:?}", path);
        commands.trigger_load(LoadWorld::default_from_file(path));
    }
}

/// Rebuild view components (Node, Transform, Browser, etc.) for entities
/// that were loaded from session.ron. Loaded entities only have model
/// components; this system adds the visual layer.
pub(crate) fn rebuild_session_views(
    main_q: Query<Entity, With<Main>>,
    spaces_need_view: Query<Entity, (With<Space>, Without<Node>)>,
    splits_need_view: Query<(Entity, &PaneSplit), Without<Node>>,
    panes_need_view: Query<Entity, (With<Pane>, Without<PaneSplit>, Without<Node>)>,
    tabs_need_view: Query<(Entity, &PageMetadata), (With<Tab>, Without<Node>)>,
    pane_sizes: Query<&PaneSize>,
    child_of_q: Query<&ChildOf>,
    all_children: Query<&Children>,
    tab_children_q: Query<&Children, With<Tab>>,
    browser_q: Query<(), With<Browser>>,
    primary_window: Single<Entity, With<PrimaryWindow>>,
    settings: Res<AppSettings>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut webview_mt: ResMut<Assets<WebviewExtendStandardMaterial>>,
) {
    if spaces_need_view.is_empty()
        && splits_need_view.is_empty()
        && panes_need_view.is_empty()
        && tabs_need_view.is_empty()
    {
        return;
    }

    let Ok(main) = main_q.single() else { return };
    let pw = *primary_window;

    // -- Space: add layout node, re-parent to Main container --
    for space in &spaces_need_view {
        commands.entity(space).insert((
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
    let gap = Val::Px(settings.layout.pane.gap);
    for (entity, split) in &splits_need_view {
        let flex_dir = match split.direction {
            PaneSplitDirection::Row => FlexDirection::Row,
            PaneSplitDirection::Column => FlexDirection::Column,
        };
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
                column_gap: gap,
                row_gap: gap,
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

    // -- Tab: add absolute-fill node + spawn Browser child --
    let mut despawned = std::collections::HashSet::new();
    for (entity, meta) in &tabs_need_view {
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
        .chain(tabs_need_view.iter().map(|(e, _)| e))
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
        "Rebuilt session views: {} spaces, {} splits, {} panes, {} tabs",
        spaces_need_view.iter().count(),
        splits_need_view.iter().count(),
        panes_need_view.iter().count(),
        tabs_need_view.iter().count(),
    );
}

/// Spawn persisted layout-state entities if they don't already exist
/// (handles first launch and migration from older sessions).
pub(crate) fn ensure_layout_state_entities(
    header_state_q: Query<(), With<HeaderState>>,
    side_sheet_state_q: Query<(), With<SideSheetState>>,
    mut commands: Commands,
) {
    if header_state_q.is_empty() {
        commands.spawn(HeaderState);
    }
    if side_sheet_state_q.is_empty() {
        commands.spawn(SideSheetState);
    }
}

/// Apply persisted open state from state entities to UI entities after load.
pub(crate) fn apply_persisted_layout_state(
    header_state_q: Query<Has<Open>, With<HeaderState>>,
    side_sheet_state_q: Query<Has<Open>, With<SideSheetState>>,
    header_q: Query<Entity, With<vmux_header::Header>>,
    side_sheet_q: Query<(Entity, &SideSheetPosition), With<SideSheet>>,
    mut commands: Commands,
) {
    for is_open in &header_state_q {
        if is_open {
            for entity in &header_q {
                commands.entity(entity).insert(Open);
            }
        }
    }
    for is_open in &side_sheet_state_q {
        if is_open {
            for (entity, pos) in &side_sheet_q {
                if *pos == SideSheetPosition::Left {
                    commands.entity(entity).insert(Open);
                }
            }
        }
    }
}
