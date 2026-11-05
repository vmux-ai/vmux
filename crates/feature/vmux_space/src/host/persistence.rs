use bevy::ecs::schedule::common_conditions::any_with_component;
use bevy::ecs::system::Command;
#[cfg(test)]
use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
#[cfg(test)]
use bevy_cef::prelude::HostWindow;
use bevy_world_serialization::WorldFilter;
use moonshine_save::prelude::*;
use moonshine_save::save::{EntityFilter, SaveCommand};
use std::path::{Path, PathBuf};

use vmux_api::BuiltinIcon;
#[cfg(test)]
use vmux_ecs::persistence::PersistenceAppExt;
use vmux_ecs::persistence::{
    PersistenceDirty, WorkspacePersisted, WorkspaceRestore, WorkspaceSaveRequest,
    WorkspaceStoreValidators,
};
#[cfg(test)]
use vmux_ecs::profile::ProfilePaths;
use vmux_ecs::profile::{CurrentProfile, SessionEnvironment};
#[cfg(test)]
use vmux_ecs::{ArchivedPage, ArchivedPagePosition, ArchivedTabPage, PageMetadata};
use vmux_layout::space::Space;
#[cfg(test)]
use vmux_layout::space::SpaceId;
use vmux_layout::window::WindowHierarchy;
use vmux_layout::{
    LayoutPersistenceSet, LayoutRestoreCommitted, LayoutRestoreRequest, LayoutStartupSet,
};
#[cfg(test)]
use vmux_layout::{
    pane::{Pane, PaneId},
    tab::{Tab, TabWorkspace, TabWorktree},
    window::WindowGeometry,
};
pub(crate) struct WorkspacePersistencePlugin;

impl Plugin for WorkspacePersistencePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<WorkspaceSaveRequest>()
            .add_message::<LayoutRestoreRequest>()
            .add_message::<LayoutRestoreCommitted>()
            .add_observer(save_on_default_event)
            .add_observer(load_on_default_event)
            .add_systems(
                Startup,
                (spawn, ApplyDeferred, load_on_startup)
                    .chain()
                    .in_set(LayoutStartupSet::Persistence),
            )
            .add_observer(mark_restore_pending)
            .add_observer(mark_dirty)
            .add_systems(
                Update,
                complete_restore
                    .after(LayoutPersistenceSet::Restore)
                    .run_if(any_with_component::<RestorePending>),
            )
            .add_systems(Update, (auto_save, handle_save_requests))
            .add_systems(Update, track_hierarchy.before(auto_save));
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
            dirty: false,
            components,
        },
        WorkspaceRestore::default(),
        WorkspaceStorePath(path),
    ));
}

fn handle_save_requests(
    mut requests: MessageReader<WorkspaceSaveRequest>,
    save_entities: SpaceSaveEntities,
    persistence: Single<&AutoSave>,
    mut saver: SpaceSaver,
) {
    for request in requests.read() {
        saver.save(
            request.path.clone(),
            save_entities.excluded(),
            persistence.components.clone(),
            None,
        );
    }
}

#[derive(Component)]
struct RestorePending;

fn mark_restore_pending(
    _trigger: On<Loaded>,
    persistence: Single<Entity, With<AutoSave>>,
    mut restore_requests: MessageWriter<LayoutRestoreRequest>,
    mut commands: Commands,
) {
    commands.entity(*persistence).insert(RestorePending);
    restore_requests.write(LayoutRestoreRequest);
}

fn complete_restore(
    mut committed: MessageReader<LayoutRestoreCommitted>,
    mut restore: Single<&mut WorkspaceRestore>,
    persistence: Single<Entity, With<RestorePending>>,
    mut commands: Commands,
) {
    if committed.read().count() == 0 {
        return;
    }
    restore.complete = true;
    commands.entity(*persistence).remove::<RestorePending>();
}

#[derive(Component)]
struct AutoSave {
    debounce: Timer,
    periodic: Timer,
    dirty: bool,
    components: WorldFilter,
}

