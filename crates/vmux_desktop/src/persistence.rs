use bevy::ecs::relationship::Relationship;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_cef::prelude::*;
use bevy_world_serialization::WorldFilter;
use moonshine_save::prelude::*;
use std::path::{Path, PathBuf};

use vmux_browser::Browser;
use vmux_core::{
    ArchivedPage, ArchivedPagePosition, ArchivedTabPage, CreatedAt, Order, PageMetadata,
};
use vmux_layout::event::SERVICES_PAGE_URL;
use vmux_layout::event::TERMINAL_PAGE_URL;
use vmux_layout::profile::Profile;
use vmux_layout::space::{Space, SpaceId};
use vmux_layout::{
    LayoutStartupSet, Open, SpaceFilePresent,
    pane::{Pane, PaneId, PaneSize, PaneSplit, PaneSplitDirection, pane_split_gaps},
    stack::Stack,
    tab::{Tab, TabDirDecided, TabWorkspace, TabWorktree},
    window::{Main, WindowGeometry},
};
use vmux_setting::AppSettings;
use vmux_setting::Settings;
use vmux_setting::event::SETTINGS_PAGE_URL;
use vmux_space::event::SPACES_PAGE_URL;
use vmux_space::{ActiveSpace, Spaces};
use vmux_terminal::Terminal;
use vmux_terminal::new_terminal_bundle_with_cwd;

pub(crate) struct PersistencePlugin;

impl Plugin for PersistencePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(AutoSave {
            debounce: Timer::from_seconds(0.5, TimerMode::Once),
            periodic: Timer::from_seconds(60.0, TimerMode::Repeating),
            dirty: false,
        })
        .init_resource::<crate::boot_status::RestoreComplete>()
        .add_message::<vmux_core::agent::SpawnAgentInStackRequest>()
        .add_message::<vmux_space::SaveSpaceRequest>()
        .add_observer(save_on_default_event)
        .add_observer(load_on_default_event)
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
        .add_systems(
            Update,
            (
                (mark_dirty_on_change, auto_save_system).chain(),
                sync_launch_to_stack,
                handle_save_space_requests,
            ),
        );
    }
}

fn handle_save_space_requests(
    mut requests: MessageReader<vmux_space::SaveSpaceRequest>,
    mut commands: Commands,
) {
    for request in requests.read() {
        save_space_to_path(&mut commands, request.path.clone());
    }
}

#[derive(Resource)]
struct SpaceViewsNeedRebuild;

fn mark_space_views_need_rebuild(_trigger: On<Loaded>, mut commands: Commands) {
    commands.insert_resource(SpaceViewsNeedRebuild);
}

fn clear_space_views_need_rebuild(
    mut restore: ResMut<crate::boot_status::RestoreComplete>,
    mut commands: Commands,
) {
    restore.0 = true;
    commands.remove_resource::<SpaceViewsNeedRebuild>();
}

#[derive(Resource)]
struct AutoSave {
    debounce: Timer,
    periodic: Timer,
    dirty: bool,
}

#[derive(bevy::ecs::system::SystemParam)]
struct TabPersistenceChanges<'w, 's> {
    changed_workspaces: Query<'w, 's, (), Changed<TabWorkspace>>,
    changed_worktrees: Query<'w, 's, (), Changed<TabWorktree>>,
    removed_workspaces: RemovedComponents<'w, 's, TabWorkspace>,
    removed_worktrees: RemovedComponents<'w, 's, TabWorktree>,
}

// v4: agent URL grammar changed (CLI moved to `vmux://agent/<kind>/cli/<sid>`, freeing the
// two-segment form for ACP sessions). Persisted stores from v3 reference the old grammar, so
// they are reset on upgrade rather than migrated in place.
const STORE_SCHEMA_VERSION: u32 = 4;

pub(crate) fn store_path() -> PathBuf {
    vmux_core::profile::store_dir().join("store.ron")
}

fn store_version_path() -> PathBuf {
    store_version_path_for_store(&store_path())
}

fn store_version_path_for_store(path: &Path) -> PathBuf {
    path.parent()
        .map(|parent| parent.join("store.version"))
        .unwrap_or_else(|| PathBuf::from("store.version"))
}

fn store_schema_is_current() -> bool {
    std::fs::read_to_string(store_version_path())
        .ok()
        .and_then(|s| s.trim().parse::<u32>().ok())
        .map(|v| v >= STORE_SCHEMA_VERSION)
        .unwrap_or(false)
}

fn write_store_schema_version(path: &Path) {
    let _ = std::fs::write(
        store_version_path_for_store(path),
        STORE_SCHEMA_VERSION.to_string(),
    );
}

fn mark_dirty_on_change(
    mut auto_save: ResMut<AutoSave>,
    added_stacks: Query<(), Added<Stack>>,
    added_panes: Query<(), Added<Pane>>,
    added_tabs: Query<(), Added<Tab>>,
    changed_tabs: Query<(), Changed<Tab>>,
    mut tab_changes: TabPersistenceChanges,
    removed_stacks: RemovedComponents<Stack>,
    removed_panes: RemovedComponents<Pane>,
    changed_meta: Query<(), (Changed<PageMetadata>, With<Stack>)>,
    changed_size: Query<(), Changed<PaneSize>>,
    changed_children: Query<(), Changed<Children>>,
    changed_geometry: Query<(), Changed<WindowGeometry>>,
    added_archived: Query<(), Added<ArchivedPage>>,
    mut removed_archived: RemovedComponents<ArchivedPage>,
    added_visits: Query<(), Added<vmux_history::Visit>>,
) {
    if !added_stacks.is_empty()
        || !added_panes.is_empty()
        || !added_tabs.is_empty()
        || !changed_tabs.is_empty()
        || !tab_changes.changed_workspaces.is_empty()
        || !tab_changes.changed_worktrees.is_empty()
        || !removed_stacks.is_empty()
        || !removed_panes.is_empty()
        || tab_changes.removed_worktrees.read().count() > 0
        || tab_changes.removed_workspaces.read().count() > 0
        || !changed_meta.is_empty()
        || !changed_size.is_empty()
        || !changed_children.is_empty()
        || !changed_geometry.is_empty()
        || !added_archived.is_empty()
        || removed_archived.read().count() > 0
        || !added_visits.is_empty()
    {
        auto_save.dirty = true;
        auto_save.debounce.reset();
    }
}

