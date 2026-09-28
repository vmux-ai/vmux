use bevy::ecs::relationship::Relationship;
use bevy::ecs::schedule::common_conditions::any_with_component;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_cef::prelude::*;
use bevy_world_serialization::WorldFilter;
use moonshine_save::prelude::*;
use moonshine_save::save::EntityFilter;
use std::path::{Path, PathBuf};

#[cfg(test)]
use vmux_core::host::persistence::PersistenceAppExt;
use vmux_core::host::persistence::{PersistenceDirty, persisted_components};
#[cfg(test)]
use vmux_core::{ArchivedPage, ArchivedPagePosition, ArchivedTabPage, PageMetadata};
#[cfg(test)]
use vmux_layout::profile::Profile;
use vmux_layout::space::Space;
#[cfg(test)]
use vmux_layout::space::SpaceId;
use vmux_layout::{LayoutPersistenceSet, LayoutStartupSet, SpaceFilePresent};
#[cfg(test)]
use vmux_layout::{
    Open,
    pane::{Pane, PaneId, PaneSize, PaneSplit},
    stack::Stack,
    tab::Tab,
    tab::{TabDirDecided, TabWorkspace, TabWorktree},
    window::{Main, WindowGeometry},
};
#[cfg(test)]
use vmux_setting::AppSettings;

pub(crate) struct PersistencePlugin;

impl Plugin for PersistencePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<vmux_space::SaveSpaceRequest>()
            .add_observer(save_on_default_event)
            .add_observer(load_on_default_event)
            .add_systems(
                Startup,
                (
                    spawn_space_persistence,
                    ApplyDeferred,
                    load_space_on_startup,
                )
                    .chain()
                    .in_set(LayoutStartupSet::Persistence),
            )
            .add_observer(mark_restore_pending)
            .add_observer(mark_persistence_dirty)
            .add_systems(
                Update,
                complete_restore
                    .after(LayoutPersistenceSet::Restore)
                    .run_if(any_with_component::<RestorePending>),
            )
            .add_systems(Update, (auto_save_system, handle_save_space_requests));
    }
}

fn spawn_space_persistence(registry: Res<AppTypeRegistry>, mut commands: Commands) {
    let components = persisted_components(&registry.read())
        .allow::<Save>()
        .allow::<ChildOf>()
        .allow::<Children>()
        .allow::<Name>();
    commands.spawn((
        Name::new("Space persistence"),
        AutoSave {
            debounce: Timer::from_seconds(0.5, TimerMode::Once),
            periodic: Timer::from_seconds(60.0, TimerMode::Repeating),
            dirty: false,
            components,
        },
    ));
}

fn handle_save_space_requests(
    mut requests: MessageReader<vmux_space::SaveSpaceRequest>,
    save_entities: SpaceSaveEntities,
    persistence: Single<&AutoSave>,
    mut commands: Commands,
) {
    for request in requests.read() {
        save_space_to_path_excluding(
            &mut commands,
            request.path.clone(),
            save_entities.excluded(),
            persistence.components.clone(),
        );
    }
}

#[derive(Component)]
struct RestorePending;

fn mark_restore_pending(
    _trigger: On<Loaded>,
    persistence: Single<Entity, With<AutoSave>>,
    mut commands: Commands,
) {
    commands.entity(*persistence).insert(RestorePending);
}

fn complete_restore(
    mut restore: Single<&mut crate::boot_status::RestoreComplete>,
    persistence: Single<Entity, With<RestorePending>>,
    mut commands: Commands,
) {
    restore.0 = true;
    commands.entity(*persistence).remove::<RestorePending>();
}

#[derive(Component)]
struct AutoSave {
    debounce: Timer,
    periodic: Timer,
    dirty: bool,
    components: WorldFilter,
}

#[derive(bevy::ecs::system::SystemParam)]
struct SpaceSaveEntities<'w, 's> {
    saved: Query<'w, 's, Entity, With<Save>>,
    child_of: Query<'w, 's, &'static ChildOf>,
    host_windows: Query<'w, 's, &'static HostWindow>,
    windows: Query<'w, 's, (), With<Window>>,
    primary_window: Query<'w, 's, Entity, With<PrimaryWindow>>,
}