#[derive(PartialEq, Eq)]
struct HierarchyState {
    name: Option<String>,
    parent: Option<Entity>,
    children: Vec<Entity>,
}

impl HierarchyState {
    fn matches(
        &self,
        name: Option<&Name>,
        parent: Option<&ChildOf>,
        children: Option<&Children>,
    ) -> bool {
        if self.name.as_deref() != name.map(Name::as_str)
            || self.parent != parent.map(ChildOf::parent)
        {
            return false;
        }
        match children {
            Some(children) => children.iter().eq(self.children.iter().copied()),
            None => self.children.is_empty(),
        }
    }

    fn of(name: Option<&Name>, parent: Option<&ChildOf>, children: Option<&Children>) -> Self {
        Self {
            name: name.map(|name| name.as_str().to_string()),
            parent: parent.map(ChildOf::parent),
            children: children
                .map(|children| children.iter().collect())
                .unwrap_or_default(),
        }
    }
}

fn track_hierarchy(
    changed: Query<
        Entity,
        (
            With<Save>,
            Or<(
                Added<Save>,
                Changed<Name>,
                Changed<ChildOf>,
                Changed<Children>,
            )>,
        ),
    >,
    nodes: Query<(Option<&Name>, Option<&ChildOf>, Option<&Children>), With<Save>>,
    mut removed_saved: RemovedComponents<Save>,
    mut removed_name: RemovedComponents<Name>,
    mut removed_parent: RemovedComponents<ChildOf>,
    mut removed_children: RemovedComponents<Children>,
    mut previous: Local<std::collections::HashMap<Entity, HierarchyState>>,
    mut commands: Commands,
) {
    let mut dirty = false;
    for entity in removed_saved.read() {
        previous.remove(&entity);
        dirty = true;
    }
    let mut touched = std::collections::HashSet::new();
    for entity in &changed {
        touched.insert(entity);
    }
    for entity in removed_name.read() {
        touched.insert(entity);
    }
    for entity in removed_parent.read() {
        touched.insert(entity);
    }
    for entity in removed_children.read() {
        touched.insert(entity);
    }
    for entity in touched {
        let Ok((name, parent, children)) = nodes.get(entity) else {
            continue;
        };
        if previous
            .get(&entity)
            .is_some_and(|previous| previous.matches(name, parent, children))
        {
            continue;
        }
        previous.insert(entity, HierarchyState::of(name, parent, children));
        dirty = true;
    }
    if dirty {
        commands.trigger(PersistenceDirty);
    }
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
        path: PathBuf,
        excluded: impl IntoIterator<Item = Entity>,
        components: WorldFilter,
        autosave: Option<Entity>,
    ) {
        if SessionEnvironment::is_test() {
            return;
        }
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let schema = WorkspaceStorePath(path.clone());
        let mut save = SaveWorld::default_into_file(path);
        save.entities = EntityFilter::block(excluded);
        save.components = components;
        self.commands.queue(WorkspaceWrite {
            save,
            schema,
            autosave,
        });
    }
}

struct WorkspaceWrite {
    save: SaveWorld,
    schema: WorkspaceStorePath,
    autosave: Option<Entity>,
}

impl Command for WorkspaceWrite {
    type Out = ();