fn auto_save_system(
    time: Res<Time>,
    mut auto_save: ResMut<AutoSave>,
    spaces: Query<(), With<Space>>,
    mut commands: Commands,
) {
    auto_save.periodic.tick(time.delta());

    if spaces.is_empty() {
        return;
    }

    if auto_save.dirty {
        auto_save.debounce.tick(time.delta());
        if auto_save.debounce.is_finished() {
            save_space_to_path(&mut commands, store_path());
            auto_save.dirty = false;
        }
    }

    if auto_save.periodic.just_finished() {
        save_space_to_path(&mut commands, store_path());
    }
}

pub(crate) fn save_space_to_path(commands: &mut Commands, path: PathBuf) {
    if vmux_core::profile::is_test_session() {
        return;
    }
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    write_store_schema_version(&path);
    // Use an allowlist to only save our model components.
    // ChildOf is the source of truth for hierarchy; Children is derived
    // automatically by Bevy's relationship system on load.
    let mut save = SaveWorld::default_into_file(path);
    save.components = WorldFilter::deny_all()
        .allow::<Save>()
        .allow::<ChildOf>()
        .allow::<Children>()
        .allow::<Name>()
        .allow::<Stack>()
        .allow::<Tab>()
        .allow::<TabWorkspace>()
        .allow::<TabWorktree>()
        .allow::<TabDirDecided>()
        .allow::<Pane>()
        .allow::<PaneSplit>()
        .allow::<PaneSize>()
        .allow::<Space>()
        .allow::<SpaceId>()
        .allow::<WindowGeometry>()
        .allow::<Profile>()
        .allow::<Open>()
        .allow::<PageMetadata>()
        .allow::<ArchivedPage>()
        .allow::<ArchivedPagePosition>()
        .allow::<ArchivedTabPage>()
        .allow::<PaneId>()
        .allow::<vmux_history::CreatedAt>()
        .allow::<vmux_history::LastActivatedAt>()
        .allow::<vmux_history::Visit>()
        .allow::<vmux_core::Url>()
        .allow::<vmux_core::VisitCount>()
        .allow::<vmux_core::LastVisitedAt>()
        .allow::<vmux_core::VisitedUrl>()
        .allow::<vmux_core::TransitionType>()
        .allow::<vmux_core::Order>()
        .allow::<vmux_terminal::launch::TerminalLaunch>();
    commands.trigger_save(save);
}

/// Check if a space file exists and trigger load on startup.
pub(crate) fn load_space_on_startup(
    active: Res<ActiveSpace>,
    registry: Res<AppTypeRegistry>,
    mut restore: ResMut<crate::boot_status::RestoreComplete>,
    mut commands: Commands,
) {
    if vmux_core::profile::is_test_session() {
        commands.insert_resource(SpaceFilePresent(false));
        restore.0 = true;
        commands.spawn(vmux_space::spaces::space_profile_bundle(&active.record));
        return;
    }
    let path = store_path();
    let removed_stale = remove_stale_space_if_needed(&path);
    let removed_incompatible = {
        let registry = registry.read();
        remove_incompatible_store_if_needed(&path, &registry)
    };
    let schema_outdated = path.exists() && !store_schema_is_current();
    if schema_outdated {
        warn!("Store schema outdated; resetting {:?}", path);
        if let Err(e) = std::fs::remove_file(&path) {
            warn!("Failed to remove outdated store {:?}: {e}", path);
        }
        let _ = std::fs::remove_file(store_version_path());
    }
    // Never load a schema-incompatible store, even if deletion failed above —
    // loading it would hit deserialization errors / unknown component types.
    let exists = path.exists() && !removed_stale && !removed_incompatible && !schema_outdated;
    commands.insert_resource(SpaceFilePresent(exists));
    if exists {
        info!("Loading space from {:?}", path);
        commands.trigger_load(LoadWorld::default_from_file(path));
    } else {
        restore.0 = true;
        commands.spawn(vmux_space::spaces::space_profile_bundle(&active.record));
    }
}

fn remove_stale_space_if_needed(path: &Path) -> bool {
    let Ok(body) = std::fs::read_to_string(path) else {
        return false;
    };
    if !space_is_stale(&body) {
        return false;
    }
    warn!("Removing stale store from {:?}", path);
    let _ = std::fs::remove_file(path);
    true
}

fn remove_incompatible_store_if_needed(
    path: &Path,
    registry: &bevy::reflect::TypeRegistry,
) -> bool {
    let Ok(body) = std::fs::read_to_string(path) else {
        return false;
    };
    if !space_has_unregistered_types(&body, registry) {
        return false;
    }
    warn!(
        "Removing incompatible store (unregistered component types) from {:?}",
        path
    );
    let _ = std::fs::remove_file(path);
    if let Some(parent) = path.parent() {
        let _ = std::fs::remove_file(parent.join("store.version"));
    }
    true
}

