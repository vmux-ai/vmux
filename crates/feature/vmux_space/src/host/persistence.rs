use bevy::ecs::schedule::common_conditions::any_with_component;
#[cfg(test)]
use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
#[cfg(test)]
use bevy_cef::prelude::HostWindow;
use bevy_world_serialization::WorldFilter;
use moonshine_save::prelude::*;
use moonshine_save::save::EntityFilter;
use std::collections::HashSet;
use std::path::PathBuf;

#[cfg(test)]
use vmux_ecs::persistence::PersistenceAppExt;
use vmux_ecs::persistence::{
    AtomicSaveWorld, DurableStore, PersistenceDirty, PersistenceRecovery, PersistenceState,
    RestorePhase, StoreLoad, TrackedLoadWorld, WorkspacePersisted, WorkspaceRestoreSet,
    WorkspaceRestoreValidator, WorkspaceSaveRequest, WorkspaceStore, WorkspaceStoreValidators,
    load_tracked_on,
};
#[cfg(test)]
use vmux_ecs::profile::ProfilePaths;
use vmux_ecs::profile::{CurrentProfile, SessionEnvironment};
#[cfg(test)]
use vmux_ecs::{ArchivedPage, ArchivedPagePosition, ArchivedTabPage, PageMetadata};
use vmux_layout::LayoutStartupSet;
use vmux_layout::space::Space;
#[cfg(test)]
use vmux_layout::space::SpaceId;
use vmux_layout::window::WindowHierarchy;
#[cfg(test)]
use vmux_layout::{
    pane::{Pane, PaneId},
    tab::{StartupDir, Tab, TabWorkspace, TabWorktree},
    window::WindowGeometry,
};
pub(crate) struct WorkspacePersistencePlugin;

impl Plugin for WorkspacePersistencePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<WorkspaceSaveRequest>()
            .add_observer(save_on::<AtomicSaveWorld>)
            .add_observer(load_on_default_event)
            .add_observer(load_tracked_on::<moonshine_save::load::DefaultUnloadFilter>)
            .add_systems(
                Startup,
                (spawn, ApplyDeferred, load_on_startup)
                    .chain()
                    .in_set(LayoutStartupSet::Persistence),
            )
            .add_observer(mark_restore_pending)
            .add_observer(mark_dirty)
            .configure_sets(
                PreUpdate,
                (
                    WorkspaceRestoreSet::Validate,
                    WorkspaceRestoreSet::Resolve,
                    WorkspaceRestoreSet::Ready,
                )
                    .chain(),
            )
            .add_systems(
                PreUpdate,
                validate_restore.in_set(WorkspaceRestoreSet::Validate),
            )
            .add_systems(
                PreUpdate,
                complete_restore
                    .in_set(WorkspaceRestoreSet::Ready)
                    .run_if(any_with_component::<RestorePending>),
            )
            .add_systems(Update, (auto_save, handle_save_requests));
    }
}

fn spawn(registry: Res<AppTypeRegistry>, profile: CurrentProfile, mut commands: Commands) {
    let components = WorkspacePersisted::filter(&registry.read())
        .allow::<Save>()
        .allow::<ChildOf>()
        .allow::<Children>()
        .allow::<Name>();
    let path = match profile.paths() {
        Some(paths) => paths.store().join("store.ron"),
        None => {
            #[cfg(test)]
            {
                ProfilePaths::current().store().join("store.ron")
            }
            #[cfg(not(test))]
            {
                return;
            }
        }
    };
    commands.spawn((
        Name::new("Space persistence"),
        AutoSave {
            debounce: Timer::from_seconds(0.5, TimerMode::Once),
            periodic: Timer::from_seconds(60.0, TimerMode::Repeating),
            components,
        },
        WorkspaceStore,
        PersistenceState::new(STORE_SCHEMA_VERSION),
        WorkspaceStorePath(path),
    ));
}

fn handle_save_requests(
    mut requests: MessageReader<WorkspaceSaveRequest>,
    save_entities: SpaceSaveEntities,
    persistence: Single<(&AutoSave, &WorkspaceStorePath, &PersistenceState), With<WorkspaceStore>>,
    mut saver: SpaceSaver,
) {
    for request in requests.read() {
        let store = DurableStore::new(request.path.clone(), STORE_NAME, STORE_SCHEMA_VERSION);
        saver.save(
            store,
            None,
            persistence.2.current_revision(),
            save_entities.excluded(),
            persistence.0.components.clone(),
        );
    }
}