    fn apply(self, world: &mut World) {
        let result = SaveCommand(self.save).apply(world);
        if let Err(error) = &result {
            warn!("Workspace save failed for {:?}: {error}", self.schema.0);
        } else {
            self.schema.write_schema_version();
        }
        let Some(entity) = self.autosave else {
            return;
        };
        let Some(mut autosave) = world.get_mut::<AutoSave>(entity) else {
            return;
        };
        autosave.dirty = result.is_err();
        let delay = if autosave.dirty { 60.0 } else { 0.5 };
        autosave
            .debounce
            .set_duration(std::time::Duration::from_secs_f32(delay));
        autosave.debounce.reset();
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

#[derive(Component)]
struct WorkspaceStorePath(PathBuf);

#[cfg(test)]
impl Default for WorkspaceStorePath {
    fn default() -> Self {
        Self(ProfilePaths::current().store().join("store.ron"))
    }
}

impl WorkspaceStorePath {
    fn version(&self) -> PathBuf {
        self.0
            .parent()
            .map(|parent| parent.join("store.version"))
            .unwrap_or_else(|| PathBuf::from("store.version"))
    }

    fn schema_is_current(&self) -> bool {
        std::fs::read_to_string(self.version())
            .ok()
            .and_then(|value| value.trim().parse::<u32>().ok())
            .is_some_and(|version| version >= STORE_SCHEMA_VERSION)
    }

    fn write_schema_version(&self) {
        let _ = std::fs::write(self.version(), STORE_SCHEMA_VERSION.to_string());
    }
}

fn mark_dirty(_trigger: On<PersistenceDirty>, mut auto_save: Single<&mut AutoSave>) {
    auto_save.dirty = true;
    auto_save.debounce.reset();
}

fn auto_save(
    time: Res<Time>,
    auto_save: Single<(Entity, &mut AutoSave)>,
    path: Single<&WorkspaceStorePath>,
    spaces: Query<(), With<Space>>,
    save_entities: SpaceSaveEntities,
    mut saver: SpaceSaver,
) {
    let (entity, mut auto_save) = auto_save.into_inner();
    auto_save.periodic.tick(time.delta());

    if spaces.is_empty() {
        return;
    }

    if !auto_save.dirty {
        return;
    }
    auto_save.debounce.tick(time.delta());
    if !auto_save.debounce.is_finished() && !auto_save.periodic.just_finished() {
        return;
    }
    saver.save(
        path.0.clone(),
        save_entities.excluded(),
        auto_save.components.clone(),
        Some(entity),
    );
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
            saver.save(path.clone(), std::iter::empty(), components.clone(), None);
        })
        .unwrap();
}

fn load_on_startup(
    registry: Res<AppTypeRegistry>,
    validators: WorkspaceStoreValidators,
    mut restore: Single<&mut WorkspaceRestore>,
    path: Single<&WorkspaceStorePath>,
    mut commands: Commands,
) {
    if SessionEnvironment::is_test() {
        restore.complete = true;
        restore.store_present = false;
        return;
    }
    let path = &path.0;
    normalize_workspace_resources_if_needed(path);
    let removed_stale = remove_rejected_store_if_needed(path, &validators);
    let removed_incompatible = {
        let registry = registry.read();
        remove_incompatible_store_if_needed(path, &registry)
    };
    let schema_outdated = path.exists() && !WorkspaceStorePath(path.clone()).schema_is_current();
    if schema_outdated {
        warn!("Store schema outdated; resetting {:?}", path);
        if let Err(e) = std::fs::remove_file(path) {
            warn!("Failed to remove outdated store {:?}: {e}", path);
        }
        let _ = std::fs::remove_file(WorkspaceStorePath(path.clone()).version());
    }
    let exists = path.exists() && !removed_stale && !removed_incompatible && !schema_outdated;
    restore.store_present = exists;
    if exists {
        info!("Loading space from {:?}", path);
        let load = match std::fs::read_to_string(path)
            .ok()
            .and_then(|body| normalized_store_icons(&body))
        {
            Some((body, unknown)) => {
                warn!(?unknown, "Replacing unavailable persisted page icons");
                LoadWorld::default_from_stream(std::io::Cursor::new(body.into_bytes()))
            }
            None => LoadWorld::default_from_file(path.clone()),
        };
        commands.trigger_load(load);
    } else {
        restore.complete = true;
    }
}

fn normalize_workspace_resources_if_needed(path: &Path) {
    let Ok(body) = std::fs::read_to_string(path) else {
        return;
    };
    let Some(normalized) = WorkspaceStoreSource(&body).without_resources() else {
        return;
    };
    if let Err(error) = std::fs::write(path, normalized) {
        warn!(
            "Failed to remove legacy workspace resources from {:?}: {error}",
            path
        );
    } else {
        info!("Removed legacy workspace resources from {:?}", path);
    }
}

struct WorkspaceStoreSource<'a>(&'a str);

