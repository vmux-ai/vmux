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
        pane::{Pane, PaneSize, PaneSplit, PaneSplitDirection},
        space::Space,
        tab::Tab,
        window::Main,
    },
    profile::Profile,
    settings::AppSettings,
    terminal::Terminal,
};
use vmux_header::PageMetadata;
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
    added_tabs: Query<(), Added<Tab>>,
    added_panes: Query<(), Added<Pane>>,
    added_spaces: Query<(), Added<Space>>,
    removed_tabs: RemovedComponents<Tab>,
    removed_panes: RemovedComponents<Pane>,
    changed_meta: Query<(), (Changed<PageMetadata>, With<Tab>)>,
    changed_size: Query<(), Changed<PaneSize>>,
) {
    if !added_tabs.is_empty()
        || !added_panes.is_empty()
        || !added_spaces.is_empty()
        || removed_tabs.len() > 0
        || removed_panes.len() > 0
        || !changed_meta.is_empty()
        || !changed_size.is_empty()
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
        .allow::<Tab>()
        .allow::<Space>()
        .allow::<Pane>()
        .allow::<PaneSplit>()
        .allow::<PaneSize>()
        .allow::<Profile>()
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
        // Re-insert ChildOf via commands to trigger relationship hooks
        // (populates Children on parent). Scene load uses reflect-based
        // insertion which bypasses hooks.
        if let Ok(co) = child_of_q.get(entity) {
            ecmds.insert(ChildOf(co.get()));
        }
    }

    // -- Leaf Pane: add stretch layout --
    for entity in &panes_need_view {
        let grow = pane_sizes.get(entity).map(|s| s.flex_grow).unwrap_or(1.0);
        let mut ecmds = commands.entity(entity);
        ecmds.insert((
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
        if let Ok(co) = child_of_q.get(entity) {
            ecmds.insert(ChildOf(co.get()));
        }
    }

    // -- Tab: add absolute-fill node + spawn Browser child --
    for (entity, meta) in &tabs_need_view {
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
        if let Ok(co) = child_of_q.get(entity) {
            ecmds.insert(ChildOf(co.get()));
        }

        let has_browser = tab_children_q
            .get(entity)
            .map(|ch| ch.iter().any(|e| browser_q.contains(e)))
            .unwrap_or(false);

        if !has_browser {
            if meta.url.starts_with(TERMINAL_WEBVIEW_URL.trim_end_matches('/')) {
                commands.spawn((
                    Terminal::new(&mut meshes, &mut webview_mt, &settings),
                    ChildOf(entity),
                ));
            } else {
                let url = if meta.url.is_empty() {
                    "about:blank"
                } else {
                    &meta.url
                };
                commands.spawn((
                    Browser::new(&mut meshes, &mut webview_mt, url),
                    ChildOf(entity),
                ));
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