#[derive(Component)]
struct RestorePending(Vec<Entity>);

#[derive(Component, Default)]
struct RestoreBaseline(HashSet<Entity>);

fn mark_restore_pending(
    trigger: On<Loaded>,
    mut persistence: Single<(Entity, &mut PersistenceState), With<WorkspaceStore>>,
    mut commands: Commands,
) {
    let (entity, state) = &mut *persistence;
    if state.phase() != RestorePhase::Materialize {
        return;
    }
    commands.entity(*entity).insert(RestorePending(
        trigger.event().entities().collect::<Vec<_>>(),
    ));
}

fn validate_restore(world: &mut World) {
    let store = {
        let mut stores = world.query_filtered::<Entity, (
            With<WorkspaceStore>,
            Or<(With<RestorePending>, With<RestoreBaseline>)>,
        )>();
        stores.iter(world).next()
    };
    let Some(store) = store else {
        return;
    };
    let phase = world
        .get::<PersistenceState>(store)
        .map(PersistenceState::phase);
    if phase == Some(RestorePhase::Failed) {
        let reason = world
            .get::<PersistenceState>(store)
            .and_then(|state| match state.recovery() {
                PersistenceRecovery::Failed(reason) => Some(reason.clone()),
                PersistenceRecovery::Healthy | PersistenceRecovery::HardReset(_) => None,
            })
            .unwrap_or_else(|| "workspace restore failed".to_string());
        reset_restore(world, store, reason);
        return;
    }
    if phase != Some(RestorePhase::Materialize) || world.get::<RestorePending>(store).is_none() {
        return;
    }
    let validators = {
        let mut query = world.query::<&WorkspaceRestoreValidator>();
        query
            .iter(world)
            .map(|validator| (validator.name, validator.validate))
            .collect::<Vec<_>>()
    };
    for (name, validate) in validators {
        if let Err(error) = validate(world) {
            reset_restore(world, store, format!("{name}: {error}"));
            return;
        }
    }
    if let Some(mut state) = world.get_mut::<PersistenceState>(store) {
        state.begin_resolve();
    }
}

fn reset_restore(world: &mut World, entity: Entity, reason: String) {
    let loaded = world
        .get::<RestorePending>(entity)
        .map(|pending| pending.0.clone());
    let baseline = world
        .get::<RestoreBaseline>(entity)
        .map(|baseline| baseline.0.clone())
        .unwrap_or_default();
    let discard = match loaded {
        Some(loaded) => loaded,
        None => {
            let mut saved = world.query_filtered::<Entity, With<Save>>();
            saved
                .iter(world)
                .filter(|loaded| !baseline.contains(loaded))
                .collect()
        }
    };
    for loaded in discard {
        if !baseline.contains(&loaded) {
            let _ = world.despawn(loaded);
        }
    }
    let path = world
        .get::<WorkspaceStorePath>(entity)
        .map(|path| path.0.clone());
    let Some(path) = path else {
        if let Some(mut state) = world.get_mut::<PersistenceState>(entity) {
            state.restore_failed("workspace store path missing");
        }
        world
            .entity_mut(entity)
            .remove::<RestorePending>()
            .remove::<RestoreBaseline>();
        return;
    };
    let store = DurableStore::new(path, STORE_NAME, STORE_SCHEMA_VERSION);
    match store.hard_reset(reason.clone()) {
        Ok(_) => {
            warn!(
                path = %store.path().display(),
                new_schema = store.schema_version(),
                %reason,
                "workspace store hard reset after restore failure"
            );
            if let Some(mut state) = world.get_mut::<PersistenceState>(entity) {
                state.hard_reset(reason);
            }
        }
        Err(error) => {
            if let Some(mut state) = world.get_mut::<PersistenceState>(entity) {
                state.restore_failed(error.to_string());
            }
        }
    }
    world
        .entity_mut(entity)
        .remove::<RestorePending>()
        .remove::<RestoreBaseline>();
}