impl WorkspaceStoreSource<'_> {
    fn without_resources(&self) -> Option<String> {
        let marker = self.0.find("resources:")? + "resources:".len();
        let open = marker + self.0[marker..].find('{')?;
        let close = self.matching_brace(open)?;
        if self.0[open + 1..close].trim().is_empty() {
            return None;
        }
        let mut normalized = self.0.to_string();
        normalized.replace_range(open + 1..close, "");
        Some(normalized)
    }

    fn matching_brace(&self, open: usize) -> Option<usize> {
        let mut depth = 0usize;
        let mut quoted = false;
        let mut escaped = false;
        for (offset, character) in self.0[open..].char_indices() {
            if quoted {
                if escaped {
                    escaped = false;
                } else if character == '\\' {
                    escaped = true;
                } else if character == '"' {
                    quoted = false;
                }
                continue;
            }
            match character {
                '"' => quoted = true,
                '{' => depth += 1,
                '}' => {
                    depth = depth.checked_sub(1)?;
                    if depth == 0 {
                        return Some(open + offset);
                    }
                }
                _ => {}
            }
        }
        None
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
        if ron::from_str::<BuiltinIcon>(name).is_ok() {
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

fn remove_rejected_store_if_needed(path: &Path, validators: &WorkspaceStoreValidators) -> bool {
    let Ok(body) = std::fs::read_to_string(path) else {
        return false;
    };
    let Some(validator) = validators.rejected_by(&body) else {
        return false;
    };
    warn!(%validator, "Removing rejected store from {:?}", path);
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

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;
    use vmux_layout::settings::{
        FocusRingSettings, LayoutSettings, PaneSettings, SideSheetSettings, WindowSettings,
    };
    use vmux_setting::{AppSettings, BrowserSettings, ShortcutSettings};

    #[derive(Resource, Default)]
    struct SaveCount(u32);

    impl SaveCount {
        fn record(_: On<Saved>, mut count: ResMut<Self>) {
            count.0 += 1;
        }
    }

    struct WorkspaceSaveFixture {
        app: App,
        persistence: Entity,
        path: PathBuf,
        _home: HomeEnvGuard,
    }

    impl WorkspaceSaveFixture {
        fn new(name: &str) -> Self {
            let home = HomeEnvGuard::use_temp_home(name);
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, vmux_ecs::PrimitivesPlugin))
                .register_persisted::<Space>()
                .insert_resource(test_settings())
                .insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
                    std::time::Duration::from_secs(61),
                ))
                .init_resource::<SaveCount>()
                .add_observer(SaveCount::record)
                .add_plugins(WorkspacePersistencePlugin);
            app.world_mut()
                .resource_mut::<Time<bevy::time::Virtual>>()
                .set_max_delta(std::time::Duration::from_secs(120));
            app.world_mut().spawn((Save, Space, Name::new("Workspace")));
            app.update();
            let persistence = app
                .world_mut()
                .query_filtered::<Entity, With<AutoSave>>()
                .single(app.world())
                .unwrap();
            let path = app
                .world()
                .get::<WorkspaceStorePath>(persistence)
                .unwrap()
                .0
                .clone();
            assert!(!path.exists());
            Self {
                app,
                persistence,
                path,
                _home: home,
            }
        }
    }

    #[test]
    fn clean_workspaces_do_not_write_periodic_snapshots() {
        let mut fixture = WorkspaceSaveFixture::new("clean-periodic-autosave");
        fixture
            .app
            .world_mut()
            .get_mut::<AutoSave>(fixture.persistence)
            .unwrap()
            .dirty = false;

        fixture.app.update();

        assert_eq!(fixture.app.world().resource::<SaveCount>().0, 0);
        assert!(!fixture.path.exists());
    }

    #[test]
    fn debounce_and_periodic_deadlines_write_one_snapshot() {
        let mut fixture = WorkspaceSaveFixture::new("coalesced-autosave");

        fixture.app.update();

        assert_eq!(fixture.app.world().resource::<SaveCount>().0, 1);
        assert!(fixture.path.exists());

        fixture.app.update();

        assert_eq!(fixture.app.world().resource::<SaveCount>().0, 1);
    }

    #[test]
    fn failed_workspace_writes_retry_after_storage_recovers_without_another_edit() {
        let mut fixture = WorkspaceSaveFixture::new("failed-autosave-retry");
        std::fs::create_dir_all(&fixture.path).unwrap();
        fixture.app.update();
        assert!(
            fixture
                .app
                .world()
                .get::<AutoSave>(fixture.persistence)
                .unwrap()
                .dirty
        );
        assert_eq!(fixture.app.world().resource::<SaveCount>().0, 0);
        std::fs::remove_dir(&fixture.path).unwrap();

        fixture.app.update();

        assert!(fixture.path.is_file());
        assert!(
            !fixture
                .app
                .world()
                .get::<AutoSave>(fixture.persistence)
                .unwrap()
                .dirty
        );
        assert_eq!(fixture.app.world().resource::<SaveCount>().0, 1);
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
            .add_observer(mark_dirty);
        app.update();
        app.world_mut()
            .get_mut::<AutoSave>(auto_save)
            .unwrap()
            .dirty = false;
        app.world_mut().spawn((Save, ArchivedPage::default()));
        app.update();
        assert!(app.world().get::<AutoSave>(auto_save).unwrap().dirty);
    }

    #[test]
    fn unchanged_saved_names_do_not_restart_autosave() {
        let _home = HomeEnvGuard::use_temp_home("saved-name-autosave");
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(vmux_ecs::PrimitivesPlugin)
            .insert_resource(test_settings())
            .add_plugins(WorkspacePersistencePlugin);
        let entity = app.world_mut().spawn((Save, Name::new("unchanged"))).id();
        app.update();
        let store = app
            .world_mut()
            .query_filtered::<Entity, With<AutoSave>>()
            .single(app.world())
            .unwrap();
        app.world_mut().get_mut::<AutoSave>(store).unwrap().dirty = false;
        app.world_mut()
            .entity_mut(entity)
            .insert(Name::new("unchanged"));
        app.update();
        assert!(!app.world().get::<AutoSave>(store).unwrap().dirty);
        app.world_mut()
            .entity_mut(entity)
            .insert(Name::new("renamed"));
        app.update();
        assert!(app.world().get::<AutoSave>(store).unwrap().dirty);
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
            .add_observer(mark_dirty);
        app.update();
        app.world_mut()
            .get_mut::<AutoSave>(auto_save)
            .unwrap()
            .dirty = false;
        app.world_mut().spawn((Save, vmux_history::Visit));
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
        app.register_persisted::<Tab>().add_observer(mark_dirty);
        let tab = app.world_mut().spawn((Save, Tab::default())).id();
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
            .add_observer(mark_dirty);
        let tab = app.world_mut().spawn((Save, Tab::default())).id();
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
            .add_observer(mark_dirty);
        let tab = app
            .world_mut()
            .spawn((
                Save,
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
        app_save
            .add_plugins(MinimalPlugins)
            .add_plugins(vmux_ecs::PrimitivesPlugin)
            .register_persisted::<vmux_ecs::Visit>()
            .register_persisted::<vmux_ecs::Url>()
            .register_persisted::<vmux_ecs::VisitCount>()
            .register_persisted::<vmux_ecs::LastVisitedAt>()
            .register_persisted::<vmux_ecs::VisitedUrl>()
            .register_persisted::<vmux_ecs::TransitionType>()
            .add_observer(save_on_default_event);

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
                vmux_ecs::LastVisitedAt(1000),
                vmux_ecs::CreatedAt(500),
            ))
            .id();

        app_save.world_mut().spawn((
            Save,
            vmux_ecs::Visit,
            vmux_ecs::VisitedUrl(url_e),
            vmux_ecs::CreatedAt(900),
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
            .trigger_load(LoadWorld::default_from_file(path));
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
        assert_eq!(lva.0, 1000);
        assert_eq!(ca.0, 500);

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
            .add_observer(save_on_default_event);
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
            .trigger_load(LoadWorld::default_from_file(path));
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
            .add_plugins(vmux_ecs::PrimitivesPlugin)
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
        app_save.add_plugins(vmux_ecs::PrimitivesPlugin);
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
            .add_plugins(vmux_ecs::PrimitivesPlugin)
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
            !WorkspaceStorePath::default().version().exists(),
            "custom save must not write default store.version"
        );
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
        assert!(matches!(pos.pane_path[0].axis, vmux_ecs::SplitAxis::Column));
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
    fn legacy_workspace_resources_are_removed_without_touching_entities() {
        let body = r#"(
  resources: {
    "vmux_bookmark::OfferedBookmarkDefaults": (
      urls: ["vmux://sessions/{active}"],
    ),
  },
  entities: {
    1: (
      components: {
        "vmux_ecs::PageMetadata": (),
      },
    ),
  },
)
"#;

        let normalized = WorkspaceStoreSource(body)
            .without_resources()
            .expect("legacy resources");

        assert!(normalized.contains("resources: {}"));
        assert!(normalized.contains("vmux_ecs::PageMetadata"));
        assert!(!normalized.contains("OfferedBookmarkDefaults"));
    }

    #[test]
    fn empty_workspace_resources_need_no_migration() {
        assert!(
            WorkspaceStoreSource("(resources: {}, entities: {})")
                .without_resources()
                .is_none()
        );
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
            .add_plugins(vmux_ecs::PrimitivesPlugin)
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
        assert_eq!(metadata.icon, vmux_api::PageIcon::None);
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
        let key = <vmux_ecs::PageMetadata as bevy::reflect::TypePath>::type_path();
        std::fs::write(&path, store_body_with_key(key)).expect("write store");

        assert!(!remove_incompatible_store_if_needed(&path, &registry));
        assert!(path.exists());
    }

    #[test]
    fn incompatible_store_is_removed_on_startup() {
        let _home = HomeEnvGuard::use_temp_home("incompatible-store-is-removed-on-startup");
        let store = WorkspaceStorePath::default();
        let path = store.0.clone();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("store dir");
        }
        std::fs::write(
            &path,
            store_body_with_key("vmux_desktop::ghost::DoesNotExist"),
        )
        .expect("write store");
        std::fs::write(store.version(), STORE_SCHEMA_VERSION.to_string()).expect("write version");

        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(test_settings())
            .add_plugins(WorkspacePersistencePlugin);
        app.update();

        assert!(
            !path.exists(),
            "incompatible store should be removed on startup"
        );
        assert!(
            !store.version().exists(),
            "store.version should be removed with the incompatible store"
        );
    }

    #[test]
    fn auto_save_system_skips_save_without_space() {
        let _home = HomeEnvGuard::use_temp_home("auto-save-system-skips-without-space");
        let mut app = App::new();
        app.world_mut().spawn((
            AutoSave {
                debounce: Timer::from_seconds(0.0, TimerMode::Once),
                periodic: Timer::from_seconds(0.0, TimerMode::Repeating),
                dirty: true,
                components: WorldFilter::allow_all(),
            },
            WorkspaceStorePath::default(),
        ));
        app.add_plugins(MinimalPlugins)
            .add_plugins(vmux_ecs::PrimitivesPlugin)
            .register_persisted::<WindowGeometry>()
            .register_type::<Option<IVec2>>()
            .register_type::<Option<Vec2>>()
            .add_observer(save_on_default_event)
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
        app.world_mut().spawn((
            AutoSave {
                debounce: Timer::from_seconds(0.0, TimerMode::Once),
                periodic: Timer::from_seconds(0.0, TimerMode::Repeating),
                dirty: true,
                components: WorldFilter::allow_all(),
            },
            WorkspaceStorePath::default(),
        ));
        app.add_plugins(MinimalPlugins)
            .add_plugins(vmux_ecs::PrimitivesPlugin)
            .register_persisted::<WindowGeometry>()
            .register_type::<Option<IVec2>>()
            .register_type::<Option<Vec2>>()
            .add_observer(save_on_default_event)
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