fn space_has_unregistered_types(body: &str, registry: &bevy::reflect::TypeRegistry) -> bool {
    component_type_path_keys(body).any(|path| registry.get_with_type_path(path).is_none())
}

fn component_type_path_keys(body: &str) -> impl Iterator<Item = &str> {
    body.lines().filter_map(|line| {
        let rest = line.trim_start().strip_prefix('"')?;
        let (key, after) = rest.split_once('"')?;
        if key.contains("::") && after.trim_start().starts_with(':') {
            Some(key)
        } else {
            None
        }
    })
}

fn space_is_stale(body: &str) -> bool {
    space_contains_stale_agent_url(body) || space_is_prompt_only_empty_url(body)
}

fn space_contains_stale_agent_url(body: &str) -> bool {
    body.split("vmux://agent/").skip(1).any(|tail| {
        let suffix = tail.split('"').next().unwrap_or_default();
        let url = format!("vmux://agent/{suffix}");
        is_stale_agent_url(&url)
    })
}

fn is_stale_agent_url(url: &str) -> bool {
    let normalized = url.trim_end_matches('/');
    if normalized == "vmux://agent" {
        return false;
    }
    if is_bare_agent_kind_url(normalized) {
        return false;
    }
    vmux_agent::AgentUrl::parse(normalized).is_none()
}

fn is_bare_agent_kind_url(normalized: &str) -> bool {
    vmux_agent::AgentKind::all()
        .into_iter()
        .any(|kind| normalized == kind.cli_url_prefix().trim_end_matches('/'))
}

fn space_is_prompt_only_empty_url(body: &str) -> bool {
    let urls = page_metadata_urls(body);
    !urls.is_empty() && urls.iter().all(|url| url.trim().is_empty())
}

fn page_metadata_urls(body: &str) -> Vec<&str> {
    let mut urls = Vec::new();
    let mut in_page_metadata = false;
    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("\"vmux_header::system::PageMetadata\":") {
            in_page_metadata = true;
            continue;
        }
        if !in_page_metadata {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("url: \"")
            && let Some((url, _)) = rest.split_once('"')
        {
            urls.push(url);
        }
        if trimmed == ")," {
            in_page_metadata = false;
        }
    }
    urls
}

fn sort_tabs_by_order(mut tabs: Vec<(Entity, Option<u32>, Option<i64>)>) -> Vec<Entity> {
    tabs.sort_by_key(|(_, order, created)| (order.unwrap_or(u32::MAX), created.unwrap_or(0)));
    tabs.into_iter().map(|(entity, _, _)| entity).collect()
}