fn complete_restore(
    mut persistence: Single<
        (Entity, &mut PersistenceState),
        (With<WorkspaceStore>, With<RestorePending>),
    >,
    mut commands: Commands,
) {
    let (entity, state) = &mut *persistence;
    if state.phase() != RestorePhase::Resolve {
        return;
    }
    state.ready(true);
    commands
        .entity(*entity)
        .remove::<RestorePending>()
        .remove::<RestoreBaseline>();
}

#[derive(Component)]
struct AutoSave {
    debounce: Timer,
    periodic: Timer,
    components: WorldFilter,
}

#[derive(bevy::ecs::system::SystemParam)]
struct SpaceSaveEntities<'w, 's> {
    saved: Query<'w, 's, Entity, With<Save>>,
    hierarchy: WindowHierarchy<'w, 's>,
    windows: Query<'w, 's, (), With<Window>>,
    primary_window: Query<'w, 's, Entity, With<PrimaryWindow>>,
}

#[derive(bevy::ecs::system::SystemParam)]
struct SpaceSaver<'w, 's> {
    commands: Commands<'w, 's>,
}

impl SpaceSaver<'_, '_> {
    fn save(
        &mut self,
        store: DurableStore,
        state: Option<Entity>,
        revision: vmux_ecs::Revision,
        excluded: impl IntoIterator<Item = Entity>,
        components: WorldFilter,
    ) {
        if SessionEnvironment::is_test() {
            return;
        }
        let mut save: AtomicSaveWorld = match state {
            Some(state) => AtomicSaveWorld::new(store, state, revision),
            None => AtomicSaveWorld::untracked(store, revision),
        };
        save.entities = EntityFilter::block(excluded);
        save.components = components;
        self.commands.trigger_save(save);
    }
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
                self.hierarchy
                    .get(*entity)
                    .is_some_and(|window| window != primary_window)
            })
            .collect()
    }
}

const STORE_SCHEMA_VERSION: u32 = 4;
const STORE_NAME: &str = "workspace";

#[derive(Component)]
struct WorkspaceStorePath(PathBuf);

#[cfg(test)]
impl Default for WorkspaceStorePath {
    fn default() -> Self {
        Self(ProfilePaths::current().store().join("store.ron"))
    }
}

impl WorkspaceStorePath {
    fn store(&self) -> DurableStore {
        DurableStore::new(&self.0, STORE_NAME, STORE_SCHEMA_VERSION)
    }

    #[cfg(test)]
    fn load(&self) -> LoadWorld {
        let StoreLoad::Ready(snapshot) = self.store().load().unwrap() else {
            panic!("workspace store should be ready");
        };
        LoadWorld::default_from_stream(std::io::Cursor::new(snapshot.into_payload().into_bytes()))
    }
}

fn mark_dirty(
    _trigger: On<PersistenceDirty>,
    mut persistence: Single<(&mut AutoSave, &mut PersistenceState), With<WorkspaceStore>>,
) {
    persistence.1.advance_dirty();
    let auto_save = &mut persistence.0;
    auto_save.debounce.reset();
}

fn auto_save(
    time: Res<Time>,
    mut persistence: Single<
        (
            Entity,
            &mut AutoSave,
            &WorkspaceStorePath,
            &mut PersistenceState,
        ),
        With<WorkspaceStore>,
    >,
    spaces: Query<(), With<Space>>,
    save_entities: SpaceSaveEntities,
    mut saver: SpaceSaver,
) {
    let (entity, auto_save, path, state) = &mut *persistence;
    let entity = *entity;
    auto_save.periodic.tick(time.delta());

    if spaces.is_empty() {
        return;
    }

    if state.dirty().is_some() {
        auto_save.debounce.tick(time.delta());
    }
    let due = auto_save.debounce.is_finished() || auto_save.periodic.just_finished();
    if !due {
        return;
    }
    let Some(revision) = state.start_save() else {
        return;
    };
    saver.save(
        path.store(),
        Some(entity),
        revision,
        save_entities.excluded(),
        auto_save.components.clone(),
    );
    if SessionEnvironment::is_test() {
        state.save_succeeded(revision, vmux_ecs::UnixMillis::now());
    }
}