impl SpaceSaveEntities<'_, '_> {
    fn excluded(&self) -> Vec<Entity> {
        let Ok(primary_window) = self.primary_window.single() else {
            return Vec::new();
        };
        self.saved
            .iter()
            .filter(|entity| {
                if self.windows.contains(*entity) {
                    return *entity != primary_window;
                }
                vmux_layout::window::host_window_of(*entity, &self.child_of, &self.host_windows)
                    .is_some_and(|window| window != primary_window)
            })
            .collect()
    }
}

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

fn mark_persistence_dirty(_trigger: On<PersistenceDirty>, mut auto_save: Single<&mut AutoSave>) {
    auto_save.dirty = true;
    auto_save.debounce.reset();
}

fn auto_save_system(
    time: Res<Time>,
    mut auto_save: Single<&mut AutoSave>,
    spaces: Query<(), With<Space>>,
    save_entities: SpaceSaveEntities,
    mut commands: Commands,
) {
    auto_save.periodic.tick(time.delta());

    if spaces.is_empty() {
        return;
    }

    if auto_save.dirty {
        auto_save.debounce.tick(time.delta());
        if auto_save.debounce.is_finished() {
            save_space_to_path_excluding(
                &mut commands,
                store_path(),
                save_entities.excluded(),
                auto_save.components.clone(),
            );
            auto_save.dirty = false;
        }
    }

    if auto_save.periodic.just_finished() {
        save_space_to_path_excluding(
            &mut commands,
            store_path(),
            save_entities.excluded(),
            auto_save.components.clone(),
        );
    }
}

#[cfg(test)]
pub(crate) fn save_space_to_path(world: &mut World, path: PathBuf) {
    let components = persisted_components(&world.resource::<AppTypeRegistry>().read())
        .allow::<Save>()
        .allow::<ChildOf>()
        .allow::<Children>()
        .allow::<Name>();
    save_space_to_path_excluding(&mut world.commands(), path, std::iter::empty(), components);
}

fn save_space_to_path_excluding(
    commands: &mut Commands,
    path: PathBuf,
    excluded: impl IntoIterator<Item = Entity>,
    components: WorldFilter,
) {
    if vmux_core::profile::is_test_session() {
        return;
    }
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    write_store_schema_version(&path);
    let mut save = SaveWorld::default_into_file(path);
    save.entities = EntityFilter::block(excluded);
    save.components = components;
    commands.trigger_save(save);
}