/// Rebuild view components (Node, Transform, Browser, etc.) for entities
/// that were loaded from space.ron. Loaded entities only have model
/// components; this system adds the visual layer.
pub(crate) fn rebuild_space_views(
    main_q: Query<Entity, With<Main>>,
    tabs_need_view: Query<(Entity, Option<&Order>, Option<&CreatedAt>), (With<Tab>, Without<Node>)>,
    spaces_need_view: Query<Entity, (With<Space>, Without<Node>)>,
    splits_need_view: Query<(Entity, &PaneSplit), Without<Node>>,
    panes_need_view: Query<Entity, (With<Pane>, Without<PaneSplit>, Without<Node>)>,
    stacks_need_view: Query<
        (
            Entity,
            &PageMetadata,
            Option<&vmux_terminal::launch::TerminalLaunch>,
        ),
        (With<Stack>, Without<Node>),
    >,
    pane_sizes: Query<&PaneSize>,
    child_of_q: Query<&ChildOf>,
    all_children: Query<&Children>,
    browser_q: Query<(), With<Browser>>,
    primary_window: Single<Entity, With<PrimaryWindow>>,
    settings: Res<AppSettings>,
    mut spawn_agent: MessageWriter<vmux_core::agent::SpawnAgentInStackRequest>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut webview_mt: ResMut<Assets<WebviewExtendStandardMaterial>>,
) {
    if tabs_need_view.is_empty()
        && spaces_need_view.is_empty()
        && splits_need_view.is_empty()
        && panes_need_view.is_empty()
        && stacks_need_view.is_empty()
    {
        return;
    }

    let Ok(main) = main_q.single() else { return };
    let pw = *primary_window;

    for space in &spaces_need_view {
        commands
            .entity(space)
            .insert((vmux_layout::space::space_view_bundle(), ChildOf(main)));
    }

    let saved_tab_order: Vec<(Entity, Option<u32>, Option<i64>)> = tabs_need_view
        .iter()
        .map(|(entity, order, created)| (entity, order.map(|o| o.0), created.map(|c| c.0)))
        .collect();
    for tab_e in sort_tabs_by_order(saved_tab_order) {
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
        ));
        if let Ok(co) = child_of_q.get(tab_e) {
            commands.entity(tab_e).insert(ChildOf(co.get()));
        }
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

        let has_browser = all_children
            .get(entity)
            .map(|ch| ch.iter().any(|e| browser_q.contains(e)))
            .unwrap_or(false);

        if !has_browser {
            if meta
                .url
                .starts_with(SERVICES_PAGE_URL.trim_end_matches('/'))
            {
                commands.spawn((
                    vmux_terminal::processes_monitor::ProcessesMonitor::new(
                        &mut meshes,
                        &mut webview_mt,
                    ),
                    ChildOf(entity),
                ));
            } else if meta
                .url
                .starts_with(TERMINAL_PAGE_URL.trim_end_matches('/'))
            {
                let cwd = saved_launch.map(|l| std::path::PathBuf::from(&l.cwd));
                let term = commands
                    .spawn((
                        new_terminal_bundle_with_cwd(
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
            } else if let Some(agent_url) = vmux_agent::AgentUrl::parse(&meta.url).filter(|u| {
                matches!(
                    u,
                    vmux_agent::AgentUrl::Cli { .. } | vmux_agent::AgentUrl::Acp { .. }
                )
            }) {
                match agent_url {
                    vmux_agent::AgentUrl::Cli { kind, sid } => {
                        let session_id = (sid != vmux_agent::url::CLI_FRESH_SID).then_some(sid);
                        let cwd = saved_launch
                            .map(|l| std::path::PathBuf::from(&l.cwd))
                            .unwrap_or_else(|| {
                                std::env::current_dir()
                                    .unwrap_or_else(|_| std::path::PathBuf::from("/"))
                            });
                        spawn_agent.write(vmux_core::agent::SpawnAgentInStackRequest {
                            kind,
                            cwd,
                            session_id,
                            stack: entity,
                            initial_prompt: None,
                            initial_attachments: Vec::new(),
                        });
                    }
                    _ => {
                        // ACP: reopen through the runtime page-open path, which reconstructs the
                        // session (and requests loadSession when the url carries a session id).
                        commands.spawn(vmux_core::PageOpenTask {
                            id: vmux_core::PageOpenId::new(),
                            stack: entity,
                            url: meta.url.clone(),
                            request_id: None,
                        });
                    }
                }
            } else if meta.url.starts_with(SPACES_PAGE_URL.trim_end_matches('/')) {
                commands.spawn((Spaces::new(&mut meshes, &mut webview_mt), ChildOf(entity)));
            } else if meta
                .url
                .starts_with(SETTINGS_PAGE_URL.trim_end_matches('/'))
            {
                commands.spawn((Settings::new(&mut meshes, &mut webview_mt), ChildOf(entity)));
            } else if meta.url.starts_with("file:") {
                if let Some(bundle) =
                    vmux_editor::restore_file_view_bundle(&meta.url, &mut meshes, &mut webview_mt)
                {
                    commands.spawn((bundle, ChildOf(entity)));
                }
            } else {
                let browser = commands
                    .spawn((
                        Browser::new(&mut meshes, &mut webview_mt, &meta.url),
                        ChildOf(entity),
                    ))
                    .id();
                commands.entity(browser).insert(meta.clone());
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
        "Rebuilt space views: {} tabs, {} splits, {} panes, {} stacks",
        tabs_need_view.iter().count(),
        splits_need_view.iter().count(),
        panes_need_view.iter().count(),
        stacks_need_view.iter().count(),
    );
}

fn sync_launch_to_stack(
    terminals: Query<
        (&ChildOf, &vmux_terminal::launch::TerminalLaunch),
        (
            With<Terminal>,
            Changed<vmux_terminal::launch::TerminalLaunch>,
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
    use bevy::ecs::entity::EntityHashMap;
    use vmux_layout::settings::{
        FocusRingSettings, LayoutSettings, PaneSettings, SideSheetSettings, WindowSettings,
    };
    use vmux_setting::{AppSettings, BrowserSettings, ShortcutSettings};

    #[test]
    fn adding_archived_page_marks_store_dirty() {
        let mut app = App::new();
        app.insert_resource(AutoSave {
            debounce: Timer::from_seconds(0.5, TimerMode::Once),
            periodic: Timer::from_seconds(60.0, TimerMode::Repeating),
            dirty: false,
        })
        .add_systems(Update, mark_dirty_on_change);
        app.update();
        app.world_mut().resource_mut::<AutoSave>().dirty = false;
        app.world_mut().spawn(ArchivedPage::default());
        app.update();
        assert!(app.world().resource::<AutoSave>().dirty);
    }

    #[test]
    fn adding_visit_marks_store_dirty() {
        let mut app = App::new();
        app.insert_resource(AutoSave {
            debounce: Timer::from_seconds(0.5, TimerMode::Once),
            periodic: Timer::from_seconds(60.0, TimerMode::Repeating),
            dirty: false,
        })
        .add_systems(Update, mark_dirty_on_change);
        app.update();
        app.world_mut().resource_mut::<AutoSave>().dirty = false;
        app.world_mut().spawn(vmux_history::Visit);
        app.update();
        assert!(app.world().resource::<AutoSave>().dirty);
    }

    #[test]
    fn changing_tab_startup_dir_marks_store_dirty() {
        let mut app = App::new();
        app.insert_resource(AutoSave {
            debounce: Timer::from_seconds(0.5, TimerMode::Once),
            periodic: Timer::from_seconds(60.0, TimerMode::Repeating),
            dirty: false,
        })
        .add_systems(Update, mark_dirty_on_change);
        let tab = app.world_mut().spawn(Tab::default()).id();
        app.update();
        app.world_mut().resource_mut::<AutoSave>().dirty = false;
        app.world_mut()
            .entity_mut(tab)
            .get_mut::<Tab>()
            .unwrap()
            .startup_dir = Some("/tmp/rebound".into());

        app.update();

        assert!(app.world().resource::<AutoSave>().dirty);
    }

    #[test]
    fn adding_tab_workspace_marks_store_dirty() {
        let mut app = App::new();
        app.insert_resource(AutoSave {
            debounce: Timer::from_seconds(0.5, TimerMode::Once),
            periodic: Timer::from_seconds(60.0, TimerMode::Repeating),
            dirty: false,
        })
        .add_systems(Update, mark_dirty_on_change);
        let tab = app.world_mut().spawn(Tab::default()).id();
        app.update();
        app.world_mut().resource_mut::<AutoSave>().dirty = false;
        app.world_mut().entity_mut(tab).insert(TabWorkspace {
            project_dir: "/tmp/project".into(),
        });

        app.update();

        assert!(app.world().resource::<AutoSave>().dirty);
    }

    #[test]
    fn removing_tab_worktree_marks_store_dirty() {
        let mut app = App::new();
        app.insert_resource(AutoSave {
            debounce: Timer::from_seconds(0.5, TimerMode::Once),
            periodic: Timer::from_seconds(60.0, TimerMode::Repeating),
            dirty: false,
        })
        .add_systems(Update, mark_dirty_on_change);
        let tab = app
            .world_mut()
            .spawn((
                Tab::default(),
                TabWorktree {
                    repo_root: "/tmp/repo".into(),
                    checkout_dir: "/tmp/worktree".into(),
                    branch: "vmux/test".into(),
                    base_ref: "main".into(),
                },
            ))
            .id();
        app.update();
        app.world_mut().resource_mut::<AutoSave>().dirty = false;
        app.world_mut().entity_mut(tab).remove::<TabWorktree>();

        app.update();

        assert!(app.world().resource::<AutoSave>().dirty);
    }

    #[test]
    fn sort_tabs_orders_by_order_field() {
        let a = Entity::from_bits(10);
        let b = Entity::from_bits(11);
        let c = Entity::from_bits(12);
        let input = vec![
            (a, Some(2u32), Some(100i64)),
            (b, Some(0), Some(200)),
            (c, Some(1), Some(50)),
        ];
        assert_eq!(sort_tabs_by_order(input), vec![b, c, a]);
    }

    #[test]
    fn sort_tabs_legacy_falls_back_to_created_at() {
        let a = Entity::from_bits(10);
        let b = Entity::from_bits(11);
        let c = Entity::from_bits(12);
        let input = vec![
            (a, None, Some(2i64)),
            (b, None, Some(3)),
            (c, None, Some(1)),
        ];
        assert_eq!(sort_tabs_by_order(input), vec![c, a, b]);
    }

    #[test]
    fn sort_tabs_ordered_before_unordered() {
        let ordered = Entity::from_bits(1);
        let legacy = Entity::from_bits(2);
        let input = vec![(legacy, None, Some(0i64)), (ordered, Some(5u32), Some(999))];
        assert_eq!(sort_tabs_by_order(input), vec![ordered, legacy]);
    }

    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    struct HomeEnvGuard {
        _guard: std::sync::MutexGuard<'static, ()>,
        old_home: Option<std::ffi::OsString>,
        old_tmpdir: Option<std::ffi::OsString>,
    }

    impl HomeEnvGuard {
        fn use_temp_home(name: &str) -> Self {
            let guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
            let old_home = std::env::var_os("HOME");
            let old_tmpdir = std::env::var_os("TMPDIR");
            let home =
                std::env::temp_dir().join(format!("vmux-test-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&home);
            std::fs::create_dir_all(&home).expect("create temp home");
            unsafe {
                std::env::set_var("HOME", &home);
                std::env::set_var("TMPDIR", &home);
            }
            Self {
                _guard: guard,
                old_home,
                old_tmpdir,
            }
        }
    }

    impl Drop for HomeEnvGuard {
        fn drop(&mut self) {
            unsafe {
                match &self.old_home {
                    Some(home) => std::env::set_var("HOME", home),
                    None => std::env::remove_var("HOME"),
                }
                match &self.old_tmpdir {
                    Some(tmpdir) => std::env::set_var("TMPDIR", tmpdir),
                    None => std::env::remove_var("TMPDIR"),
                }
            }
        }
    }

    fn test_settings() -> AppSettings {
        AppSettings {
            browser: BrowserSettings {
                startup_url: "about:blank".to_string(),
                ..Default::default()
            },
            layout: LayoutSettings {
                radius: 0.0,
                window: WindowSettings { padding: 0.0 },
                pane: PaneSettings { gap: 0.0 },
                side_sheet: SideSheetSettings::default(),
                focus_ring: FocusRingSettings::default(),
            },
            shortcuts: ShortcutSettings::default(),
            terminal: None,
            auto_update: false,
            agent: vmux_setting::AgentSettings::default(),
            spaces: Default::default(),
            recording: Default::default(),
            editor: Default::default(),
            appearance: Default::default(),
        }
    }

    #[test]
    fn persisted_terminal_tab_reattaches_saved_process() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(test_settings())
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<WebviewExtendStandardMaterial>>()
            .init_resource::<vmux_agent::strategy::AgentStrategies>()
            .add_message::<vmux_core::agent::SpawnAgentInStackRequest>()
            .add_systems(Update, rebuild_space_views);

        let main = app.world_mut().spawn(Main).id();
        app.world_mut().spawn(PrimaryWindow);
        let space = app.world_mut().spawn((Tab::default(), ChildOf(main))).id();
        let pane = app.world_mut().spawn((Pane, ChildOf(space))).id();
        let saved_url = format!(
            "{}{}",
            TERMINAL_PAGE_URL,
            uuid::Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap()
        );
        let tab = app
            .world_mut()
            .spawn((
                Stack::default(),
                PageMetadata {
                    title: "Terminal".to_string(),
                    url: saved_url.clone(),
                    icon: vmux_core::PageIcon::None,
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
        assert_eq!(meta.url, TERMINAL_PAGE_URL);
    }

    #[test]
    fn url_and_visit_round_trip() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("test_history.ron");

        let mut app_save = App::new();
        app_save.add_plugins(MinimalPlugins);
        app_save.add_plugins(vmux_core::CorePlugin);
        app_save.add_observer(save_on_default_event);

        let url_e = app_save
            .world_mut()
            .spawn((
                Save,
                vmux_core::Url,
                PageMetadata {
                    url: "https://example.com".into(),
                    title: "Example".into(),
                    icon: vmux_core::PageIcon::None,
                    bg_color: None,
                },
                vmux_core::VisitCount(3),
                vmux_core::LastVisitedAt(1000),
                vmux_core::CreatedAt(500),
            ))
            .id();

        app_save.world_mut().spawn((
            Save,
            vmux_core::Visit,
            vmux_core::VisitedUrl(url_e),
            vmux_core::CreatedAt(900),
            vmux_core::TransitionType::Typed,
        ));

        save_space_to_path(&mut app_save.world_mut().commands(), path.clone());
        app_save.update();

        assert!(path.exists(), "save file should exist");

        let mut app_load = App::new();
        app_load
            .add_plugins((MinimalPlugins, AssetPlugin::default()))
            .add_plugins(vmux_core::CorePlugin)
            .add_observer(load_on_default_event);
        app_load.update();

        app_load
            .world_mut()
            .commands()
            .trigger_load(LoadWorld::default_from_file(path));
        app_load.update();

        let url_count = app_load
            .world_mut()
            .query::<&vmux_core::Url>()
            .iter(app_load.world())
            .count();
        let visit_count = app_load
            .world_mut()
            .query::<&vmux_core::Visit>()
            .iter(app_load.world())
            .count();
        assert_eq!(url_count, 1, "Url not round-tripped");
        assert_eq!(visit_count, 1, "Visit not round-tripped");

        let (vc, lva, ca) = app_load
            .world_mut()
            .query::<(
                &vmux_core::VisitCount,
                &vmux_core::LastVisitedAt,
                &vmux_core::CreatedAt,
            )>()
            .iter(app_load.world())
            .find(|(vc, _, _)| vc.0 == 3)
            .expect("Url entity fields not round-tripped");
        assert_eq!(vc.0, 3);
        assert_eq!(lva.0, 1000);
        assert_eq!(ca.0, 500);

        let tt = app_load
            .world_mut()
            .query::<&vmux_core::TransitionType>()
            .iter(app_load.world())
            .next()
            .expect("TransitionType not round-tripped");
        assert_eq!(*tt, vmux_core::TransitionType::Typed);
    }

    #[test]
    fn window_geometry_round_trips_through_store() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("store.ron");

        let mut app_save = App::new();
        app_save.add_plugins(MinimalPlugins);
        app_save.add_plugins(vmux_core::CorePlugin);
        app_save
            .register_type::<WindowGeometry>()
            .register_type::<Option<IVec2>>()
            .register_type::<Option<Vec2>>();
        app_save.add_observer(save_on_default_event);
        app_save.world_mut().spawn((
            Save,
            WindowGeometry {
                fullscreen: true,
                position: Some(IVec2::new(11, 22)),
                size: Some(Vec2::new(640.0, 480.0)),
            },
        ));

        save_space_to_path(&mut app_save.world_mut().commands(), path.clone());
        app_save.update();
        assert!(path.exists(), "store file should exist");

        let mut app_load = App::new();
        app_load
            .add_plugins((MinimalPlugins, AssetPlugin::default()))
            .add_plugins(vmux_core::CorePlugin);
        app_load
            .register_type::<WindowGeometry>()
            .register_type::<Option<IVec2>>()
            .register_type::<Option<Vec2>>();
        app_load.add_observer(load_on_default_event);
        app_load.update();
        app_load
            .world_mut()
            .commands()
            .trigger_load(LoadWorld::default_from_file(path));
        app_load.update();

        let geom = app_load
            .world_mut()
            .query::<&WindowGeometry>()
            .single(app_load.world())
            .expect("WindowGeometry not round-tripped");
        assert!(geom.fullscreen);
        assert_eq!(geom.position, Some(IVec2::new(11, 22)));
        assert_eq!(geom.size, Some(Vec2::new(640.0, 480.0)));
    }

    #[test]
    fn custom_save_writes_schema_version_next_to_saved_store() {
        let _home = HomeEnvGuard::use_temp_home("custom-save-schema-version");
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("custom-store.ron");

        let mut app_save = App::new();
        app_save
            .add_plugins(MinimalPlugins)
            .add_plugins(vmux_core::CorePlugin)
            .add_observer(save_on_default_event);
        app_save.world_mut().spawn((
            Save,
            Space,
            SpaceId("space-1".to_string()),
            WindowGeometry {
                fullscreen: false,
                position: None,
                size: None,
            },
        ));

        save_space_to_path(&mut app_save.world_mut().commands(), path.clone());
        app_save.update();

        assert!(path.exists(), "custom store should be saved");
        assert!(
            dir.path().join("store.version").exists(),
            "schema version should be written next to custom store"
        );
        assert!(
            !store_version_path().exists(),
            "custom save must not write default store.version"
        );
    }

    #[test]
    fn pane_id_and_position_round_trip_through_store() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("store.ron");

        let mut app_save = App::new();
        app_save.add_plugins(MinimalPlugins);
        app_save.add_plugins(vmux_core::CorePlugin);
        app_save.register_type::<PaneId>();
        app_save.add_observer(save_on_default_event);
        app_save
            .world_mut()
            .spawn((Save, Pane, PaneId("p-1".to_string())));
        app_save.world_mut().spawn((
            Save,
            ArchivedPage {
                url: "https://x".into(),
                ..default()
            },
            ArchivedPagePosition {
                leaf_pane_id: "p-1".into(),
                stack_index: 1,
                pane_path: vec![vmux_core::PaneStep {
                    split_id: "root".into(),
                    axis: vmux_core::SplitAxis::Column,
                    child_index: 2,
                    flex_weights: vec![1.0, 4.0],
                }],
            },
            ArchivedTabPage {
                group_id: "tab-group".into(),
                tab_name: "Recovered".into(),
                tab_startup_dir: Some("/tmp/recovered".into()),
                active: true,
            },
        ));
        save_space_to_path(&mut app_save.world_mut().commands(), path.clone());
        app_save.update();
        assert!(path.exists());

        let mut app_load = App::new();
        app_load
            .add_plugins((MinimalPlugins, AssetPlugin::default()))
            .add_plugins(vmux_core::CorePlugin)
            .register_type::<PaneId>()
            .add_observer(load_on_default_event);
        app_load.update();
        app_load
            .world_mut()
            .commands()
            .trigger_load(LoadWorld::default_from_file(path));
        app_load.update();

        let pid = app_load
            .world_mut()
            .query::<&PaneId>()
            .single(app_load.world())
            .expect("PaneId round-tripped");
        assert_eq!(pid.0, "p-1");
        let pos = app_load
            .world_mut()
            .query::<&ArchivedPagePosition>()
            .single(app_load.world())
            .expect("position round-tripped");
        assert_eq!(pos.leaf_pane_id, "p-1");
        assert_eq!(pos.pane_path[0].child_index, 2);
        assert!(matches!(
            pos.pane_path[0].axis,
            vmux_core::SplitAxis::Column
        ));
        let tab = app_load
            .world_mut()
            .query::<&ArchivedTabPage>()
            .single(app_load.world())
            .expect("tab archive metadata round-tripped");
        assert_eq!(tab.group_id, "tab-group");
        assert_eq!(tab.tab_name, "Recovered");
        assert_eq!(tab.tab_startup_dir.as_deref(), Some("/tmp/recovered"));
        assert!(tab.active);
    }

    #[test]
    fn runtime_loaded_space_rebuilds_browser_views() {
        let _home = HomeEnvGuard::use_temp_home("runtime-loaded-space-rebuilds-browser-views");
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(test_settings())
            .insert_resource(ActiveSpace {
                record: vmux_space::model::bootstrap_space_record(),
            })
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<WebviewExtendStandardMaterial>>()
            .init_resource::<vmux_agent::strategy::AgentStrategies>()
            .add_plugins(PersistencePlugin);

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
                    icon: vmux_core::PageIcon::Favicon(
                        "https://example.com/favicon.ico".to_string(),
                    ),
                    bg_color: Some("#123456".to_string()),
                },
                ChildOf(pane),
            ))
            .id();

        app.world_mut().trigger(Loaded {
            entity_map: EntityHashMap::default(),
        });
        app.update();

        let children = app.world().get::<Children>(tab).unwrap();
        let browser = children
            .iter()
            .find(|entity| app.world().entity(*entity).contains::<Browser>())
            .expect("browser child");
        let meta = app.world().get::<PageMetadata>(browser).unwrap();
        assert_eq!(meta.title, "Example");
        assert_eq!(meta.url, "https://example.com");
        assert_eq!(
            meta.icon,
            vmux_core::PageIcon::Favicon("https://example.com/favicon.ico".to_string())
        );
        assert_eq!(meta.bg_color.as_deref(), Some("#123456"));
    }

    #[test]
    fn current_page_agent_url_does_not_mark_space_stale() {
        assert!(!space_contains_stale_agent_url(
            r#"url: "vmux://agent/echo/echo/edb5335d-20cf-4c3d-9433-8619c405a0f2""#
        ));
    }

    #[test]
    fn known_cli_agent_url_does_not_mark_space_stale() {
        assert!(!space_contains_stale_agent_url(
            r#"url: "vmux://agent/codex/edb5335d-20cf-4c3d-9433-8619c405a0f2""#
        ));
    }

    #[test]
    fn bare_cli_agent_url_does_not_mark_space_stale() {
        assert!(!space_contains_stale_agent_url(
            r#"url: "vmux://agent/vibe/""#
        ));
    }

    #[test]
    fn malformed_agent_url_marks_space_stale() {
        // Under the ACP grammar `vmux://agent/<id>/<sid>` is a valid session url for any id, so an
        // unknown id is no longer stale-by-parse (the runtime handler errors gracefully for an
        // unconfigured agent). Only genuinely malformed urls (too many segments) are stale.
        assert!(!space_contains_stale_agent_url(
            r#"url: "vmux://agent/bogus/edb5335d-20cf-4c3d-9433-8619c405a0f2""#
        ));
        assert!(space_contains_stale_agent_url(
            r#"url: "vmux://agent/a/b/c/d/e""#
        ));
    }

    #[test]
    fn current_page_agent_space_file_is_not_removed_before_load() {
        let dir = tempfile::tempdir().expect("tempdir");
        let space_dir = dir.path().join("profiles/personal/spaces/space-1");
        std::fs::create_dir_all(&space_dir).expect("space dir");
        let path = space_dir.join("space.ron");
        std::fs::write(
            &path,
            r#"url: "vmux://agent/echo/echo/edb5335d-20cf-4c3d-9433-8619c405a0f2""#,
        )
        .expect("write space");

        assert!(!remove_stale_space_if_needed(&path));
        assert!(path.exists());
        assert!(space_dir.exists());
    }

    #[test]
    fn prompt_only_empty_url_space_is_removed_before_load() {
        let dir = tempfile::tempdir().expect("tempdir");
        let space_dir = dir.path().join("profiles/personal/spaces/space-1");
        std::fs::create_dir_all(&space_dir).expect("space dir");
        let path = space_dir.join("space.ron");
        std::fs::write(
            &path,
            r#"
(
  resources: {},
  entities: {
    1: (
      components: {
        "vmux_desktop::layout::stack::Stack": (
          scroll_x: 0.0,
          scroll_y: 0.0,
        ),
        "vmux_header::system::PageMetadata": (
          title: "",
          url: "",
          icon: None,
          bg_color: None,
        ),
      },
    ),
  },
)
"#,
        )
        .expect("write prompt-only space");

        assert!(remove_stale_space_if_needed(&path));
        assert!(!path.exists());
        assert!(space_dir.exists());
    }

    fn registry_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(vmux_core::CorePlugin);
        app
    }

    fn store_body_with_key(key: &str) -> String {
        format!(
            "(\n  resources: {{}},\n  entities: {{\n    1: (\n      components: {{\n        \"{key}\": (),\n      }},\n    ),\n  }},\n)\n"
        )
    }

    #[test]
    fn store_with_unregistered_component_type_is_incompatible() {
        let app = registry_app();
        let registry = app.world().resource::<AppTypeRegistry>().read();
        let body = store_body_with_key("vmux_desktop::ghost::DoesNotExist");
        assert!(space_has_unregistered_types(&body, &registry));
    }

    #[test]
    fn store_with_registered_component_types_is_compatible() {
        let app = registry_app();
        let registry = app.world().resource::<AppTypeRegistry>().read();
        let key = <vmux_core::PageMetadata as bevy::reflect::TypePath>::type_path();
        let body = store_body_with_key(key);
        assert!(!space_has_unregistered_types(&body, &registry));
    }

    #[test]
    fn incompatible_store_is_removed_before_load() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("store.ron");
        std::fs::write(dir.path().join("store.version"), "2").expect("write version");
        std::fs::write(
            &path,
            store_body_with_key("vmux_desktop::ghost::DoesNotExist"),
        )
        .expect("write store");

        let app = registry_app();
        let registry = app.world().resource::<AppTypeRegistry>().read();
        assert!(remove_incompatible_store_if_needed(&path, &registry));
        assert!(!path.exists());
        assert!(!dir.path().join("store.version").exists());
    }

    #[test]
    fn compatible_store_is_kept_before_load() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("store.ron");
        let app = registry_app();
        let registry = app.world().resource::<AppTypeRegistry>().read();
        let key = <vmux_core::PageMetadata as bevy::reflect::TypePath>::type_path();
        std::fs::write(&path, store_body_with_key(key)).expect("write store");

        assert!(!remove_incompatible_store_if_needed(&path, &registry));
        assert!(path.exists());
    }

    #[test]
    fn incompatible_store_resets_layout_on_startup() {
        let _home = HomeEnvGuard::use_temp_home("incompatible-store-resets-layout-on-startup");
        let path = store_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("store dir");
        }
        std::fs::write(
            &path,
            store_body_with_key("vmux_desktop::ghost::DoesNotExist"),
        )
        .expect("write store");
        std::fs::write(store_version_path(), STORE_SCHEMA_VERSION.to_string())
            .expect("write version");

        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(test_settings())
            .insert_resource(ActiveSpace {
                record: vmux_space::model::bootstrap_space_record(),
            })
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<WebviewExtendStandardMaterial>>()
            .init_resource::<vmux_agent::strategy::AgentStrategies>()
            .add_plugins(PersistencePlugin);
        app.world_mut().spawn(Main);
        app.world_mut().spawn(PrimaryWindow);
        app.update();

        assert!(
            !path.exists(),
            "incompatible store should be removed on startup"
        );
        assert!(
            !store_version_path().exists(),
            "store.version should be removed with the incompatible store"
        );
        let spaces = app.world_mut().query::<&Space>().iter(app.world()).count();
        assert_eq!(spaces, 1, "a fresh space should be spawned after reset");
    }

    #[test]
    fn auto_save_system_skips_save_without_space() {
        let _home = HomeEnvGuard::use_temp_home("auto-save-system-skips-without-space");
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(vmux_core::CorePlugin)
            .register_type::<WindowGeometry>()
            .register_type::<Option<IVec2>>()
            .register_type::<Option<Vec2>>()
            .insert_resource(AutoSave {
                debounce: Timer::from_seconds(0.0, TimerMode::Once),
                periodic: Timer::from_seconds(0.0, TimerMode::Repeating),
                dirty: true,
            })
            .add_observer(save_on_default_event)
            .add_systems(Update, auto_save_system);
        app.world_mut().spawn((
            Save,
            WindowGeometry {
                fullscreen: false,
                position: None,
                size: None,
            },
        ));
        app.update();
        app.update();
        assert!(
            !store_path().exists(),
            "auto_save must skip when no Space exists"
        );
    }

    #[test]
    fn auto_save_system_saves_with_space() {
        let _home = HomeEnvGuard::use_temp_home("auto-save-system-saves-with-space");
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(vmux_core::CorePlugin)
            .register_type::<WindowGeometry>()
            .register_type::<Option<IVec2>>()
            .register_type::<Option<Vec2>>()
            .insert_resource(AutoSave {
                debounce: Timer::from_seconds(0.0, TimerMode::Once),
                periodic: Timer::from_seconds(0.0, TimerMode::Repeating),
                dirty: true,
            })
            .add_observer(save_on_default_event)
            .add_systems(Update, auto_save_system);
        app.world_mut().spawn((
            Save,
            Space,
            SpaceId("space-1".to_string()),
            WindowGeometry {
                fullscreen: false,
                position: None,
                size: None,
            },
        ));
        app.update();
        app.update();
        assert!(
            store_path().exists(),
            "auto_save must save when a Space exists"
        );
    }
}