#[cfg(test)]
fn save_space_to_path(world: &mut World, path: PathBuf) {
    let components = WorkspacePersisted::filter(&world.resource::<AppTypeRegistry>().read())
        .allow::<Save>()
        .allow::<ChildOf>()
        .allow::<Children>()
        .allow::<Name>();
    world
        .run_system_once(move |mut saver: SpaceSaver| {
            saver.save(
                DurableStore::new(path.clone(), STORE_NAME, STORE_SCHEMA_VERSION),
                None,
                vmux_ecs::Revision::new(1),
                std::iter::empty(),
                components.clone(),
            );
        })
        .unwrap();
}

fn load_on_startup(
    registry: Res<AppTypeRegistry>,
    validators: WorkspaceStoreValidators,
    saved: Query<Entity, With<Save>>,
    mut persistence: Single<
        (Entity, &WorkspaceStorePath, &mut PersistenceState),
        With<WorkspaceStore>,
    >,
    mut commands: Commands,
) {
    let (entity, path, state) = &mut *persistence;
    if SessionEnvironment::is_test() {
        state.ready(false);
        return;
    }
    state.begin_read();
    let store = path.store();
    let snapshot = match store.load() {
        Ok(StoreLoad::Missing) => {
            state.ready(false);
            return;
        }
        Ok(StoreLoad::Reset(reset)) => {
            warn!(
                path = %store.path().display(),
                old_schema = ?reset.old_schema_version(),
                new_schema = store.schema_version(),
                reason = %reset.reason(),
                "workspace store hard reset"
            );
            state.hard_reset(reset.reason());
            return;
        }
        Ok(StoreLoad::Ready(snapshot)) => snapshot,
        Err(error) => {
            error!(path = %store.path().display(), %error, "workspace store read failed");
            state.restore_failed(error.to_string());
            return;
        }
    };
    state.begin_validation(snapshot.metadata().revision());
    let rejection = validators
        .rejected_by(snapshot.payload())
        .map(|name| format!("rejected by {name}"))
        .or_else(|| {
            let registry = registry.read();
            space_has_unregistered_types(snapshot.payload(), &registry)
                .then(|| "unregistered component type".to_string())
        });
    if let Some(reason) = rejection {
        match store.hard_reset(reason.clone()) {
            Ok(_) => {
                warn!(
                    path = %store.path().display(),
                    old_schema = snapshot.metadata().schema_version(),
                    new_schema = store.schema_version(),
                    %reason,
                    "workspace store hard reset"
                );
                state.hard_reset(reason);
            }
            Err(error) => {
                state.restore_failed(error.to_string());
            }
        }
        return;
    }
    info!(path = %store.path().display(), "loading workspace store");
    state.begin_materialize();
    commands
        .entity(*entity)
        .insert(RestoreBaseline(saved.iter().collect()));
    TrackedLoadWorld::default_from_stream(
        *entity,
        std::io::Cursor::new(snapshot.into_payload().into_bytes()),
    )
    .trigger(&mut commands);
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

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;
    use vmux_layout::settings::{
        FocusRingSettings, LayoutSettings, PaneSettings, SideSheetSettings, WindowSettings,
    };
    use vmux_setting::{AppSettings, BrowserSettings, ShortcutSettings};

    struct TestStore(Entity);

    impl TestStore {
        fn spawn(app: &mut App) -> Self {
            Self(
                app.world_mut()
                    .spawn((
                        AutoSave {
                            debounce: Timer::from_seconds(0.5, TimerMode::Once),
                            periodic: Timer::from_seconds(60.0, TimerMode::Repeating),
                            components: WorldFilter::allow_all(),
                        },
                        WorkspaceStore,
                        PersistenceState::restored(
                            STORE_SCHEMA_VERSION,
                            vmux_ecs::Revision::new(1),
                        ),
                    ))
                    .id(),
            )
        }

        fn clear(&self, app: &mut App) {
            app.world_mut()
                .get_mut::<PersistenceState>(self.0)
                .unwrap()
                .ready(true);
            let revision = app
                .world()
                .get::<PersistenceState>(self.0)
                .unwrap()
                .current_revision();
            let mut state = app.world_mut().get_mut::<PersistenceState>(self.0).unwrap();
            state.mark_dirty(revision);
            let revision = state.start_save().unwrap();
            state.save_succeeded(revision, vmux_ecs::UnixMillis::now());
        }

        fn dirty(&self, app: &App) -> bool {
            app.world()
                .get::<PersistenceState>(self.0)
                .unwrap()
                .dirty()
                .is_some()
        }
    }

    #[test]
    fn adding_archived_page_marks_store_dirty() {
        let mut app = App::new();
        let store = TestStore::spawn(&mut app);
        app.register_persisted::<ArchivedPage>()
            .add_observer(mark_dirty);
        app.update();
        store.clear(&mut app);
        app.world_mut().spawn(ArchivedPage::default());
        app.update();
        assert!(store.dirty(&app));
    }

    #[test]
    fn adding_visit_marks_store_dirty() {
        let mut app = App::new();
        let store = TestStore::spawn(&mut app);
        app.register_persisted::<vmux_history::Visit>()
            .add_observer(mark_dirty);
        app.update();
        store.clear(&mut app);
        app.world_mut().spawn(vmux_history::Visit);
        app.update();
        assert!(store.dirty(&app));
    }

    #[test]
    fn changing_tab_startup_dir_marks_store_dirty() {
        let mut app = App::new();
        let store = TestStore::spawn(&mut app);
        app.register_persisted::<StartupDir>()
            .add_observer(mark_dirty);
        let tab = app.world_mut().spawn(Tab::default()).id();
        app.update();
        store.clear(&mut app);
        app.world_mut()
            .entity_mut(tab)
            .get_mut::<StartupDir>()
            .unwrap()
            .0 = Some("/tmp/rebound".into());

        app.update();

        assert!(store.dirty(&app));
    }

    #[test]
    fn adding_tab_workspace_marks_store_dirty() {
        let mut app = App::new();
        let store = TestStore::spawn(&mut app);
        app.register_persisted::<TabWorkspace>()
            .add_observer(mark_dirty);
        let tab = app.world_mut().spawn(Tab::default()).id();
        app.update();
        store.clear(&mut app);
        app.world_mut().entity_mut(tab).insert(TabWorkspace {
            project_dir: "/tmp/project".into(),
        });

        app.update();

        assert!(store.dirty(&app));
    }

    #[test]
    fn removing_tab_worktree_marks_store_dirty() {
        let mut app = App::new();
        let store = TestStore::spawn(&mut app);
        app.register_persisted::<Tab>()
            .register_persisted::<TabWorktree>()
            .add_observer(mark_dirty);
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
        store.clear(&mut app);
        app.world_mut().entity_mut(tab).remove::<TabWorktree>();

        app.update();

        assert!(store.dirty(&app));
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
        app_save
            .add_plugins(MinimalPlugins)
            .add_plugins(vmux_ecs::PrimitivesPlugin)
            .register_persisted::<vmux_ecs::Visit>()
            .register_persisted::<vmux_ecs::Url>()
            .register_persisted::<vmux_ecs::VisitCount>()
            .register_persisted::<vmux_ecs::LastVisitedAt>()
            .register_persisted::<vmux_ecs::VisitedUrl>()
            .register_persisted::<vmux_ecs::TransitionType>()
            .add_observer(save_on::<AtomicSaveWorld>);

        let url_e = app_save
            .world_mut()
            .spawn((
                Save,
                vmux_ecs::Url,
                PageMetadata {
                    url: "https://example.com".into(),
                    title: "Example".into(),
                    icon: vmux_api::PageIcon::None,
                    bg_color: None,
                },
                vmux_ecs::VisitCount(3),
                vmux_ecs::LastVisitedAt::new(1000),
                vmux_ecs::CreatedAt::new(500),
            ))
            .id();

        app_save.world_mut().spawn((
            Save,
            vmux_ecs::Visit,
            vmux_ecs::VisitedUrl(url_e),
            vmux_ecs::CreatedAt::new(900),
            vmux_ecs::TransitionType::Typed,
        ));

        save_space_to_path(app_save.world_mut(), path.clone());
        app_save.update();

        assert!(path.exists(), "save file should exist");

        let mut app_load = App::new();
        app_load
            .add_plugins((MinimalPlugins, AssetPlugin::default()))
            .add_plugins(vmux_ecs::PrimitivesPlugin)
            .register_persisted::<vmux_ecs::Visit>()
            .register_persisted::<vmux_ecs::Url>()
            .register_persisted::<vmux_ecs::VisitCount>()
            .register_persisted::<vmux_ecs::LastVisitedAt>()
            .register_persisted::<vmux_ecs::VisitedUrl>()
            .register_persisted::<vmux_ecs::TransitionType>()
            .add_observer(load_on_default_event);
        app_load.update();

        app_load
            .world_mut()
            .commands()
            .trigger_load(WorkspaceStorePath(path).load());
        app_load.update();

        let url_count = app_load
            .world_mut()
            .query::<&vmux_ecs::Url>()
            .iter(app_load.world())
            .count();
        let visit_count = app_load
            .world_mut()
            .query::<&vmux_ecs::Visit>()
            .iter(app_load.world())
            .count();
        assert_eq!(url_count, 1, "Url not round-tripped");
        assert_eq!(visit_count, 1, "Visit not round-tripped");

        let (vc, lva, ca) = app_load
            .world_mut()
            .query::<(
                &vmux_ecs::VisitCount,
                &vmux_ecs::LastVisitedAt,
                &vmux_ecs::CreatedAt,
            )>()
            .iter(app_load.world())
            .find(|(vc, _, _)| vc.0 == 3)
            .expect("Url entity fields not round-tripped");
        assert_eq!(vc.0, 3);
        assert_eq!(lva.milliseconds(), 1000);
        assert_eq!(ca.milliseconds(), 500);

        let tt = app_load
            .world_mut()
            .query::<&vmux_ecs::TransitionType>()
            .iter(app_load.world())
            .next()
            .expect("TransitionType not round-tripped");
        assert_eq!(*tt, vmux_ecs::TransitionType::Typed);
    }

    #[test]
    fn persisted_git_branch_icon_still_loads() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("legacy-git-icon.ron");

        let mut app_save = App::new();
        app_save
            .add_plugins(MinimalPlugins)
            .add_plugins(vmux_ecs::PrimitivesPlugin)
            .add_observer(save_on::<AtomicSaveWorld>);
        app_save.world_mut().spawn((
            Save,
            PageMetadata {
                title: "Git".into(),
                url: "git://".into(),
                icon: vmux_api::PageIcon::Builtin(vmux_api::BuiltinIcon::Project),
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
            .add_plugins(vmux_ecs::PrimitivesPlugin)
            .add_observer(load_on_default_event);
        app_load.update();
        app_load
            .world_mut()
            .commands()
            .trigger_load(WorkspaceStorePath(path).load());
        app_load.update();

        let metadata = app_load
            .world_mut()
            .query::<&PageMetadata>()
            .single(app_load.world())
            .expect("legacy page metadata loaded");
        assert_eq!(
            metadata.icon,
            vmux_api::PageIcon::Builtin(vmux_api::BuiltinIcon::GitBranch)
        );
    }

    #[test]
    fn window_geometry_round_trips_through_store() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("store.ron");

        let mut app_save = App::new();
        app_save.add_plugins(MinimalPlugins);
        app_save.add_plugins(vmux_ecs::PrimitivesPlugin);
        app_save
            .register_persisted::<WindowGeometry>()
            .register_type::<Option<IVec2>>()
            .register_type::<Option<Vec2>>();
        app_save.add_observer(save_on::<AtomicSaveWorld>);
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
            .add_plugins(vmux_ecs::PrimitivesPlugin);
        app_load
            .register_type::<WindowGeometry>()
            .register_type::<Option<IVec2>>()
            .register_type::<Option<Vec2>>();
        app_load.add_observer(load_on_default_event);
        app_load.update();
        app_load
            .world_mut()
            .commands()
            .trigger_load(WorkspaceStorePath(path).load());
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
    fn custom_save_embeds_schema_version_in_saved_store() {
        let _home = HomeEnvGuard::use_temp_home("custom-save-schema-version");
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("custom-store.ron");

        let mut app_save = App::new();
        app_save
            .add_plugins(MinimalPlugins)
            .add_plugins(vmux_ecs::PrimitivesPlugin)
            .register_persisted::<Space>()
            .register_persisted::<SpaceId>()
            .register_persisted::<WindowGeometry>()
            .add_observer(save_on::<AtomicSaveWorld>);
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

        let StoreLoad::Ready(snapshot) = DurableStore::new(&path, STORE_NAME, STORE_SCHEMA_VERSION)
            .load()
            .unwrap()
        else {
            panic!("custom store should be ready");
        };
        assert_eq!(snapshot.metadata().schema_version(), STORE_SCHEMA_VERSION);
        assert!(!dir.path().join("store.version").exists());
    }

    #[test]
    fn pane_id_and_position_round_trip_through_store() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("store.ron");

        let mut app_save = App::new();
        app_save.add_plugins(MinimalPlugins);
        app_save.add_plugins(vmux_ecs::PrimitivesPlugin);
        app_save
            .register_persisted::<PaneId>()
            .register_persisted::<ArchivedPage>()
            .register_persisted::<ArchivedPagePosition>()
            .register_persisted::<ArchivedTabPage>()
            .register_type::<vmux_ecs::PaneStep>()
            .register_type::<vmux_ecs::SplitAxis>()
            .register_type::<Vec<vmux_ecs::PaneStep>>();
        app_save.add_observer(save_on::<AtomicSaveWorld>);
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
                pane_path: vec![vmux_ecs::PaneStep {
                    split_id: "root".into(),
                    axis: vmux_ecs::SplitAxis::Column,
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
            .add_plugins(vmux_ecs::PrimitivesPlugin)
            .register_type::<PaneId>()
            .register_type::<ArchivedPage>()
            .register_type::<ArchivedPagePosition>()
            .register_type::<ArchivedTabPage>()
            .register_type::<vmux_ecs::PaneStep>()
            .register_type::<vmux_ecs::SplitAxis>()
            .register_type::<Vec<vmux_ecs::PaneStep>>()
            .add_observer(load_on_default_event);
        app_load.update();
        app_load
            .world_mut()
            .commands()
            .trigger_load(WorkspaceStorePath(path).load());
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
        assert!(matches!(pos.pane_path[0].axis, vmux_ecs::SplitAxis::Column));
        let tab = app_load
            .world_mut()
            .query::<&ArchivedTabPage>()
            .single(app_load.world())
            .expect("tab archive metadata round-tripped");
        assert_eq!(tab.group_id, "tab-group");
        assert_eq!(tab.tab_name, "Recovered");
        assert_eq!(
            tab.tab_startup_dir.as_deref(),
            Some(std::path::Path::new("/tmp/recovered"))
        );
        assert!(tab.active);
    }

    fn registry_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(vmux_ecs::PrimitivesPlugin);
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
        let key = <vmux_ecs::PageMetadata as bevy::reflect::TypePath>::type_path();
        let body = store_body_with_key(key);
        assert!(!space_has_unregistered_types(&body, &registry));
    }

    #[test]
    fn incompatible_component_store_is_hard_reset_on_startup() {
        let _home = HomeEnvGuard::use_temp_home("incompatible-store-is-removed-on-startup");
        let store = WorkspaceStorePath::default();
        let path = store.0.clone();
        store
            .store()
            .write(
                vmux_ecs::Revision::new(1),
                store_body_with_key("vmux_desktop::ghost::DoesNotExist"),
            )
            .unwrap();

        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(test_settings())
            .add_plugins(WorkspacePersistencePlugin);
        app.update();

        assert!(
            !path.exists(),
            "incompatible store should be removed on startup"
        );
        let state = app
            .world_mut()
            .query_filtered::<&PersistenceState, With<WorkspaceStore>>()
            .single(app.world())
            .unwrap();
        assert!(matches!(
            state.recovery(),
            vmux_ecs::persistence::PersistenceRecovery::HardReset(_)
        ));
    }

    #[test]
    fn invalid_materialized_world_is_discarded_before_ready() {
        let _home = HomeEnvGuard::use_temp_home("invalid-materialized-world-is-discarded");
        let path = WorkspaceStorePath::default();
        std::fs::write(&path.0, "invalid").unwrap();
        let mut state = PersistenceState::new(STORE_SCHEMA_VERSION);
        state.begin_read();
        state.begin_validation(vmux_ecs::Revision::new(1));
        state.begin_materialize();
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(PreUpdate, validate_restore);
        let first = app
            .world_mut()
            .spawn((Save, SpaceId("duplicate".to_string())))
            .id();
        let second = app
            .world_mut()
            .spawn((Save, SpaceId("duplicate".to_string())))
            .id();
        app.world_mut().spawn((
            Name::new("Space identity validator"),
            WorkspaceRestoreValidator::unique::<SpaceId>("Space identity"),
        ));
        let store = app
            .world_mut()
            .spawn((
                WorkspaceStore,
                state,
                path,
                RestorePending(vec![first, second]),
                RestoreBaseline::default(),
            ))
            .id();

        app.update();

        assert!(app.world().get_entity(first).is_err());
        assert!(app.world().get_entity(second).is_err());
        assert!(matches!(
            app.world()
                .get::<PersistenceState>(store)
                .unwrap()
                .recovery(),
            PersistenceRecovery::HardReset(_)
        ));
        assert!(!WorkspaceStorePath::default().0.exists());
    }

    #[test]
    fn valid_materialized_world_becomes_ready_after_resolution() {
        let mut state = PersistenceState::new(STORE_SCHEMA_VERSION);
        state.begin_read();
        state.begin_validation(vmux_ecs::Revision::new(1));
        state.begin_materialize();
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .configure_sets(
                PreUpdate,
                (
                    WorkspaceRestoreSet::Validate,
                    WorkspaceRestoreSet::Resolve,
                    WorkspaceRestoreSet::Ready,
                )
                    .chain(),
            )
            .add_systems(
                PreUpdate,
                validate_restore.in_set(WorkspaceRestoreSet::Validate),
            )
            .add_systems(
                PreUpdate,
                complete_restore.in_set(WorkspaceRestoreSet::Ready),
            );
        let loaded = app.world_mut().spawn(Save).id();
        let store = app
            .world_mut()
            .spawn((
                WorkspaceStore,
                state,
                WorkspaceStorePath::default(),
                RestorePending(vec![loaded]),
                RestoreBaseline::default(),
            ))
            .id();

        app.update();

        assert_eq!(
            app.world().get::<PersistenceState>(store).unwrap().phase(),
            RestorePhase::Ready
        );
        assert!(app.world().get_entity(loaded).is_ok());
        assert!(app.world().get::<RestorePending>(store).is_none());
    }

    #[test]
    fn auto_save_system_skips_save_without_space() {
        let _home = HomeEnvGuard::use_temp_home("auto-save-system-skips-without-space");
        let mut app = App::new();
        let mut state =
            PersistenceState::restored(STORE_SCHEMA_VERSION, vmux_ecs::Revision::new(1));
        state.mark_dirty(vmux_ecs::Revision::new(2));
        app.world_mut().spawn((
            AutoSave {
                debounce: Timer::from_seconds(0.0, TimerMode::Once),
                periodic: Timer::from_seconds(0.0, TimerMode::Repeating),
                components: WorldFilter::allow_all(),
            },
            WorkspaceStore,
            state,
            WorkspaceStorePath::default(),
        ));
        app.add_plugins(MinimalPlugins)
            .add_plugins(vmux_ecs::PrimitivesPlugin)
            .register_persisted::<WindowGeometry>()
            .register_type::<Option<IVec2>>()
            .register_type::<Option<Vec2>>()
            .add_observer(save_on::<AtomicSaveWorld>)
            .add_systems(Update, auto_save);
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
            !WorkspaceStorePath::default().0.exists(),
            "auto_save must skip when no Space exists"
        );
    }

    #[test]
    fn auto_save_system_saves_with_space() {
        let _home = HomeEnvGuard::use_temp_home("auto-save-system-saves-with-space");
        let mut app = App::new();
        let mut state =
            PersistenceState::restored(STORE_SCHEMA_VERSION, vmux_ecs::Revision::new(1));
        state.mark_dirty(vmux_ecs::Revision::new(2));
        app.world_mut().spawn((
            AutoSave {
                debounce: Timer::from_seconds(0.0, TimerMode::Once),
                periodic: Timer::from_seconds(0.0, TimerMode::Repeating),
                components: WorldFilter::allow_all(),
            },
            WorkspaceStore,
            state,
            WorkspaceStorePath::default(),
        ));
        app.add_plugins(MinimalPlugins)
            .add_plugins(vmux_ecs::PrimitivesPlugin)
            .register_persisted::<WindowGeometry>()
            .register_type::<Option<IVec2>>()
            .register_type::<Option<Vec2>>()
            .add_observer(save_on::<AtomicSaveWorld>)
            .add_systems(Update, auto_save);
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
            WorkspaceStorePath::default().0.exists(),
            "auto_save must save when a Space exists"
        );
    }
}