pub(crate) fn load_space_on_startup(
    registry: Res<AppTypeRegistry>,
    mut restore: Single<(
        Entity,
        &mut crate::boot_status::RestoreComplete,
        &mut SpaceFilePresent,
    )>,
    mut commands: Commands,
) {
    let bootstrap = vmux_space::model::bootstrap_space_record();
    if vmux_core::profile::is_test_session() {
        restore.1.0 = true;
        restore.2.0 = false;
        commands.spawn(vmux_space::spaces::space_profile_bundle(&bootstrap));
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
    let exists = path.exists() && !removed_stale && !removed_incompatible && !schema_outdated;
    restore.2.0 = exists;
    if exists {
        info!("Loading space from {:?}", path);
        let load = match std::fs::read_to_string(&path)
            .ok()
            .and_then(|body| normalized_store_icons(&body))
        {
            Some((body, unknown)) => {
                warn!(?unknown, "Replacing unavailable persisted page icons");
                LoadWorld::default_from_stream(std::io::Cursor::new(body.into_bytes()))
            }
            None => LoadWorld::default_from_file(path),
        };
        commands.trigger_load(load);
    } else {
        restore.1.0 = true;
        commands.spawn(vmux_space::spaces::space_profile_bundle(&bootstrap));
    }
}

fn normalized_store_icons(body: &str) -> Option<(String, Vec<String>)> {
    let mut normalized = String::with_capacity(body.len());
    let mut unknown = Vec::new();

    for line in body.split_inclusive('\n') {
        let (content, newline) = line
            .strip_suffix('\n')
            .map(|content| (content, "\n"))
            .unwrap_or((line, ""));
        let trimmed = content.trim();
        let name = trimmed
            .strip_prefix("icon: Builtin(")
            .and_then(|value| value.strip_suffix("),"));
        let Some(name) = name else {
            normalized.push_str(line);
            continue;
        };
        if ron::from_str::<vmux_core::BuiltinIcon>(name).is_ok() {
            normalized.push_str(line);
            continue;
        }
        let indent = &content[..content.len() - content.trim_start().len()];
        normalized.push_str(indent);
        normalized.push_str("icon: r#None,");
        normalized.push_str(newline);
        unknown.push(name.to_string());
    }

    if unknown.is_empty() {
        None
    } else {
        Some((normalized, unknown))
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
    for prefix in ["vmux://sessions/", "vmux://agent/"] {
        if body.split(prefix).skip(1).any(|tail| {
            let suffix = tail.split('"').next().unwrap_or_default();
            let url = format!("{prefix}{suffix}");
            is_stale_agent_url(&url)
        }) {
            return true;
        }
    }
    false
}

fn is_stale_agent_url(url: &str) -> bool {
    let normalized = url.trim_end_matches('/');
    if matches!(normalized, "vmux://sessions" | "vmux://agent") {
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

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;
    use vmux_layout::settings::{
        FocusRingSettings, LayoutSettings, PaneSettings, SideSheetSettings, WindowSettings,
    };
    use vmux_setting::{AppSettings, BrowserSettings, ShortcutSettings};

    #[test]
    fn saved_components_keep_the_type_paths_stores_name_them_by() {
        use bevy::reflect::TypePath;

        let actual = vec![
            <ChildOf as TypePath>::type_path(),
            <Children as TypePath>::type_path(),
            <Name as TypePath>::type_path(),
            <Stack as TypePath>::type_path(),
            <Tab as TypePath>::type_path(),
            <TabWorkspace as TypePath>::type_path(),
            <TabWorktree as TypePath>::type_path(),
            <TabDirDecided as TypePath>::type_path(),
            <Pane as TypePath>::type_path(),
            <PaneSplit as TypePath>::type_path(),
            <PaneSize as TypePath>::type_path(),
            <Space as TypePath>::type_path(),
            <SpaceId as TypePath>::type_path(),
            <WindowGeometry as TypePath>::type_path(),
            <Profile as TypePath>::type_path(),
            <Open as TypePath>::type_path(),
            <PageMetadata as TypePath>::type_path(),
            <ArchivedPage as TypePath>::type_path(),
            <ArchivedPagePosition as TypePath>::type_path(),
            <ArchivedTabPage as TypePath>::type_path(),
            <PaneId as TypePath>::type_path(),
            <vmux_history::CreatedAt as TypePath>::type_path(),
            <vmux_history::LastActivatedAt as TypePath>::type_path(),
            <vmux_history::Visit as TypePath>::type_path(),
            <vmux_core::Url as TypePath>::type_path(),
            <vmux_core::VisitCount as TypePath>::type_path(),
            <vmux_core::LastVisitedAt as TypePath>::type_path(),
            <vmux_core::VisitedUrl as TypePath>::type_path(),
            <vmux_core::TransitionType as TypePath>::type_path(),
            <vmux_core::Order as TypePath>::type_path(),
            <vmux_editor::StackExplorerVisibility as TypePath>::type_path(),
            <vmux_terminal::launch::TerminalLaunch as TypePath>::type_path(),
        ];

        assert_eq!(
            actual,
            [
                "bevy_ecs::hierarchy::ChildOf",
                "bevy_ecs::hierarchy::Children",
                "bevy_ecs::name::Name",
                "vmux_desktop::layout::stack::Stack",
                "vmux_desktop::layout::tab::Tab",
                "vmux_desktop::layout::tab::TabWorkspace",
                "vmux_desktop::layout::tab::TabWorktree",
                "vmux_desktop::layout::tab::TabDirDecided",
                "vmux_desktop::layout::pane::Pane",
                "vmux_desktop::layout::pane::PaneSplit",
                "vmux_desktop::layout::pane::PaneSize",
                "vmux_desktop::space::Space",
                "vmux_desktop::space::SpaceId",
                "vmux_desktop::layout::window::WindowGeometry",
                "vmux_desktop::profile::Profile",
                "vmux_desktop::layout::Open",
                "vmux_header::system::PageMetadata",
                "vmux_core::archive::ArchivedPage",
                "vmux_core::archive::ArchivedPagePosition",
                "vmux_core::archive::ArchivedTabPage",
                "vmux_desktop::layout::pane::PaneId",
                "vmux_history::CreatedAt",
                "vmux_history::LastActivatedAt",
                "vmux_history::Visit",
                "vmux_history::Url",
                "vmux_history::VisitCount",
                "vmux_history::LastVisitedAt",
                "vmux_history::VisitedUrl",
                "vmux_history::TransitionType",
                "vmux_core::Order",
                "vmux_editor::plugin::StackExplorerVisibility",
                "vmux_core::terminal::TerminalLaunch",
            ]
        );
    }

    #[test]
    fn adding_archived_page_marks_store_dirty() {
        let mut app = App::new();
        let auto_save = app
            .world_mut()
            .spawn(AutoSave {
                debounce: Timer::from_seconds(0.5, TimerMode::Once),
                periodic: Timer::from_seconds(60.0, TimerMode::Repeating),
                dirty: false,
                components: WorldFilter::allow_all(),
            })
            .id();
        app.register_persisted::<ArchivedPage>()
            .add_observer(mark_persistence_dirty);
        app.update();
        app.world_mut()
            .get_mut::<AutoSave>(auto_save)
            .unwrap()
            .dirty = false;
        app.world_mut().spawn(ArchivedPage::default());
        app.update();
        assert!(app.world().get::<AutoSave>(auto_save).unwrap().dirty);
    }

    #[test]
    fn adding_visit_marks_store_dirty() {
        let mut app = App::new();
        let auto_save = app
            .world_mut()
            .spawn(AutoSave {
                debounce: Timer::from_seconds(0.5, TimerMode::Once),
                periodic: Timer::from_seconds(60.0, TimerMode::Repeating),
                dirty: false,
                components: WorldFilter::allow_all(),
            })
            .id();
        app.register_persisted::<vmux_history::Visit>()
            .add_observer(mark_persistence_dirty);
        app.update();
        app.world_mut()
            .get_mut::<AutoSave>(auto_save)
            .unwrap()
            .dirty = false;
        app.world_mut().spawn(vmux_history::Visit);
        app.update();
        assert!(app.world().get::<AutoSave>(auto_save).unwrap().dirty);
    }

    #[test]
    fn changing_stack_explorer_visibility_marks_store_dirty() {
        let mut app = App::new();
        let auto_save = app
            .world_mut()
            .spawn(AutoSave {
                debounce: Timer::from_seconds(0.5, TimerMode::Once),
                periodic: Timer::from_seconds(60.0, TimerMode::Repeating),
                dirty: false,
                components: WorldFilter::allow_all(),
            })
            .id();
        app.register_persisted::<vmux_editor::StackExplorerVisibility>()
            .add_observer(mark_persistence_dirty);
        let stack = app
            .world_mut()
            .spawn(vmux_editor::StackExplorerVisibility { visible: false })
            .id();
        app.update();
        app.world_mut()
            .get_mut::<AutoSave>(auto_save)
            .unwrap()
            .dirty = false;
        app.world_mut()
            .get_mut::<vmux_editor::StackExplorerVisibility>(stack)
            .unwrap()
            .visible = true;

        app.update();

        assert!(app.world().get::<AutoSave>(auto_save).unwrap().dirty);
    }

    #[test]
    fn changing_tab_startup_dir_marks_store_dirty() {
        let mut app = App::new();
        let auto_save = app
            .world_mut()
            .spawn(AutoSave {
                debounce: Timer::from_seconds(0.5, TimerMode::Once),
                periodic: Timer::from_seconds(60.0, TimerMode::Repeating),
                dirty: false,
                components: WorldFilter::allow_all(),
            })
            .id();
        app.register_persisted::<Tab>()
            .add_observer(mark_persistence_dirty);
        let tab = app.world_mut().spawn(Tab::default()).id();
        app.update();
        app.world_mut()
            .get_mut::<AutoSave>(auto_save)
            .unwrap()
            .dirty = false;
        app.world_mut()
            .entity_mut(tab)
            .get_mut::<Tab>()
            .unwrap()
            .startup_dir = Some("/tmp/rebound".into());

        app.update();

        assert!(app.world().get::<AutoSave>(auto_save).unwrap().dirty);
    }

    #[test]
    fn adding_tab_workspace_marks_store_dirty() {
        let mut app = App::new();
        let auto_save = app
            .world_mut()
            .spawn(AutoSave {
                debounce: Timer::from_seconds(0.5, TimerMode::Once),
                periodic: Timer::from_seconds(60.0, TimerMode::Repeating),
                dirty: false,
                components: WorldFilter::allow_all(),
            })
            .id();
        app.register_persisted::<TabWorkspace>()
            .add_observer(mark_persistence_dirty);
        let tab = app.world_mut().spawn(Tab::default()).id();
        app.update();
        app.world_mut()
            .get_mut::<AutoSave>(auto_save)
            .unwrap()
            .dirty = false;
        app.world_mut().entity_mut(tab).insert(TabWorkspace {
            project_dir: "/tmp/project".into(),
        });

        app.update();

        assert!(app.world().get::<AutoSave>(auto_save).unwrap().dirty);
    }

    #[test]
    fn removing_tab_worktree_marks_store_dirty() {
        let mut app = App::new();
        let auto_save = app
            .world_mut()
            .spawn(AutoSave {
                debounce: Timer::from_seconds(0.5, TimerMode::Once),
                periodic: Timer::from_seconds(60.0, TimerMode::Repeating),
                dirty: false,
                components: WorldFilter::allow_all(),
            })
            .id();
        app.register_persisted::<Tab>()
            .register_persisted::<TabWorktree>()
            .add_observer(mark_persistence_dirty);
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
        app.world_mut()
            .get_mut::<AutoSave>(auto_save)
            .unwrap()
            .dirty = false;
        app.world_mut().entity_mut(tab).remove::<TabWorktree>();

        app.update();

        assert!(app.world().get::<AutoSave>(auto_save).unwrap().dirty);
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
            update_channel: Default::default(),
            agent: vmux_setting::AgentSettings::default(),
            spaces: Default::default(),
            projects: Default::default(),
            recording: Default::default(),
            editor: Default::default(),
            appearance: Default::default(),
        }
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

        save_space_to_path(app_save.world_mut(), path.clone());
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
    fn persisted_git_branch_icon_still_loads() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("legacy-git-icon.ron");

        let mut app_save = App::new();
        app_save
            .add_plugins(MinimalPlugins)
            .add_plugins(vmux_core::CorePlugin)
            .add_observer(save_on_default_event);
        app_save.world_mut().spawn((
            Save,
            PageMetadata {
                title: "Git".into(),
                url: "vmux://git/".into(),
                icon: vmux_core::PageIcon::Builtin(vmux_core::BuiltinIcon::Project),
                bg_color: None,
            },
        ));
        save_space_to_path(app_save.world_mut(), path.clone());
        app_save.update();

        let current = std::fs::read_to_string(&path).expect("saved store");
        let legacy = current.replacen("Builtin(Project)", "Builtin(GitBranch)", 1);
        assert_ne!(legacy, current, "project icon serialized differently");
        std::fs::write(&path, legacy).expect("legacy store");

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

        let metadata = app_load
            .world_mut()
            .query::<&PageMetadata>()
            .single(app_load.world())
            .expect("legacy page metadata loaded");
        assert_eq!(
            metadata.icon,
            vmux_core::PageIcon::Builtin(vmux_core::BuiltinIcon::GitBranch)
        );
    }

    #[test]
    fn stack_explorer_visibility_round_trips_through_store() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("store.ron");

        let mut app_save = App::new();
        app_save
            .add_plugins(MinimalPlugins)
            .add_plugins(vmux_core::CorePlugin)
            .register_persisted::<vmux_editor::StackExplorerVisibility>()
            .add_observer(save_on_default_event);
        app_save
            .world_mut()
            .spawn((Save, vmux_editor::StackExplorerVisibility { visible: true }));
        save_space_to_path(app_save.world_mut(), path.clone());
        app_save.update();

        let mut app_load = App::new();
        app_load
            .add_plugins((MinimalPlugins, AssetPlugin::default()))
            .add_plugins(vmux_core::CorePlugin)
            .register_type::<vmux_editor::StackExplorerVisibility>()
            .add_observer(load_on_default_event);
        app_load.update();
        app_load
            .world_mut()
            .commands()
            .trigger_load(LoadWorld::default_from_file(path));
        app_load.update();

        let visibility = app_load
            .world_mut()
            .query::<&vmux_editor::StackExplorerVisibility>()
            .single(app_load.world())
            .expect("stack explorer visibility round-tripped");
        assert!(visibility.visible);
    }

    #[test]
    fn legacy_side_sheet_sections_load_through_store() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("store.ron");
        std::fs::write(
            &path,
            r#"(
  resources: {},
  entities: {
    1: (
      components: {
        "vmux_desktop::layout::side_sheet::SideSheetSectionsExpanded": (
          projects: true,
          bookmarks: true,
          knowledge: true,
          tools: true,
        ),
      },
    ),
  },
)
"#,
        )
        .expect("write store");

        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .add_plugins(vmux_core::CorePlugin)
            .register_type::<vmux_layout::side_sheet::SideSheetSectionsExpanded>()
            .add_observer(load_on_default_event);
        app.update();
        app.world_mut()
            .commands()
            .trigger_load(LoadWorld::default_from_file(path));
        app.update();

        let sections = app
            .world_mut()
            .query::<&vmux_layout::side_sheet::SideSheetSectionsExpanded>()
            .single(app.world())
            .expect("legacy side sheet sections loaded");
        assert!(sections.bookmarks);
    }

    #[test]
    fn window_geometry_round_trips_through_store() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("store.ron");

        let mut app_save = App::new();
        app_save.add_plugins(MinimalPlugins);
        app_save.add_plugins(vmux_core::CorePlugin);
        app_save
            .register_persisted::<WindowGeometry>()
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

        save_space_to_path(app_save.world_mut(), path.clone());
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
    fn secondary_window_views_are_excluded_from_store() {
        let mut app = App::new();
        let primary_window = app
            .world_mut()
            .spawn((Window::default(), PrimaryWindow, Save))
            .id();
        let secondary_window = app.world_mut().spawn((Window::default(), Save)).id();
        let primary_root = app.world_mut().spawn(HostWindow(primary_window)).id();
        let secondary_root = app.world_mut().spawn(HostWindow(secondary_window)).id();
        let primary_space = app
            .world_mut()
            .spawn((Save, Space, ChildOf(primary_root)))
            .id();
        let secondary_space = app
            .world_mut()
            .spawn((Save, Space, ChildOf(secondary_root)))
            .id();
        let global = app.world_mut().spawn(Save).id();

        let excluded = app
            .world_mut()
            .run_system_once(|entities: SpaceSaveEntities| entities.excluded())
            .unwrap();

        assert!(excluded.contains(&secondary_window));
        assert!(excluded.contains(&secondary_space));
        assert!(!excluded.contains(&primary_window));
        assert!(!excluded.contains(&primary_space));
        assert!(!excluded.contains(&global));
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
            .register_persisted::<Space>()
            .register_persisted::<SpaceId>()
            .register_persisted::<WindowGeometry>()
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

        save_space_to_path(app_save.world_mut(), path.clone());
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
        app_save.register_persisted::<PaneId>();
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
        save_space_to_path(app_save.world_mut(), path.clone());
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
    fn current_page_agent_url_does_not_mark_space_stale() {
        assert!(!space_contains_stale_agent_url(
            r#"url: "vmux://sessions/echo/echo/edb5335d-20cf-4c3d-9433-8619c405a0f2""#
        ));
    }

    #[test]
    fn legacy_agent_url_does_not_mark_space_stale() {
        assert!(!space_contains_stale_agent_url(
            r#"url: "vmux://agent/claude/session-id""#
        ));
    }

    #[test]
    fn known_cli_agent_url_does_not_mark_space_stale() {
        assert!(!space_contains_stale_agent_url(
            r#"url: "vmux://sessions/codex/edb5335d-20cf-4c3d-9433-8619c405a0f2""#
        ));
    }

    #[test]
    fn bare_cli_agent_url_does_not_mark_space_stale() {
        assert!(!space_contains_stale_agent_url(
            r#"url: "vmux://sessions/vibe/""#
        ));
    }

    #[test]
    fn malformed_agent_url_marks_space_stale() {
        assert!(!space_contains_stale_agent_url(
            r#"url: "vmux://sessions/bogus/edb5335d-20cf-4c3d-9433-8619c405a0f2""#
        ));
        assert!(space_contains_stale_agent_url(
            r#"url: "vmux://sessions/a/b/c/d/e""#
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
            r#"url: "vmux://sessions/echo/echo/edb5335d-20cf-4c3d-9433-8619c405a0f2""#,
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
    fn unavailable_persisted_icons_fall_back_without_resetting_the_store() {
        let body = r#"
        icon: Builtin(Files),
        icon: Builtin(LegacyProject),
        icon: Builtin(LegacyKeyboard),
"#;
        let (normalized, unknown) = normalized_store_icons(body).expect("unknown icons");

        assert_eq!(unknown, ["LegacyProject", "LegacyKeyboard"]);
        assert_eq!(
            normalized,
            r#"
        icon: Builtin(Files),
        icon: r#None,
        icon: r#None,
"#
        );
    }

    #[test]
    fn available_persisted_icons_need_no_migration() {
        let body = "        icon: Builtin(GitBranch),\n";
        assert_eq!(normalized_store_icons(body), None);
    }

    #[test]
    fn unavailable_persisted_icon_store_loads_after_migration() {
        let body = r#"(
  resources: {},
  entities: {
    1: (
      components: {
        "vmux_header::system::PageMetadata": (
          title: "Projects",
          url: "vmux://projects/",
          icon: Builtin(LegacyProject),
          bg_color: None,
        ),
      },
    ),
  },
)
"#;
        let (normalized, _) = normalized_store_icons(body).expect("unknown icon");
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .add_plugins(vmux_core::CorePlugin)
            .add_observer(load_on_default_event);
        app.update();
        app.world_mut()
            .commands()
            .trigger_load(LoadWorld::default_from_stream(std::io::Cursor::new(
                normalized.into_bytes(),
            )));
        app.update();

        let metadata = app
            .world_mut()
            .query::<&PageMetadata>()
            .single(app.world())
            .expect("page metadata loaded");
        assert_eq!(metadata.icon, vmux_core::PageIcon::None);
        assert_eq!(metadata.url, "vmux://projects/");
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
        app.world_mut()
            .spawn(crate::boot_status::RestoreComplete::default());
        app.add_plugins(MinimalPlugins)
            .insert_resource(test_settings())
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
        app.world_mut().spawn(AutoSave {
            debounce: Timer::from_seconds(0.0, TimerMode::Once),
            periodic: Timer::from_seconds(0.0, TimerMode::Repeating),
            dirty: true,
            components: WorldFilter::allow_all(),
        });
        app.add_plugins(MinimalPlugins)
            .add_plugins(vmux_core::CorePlugin)
            .register_persisted::<WindowGeometry>()
            .register_type::<Option<IVec2>>()
            .register_type::<Option<Vec2>>()
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
        app.world_mut().spawn(AutoSave {
            debounce: Timer::from_seconds(0.0, TimerMode::Once),
            periodic: Timer::from_seconds(0.0, TimerMode::Repeating),
            dirty: true,
            components: WorldFilter::allow_all(),
        });
        app.add_plugins(MinimalPlugins)
            .add_plugins(vmux_core::CorePlugin)
            .register_persisted::<WindowGeometry>()
            .register_type::<Option<IVec2>>()
            .register_type::<Option<Vec2>>()
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
