use bevy::ecs::reflect::ReflectComponent;
use bevy::ecs::resource::IsResource;
use bevy::ecs::schedule::ApplyDeferred;
use bevy::ecs::schedule::common_conditions::any_with_component;
use bevy::ecs::system::{Command, SystemParam};
use bevy::prelude::*;
use bevy::prelude::{App, Plugin};
use bevy::tasks::IoTaskPool;
use bevy::window::PrimaryWindow;
use bevy::winit::{EventLoopProxyWrapper, WinitUserEvent};
#[cfg(test)]
use bevy_cef::prelude::HostWindow;
use bevy_world_serialization::{DynamicWorldBuilder, WorldFilter};
use moonshine_save::load::{LoadCommand, LoadInput};
use moonshine_save::prelude::*;
use moonshine_save::save::{EntityFilter, SaveCommand, SaveEvent};
use serde::de::DeserializeSeed;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, mpsc};
use vmux_ecs::{CreatedAt, EntityTarget, PageMetadata};

#[cfg(test)]
use vmux_ecs::profile::ProfilePaths;
use vmux_ecs::profile::{ActiveProfile, CurrentProfile, SessionEnvironment};
use vmux_ecs::store::{
    AutoSave, AutoSavePaused, AutoSavePlugin, AutoSavePolicy, AutoSaveSet, Phase, State,
    StoreAppExt, StoreDirty, StorePath, StoreWriteRequest, StoreWriteResult,
    WorkspaceCommitCompleted, WorkspaceCommitRequest, WorkspaceRestore, WorkspaceRestoreValidator,
    WorkspaceSaveRequest, WorkspaceStore, WorkspaceStoreValidators, WorkspaceStored,
};
use vmux_ecs::time::BackoffPolicy;
#[cfg(test)]
use vmux_ecs::time::Clock;
#[cfg(test)]
use vmux_ecs::{ArchivedPage, ArchivedPagePosition, ArchivedTabPage};
use vmux_layout::space::Space;
#[cfg(test)]
use vmux_layout::space::SpaceId;
use vmux_layout::window::WindowHierarchy;
use vmux_layout::{LayoutRestoreCommitted, LayoutRestoreRequest, LayoutStartupSet, LayoutStoreSet};
#[cfg(test)]
use vmux_layout::{
    pane::{Pane, PaneId},
    tab::{Tab, TabWorkspace, TabWorktree},
    window::WindowGeometry,
};
pub(crate) struct WorkspaceStorePlugin;

impl Plugin for WorkspaceStorePlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<SchemaPlugin>() {
            app.add_plugins(SchemaPlugin);
        }
        if !app.is_plugin_added::<AutoSavePlugin<WorkspaceStore>>() {
            app.add_plugins(AutoSavePlugin::<WorkspaceStore>::default());
        }
        app.add_plugins(WorkspacePersistencePlugin)
            .add_message::<WorkspaceSaveRequest>()
            .add_message::<LayoutRestoreRequest>()
            .add_message::<LayoutRestoreCommitted>()
            .add_observer(save_on_default_event)
            .add_observer(load_on_default_event)
            .add_systems(
                Startup,
                (spawn, ApplyDeferred, load_on_startup)
                    .chain()
                    .in_set(LayoutStartupSet::Store),
            )
            .add_observer(mark_restore_pending)
            .add_observer(prune_restored)
            .add_observer(restore_children)
            .add_observer(resolve_ready)
            .add_observer(begin_load)
            .add_observer(decoded_snapshot)
            .add_observer(materialized)
            .add_observer(write_autosave)
            .add_systems(
                Update,
                complete_restore
                    .after(LayoutStoreSet::Restore)
                    .run_if(any_with_component::<RestorePending>),
            )
            .add_systems(
                Update,
                (scope_pages, handle_save_requests, drain_writes).chain(),
            )
            .add_systems(Update, track_hierarchy)
            .add_systems(PostUpdate, sync_gate.before(AutoSaveSet::Apply));
    }
}

struct WorkspacePersistencePlugin;

impl Plugin for WorkspacePersistencePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(WorkspaceCommitPlugin)
            .add_message::<AppExit>()
            .add_observer(begin_auto)
            .add_observer(begin_direct)
            .add_observer(captured)
            .add_observer(write_completed)
            .add_observer(direct_completed)
            .add_observer(restore_failed)
            .add_observer(orphaned)
            .add_systems(Update, drain_orphans.after(drain_writes))
            .add_systems(
                Last,
                flush_attached
                    .after(AutoSaveSet::Flush)
                    .before(vmux_ecs::service::ShutdownSet),
            )
            .add_systems(
                Last,
                flush_orphans
                    .after(AutoSaveSet::Flush)
                    .before(vmux_ecs::service::ShutdownSet),
            );
    }
}

struct WorkspaceCommitPlugin;

impl Plugin for WorkspaceCommitPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<vmux_ecs::OperationPlugin>() {
            app.add_plugins(vmux_ecs::OperationPlugin);
        }
        app.add_message::<WorkspaceCommitRequest>()
            .add_message::<WorkspaceCommitCompleted>()
            .add_systems(
                Update,
                (admit, ApplyDeferred, retire, ApplyDeferred, dispatch).chain(),
            )
            .add_systems(Update, settle.after(drain_writes).after(drain_orphans))
            .add_observer(capture)
            .add_observer(receive_written);
    }
}

#[derive(Component)]
struct Commit {
    request: WorkspaceCommitRequest,
    store: Entity,
    revision: u64,
}

impl Commit {
    fn completed(&self, result: Result<(), String>) -> WorkspaceCommitCompleted {
        WorkspaceCommitCompleted {
            request_id: self.request.request_id.clone(),
            requester: self.request.requester,
            profile: self.request.profile,
            store: self.store,
            path: self.request.path.clone(),
            revision: self.revision,
            result,
        }
    }
}

#[derive(Component)]
struct CommitFlight {
    revision: u64,
    capture: vmux_ecs::Uuid,
}

#[derive(Component)]
struct CommitRetired(String);

#[derive(Component)]
struct CommitOutcome(Result<(), String>);

#[derive(Component)]
struct CommitFinished;

#[derive(SystemParam)]
struct CommitSchema<'w, 's> {
    entities: Query<
        'w,
        's,
        EntityRef<'static>,
        (
            With<Save>,
            Without<AutoSave<WorkspaceStore>>,
            Without<Commit>,
            Without<IsResource>,
        ),
    >,
    registry: Res<'w, AppTypeRegistry>,
    components: &'w bevy::ecs::component::Components,
}

#[derive(SystemParam)]
struct CommitAdmission<'w, 's> {
    state: ParamSet<
        'w,
        's,
        (
            SaveAdmission<'w, 's>,
            Query<
                'w,
                's,
                (
                    Entity,
                    &'static StorePath,
                    &'static EntityTarget<ActiveProfile>,
                    &'static mut State,
                    &'static SnapshotFilter,
                    &'static WorkspaceRestore,
                ),
                With<AutoSave<WorkspaceStore>>,
            >,
        ),
    >,
    profile: CurrentProfile<'w, 's>,
}

impl CommitAdmission<'_, '_> {
    fn source(&mut self, request: &WorkspaceCommitRequest) -> Result<Entity, String> {
        if self.profile.entity() != Some(request.profile)
            || self
                .profile
                .paths()
                .is_none_or(|paths| paths.store().join("store.ron") != request.path)
        {
            return Err("Workspace commit profile is not current".into());
        }
        if !self.state.p0().accepts(&request.path) {
            return Err("Workspace save admission refused the commit".into());
        }
        let mut matching = None;
        for (entity, path, owner, state, _, restore) in &self.state.p1() {
            if path.0 != request.path || owner.entity() != request.profile {
                continue;
            }
            if matching.is_some() {
                return Err("Workspace commit source is ambiguous".into());
            }
            if state.phase() != Phase::Ready || !restore.complete {
                return Err("Workspace restore is not ready".into());
            }
            matching = Some(entity);
        }
        matching.ok_or_else(|| "Workspace commit source does not match the profile and path".into())
    }
}

impl CommitSchema<'_, '_> {
    fn required(
        &self,
        entity: Entity,
        filter: &WorldFilter,
    ) -> Result<Vec<std::any::TypeId>, String> {
        let entity = self
            .entities
            .get(entity)
            .map_err(|_| "Required entity is absent")?;
        if !entity.contains::<Save>() {
            return Err("Required entity is not stored".into());
        }
        let registry = self.registry.read();
        let mut required = Vec::new();
        for component in entity.archetype().components() {
            let Some(id) = self
                .components
                .get_info(*component)
                .and_then(|info| info.type_id())
            else {
                continue;
            };
            let Some(registration) = registry.get(id) else {
                continue;
            };
            if registration.data::<WorkspaceStored>().is_none() {
                continue;
            }
            if !filter.is_allowed_by_id(id) || registration.data::<ReflectComponent>().is_none() {
                return Err(
                    "Required registered component is omitted by the workspace schema".into(),
                );
            }
            required.push(id);
        }
        if required.is_empty() {
            return Err("Required entity has no registered workspace data".into());
        }
        Ok(required)
    }

    fn captured(&self, entity: Entity, writer: &SnapshotWriter) -> bool {
        let Ok(required) = self.required(entity, &writer.filter) else {
            return false;
        };
        let Some(captured) = writer
            .saved
            .world
            .entities
            .iter()
            .find(|entry| entry.entity == entity)
        else {
            return false;
        };
        for id in required {
            if !captured.components.iter().any(|value| {
                value
                    .get_represented_type_info()
                    .is_some_and(|info| info.type_id() == id)
            }) {
                return false;
            }
        }
        true
    }
}

fn admit(
    mut requests: MessageReader<WorkspaceCommitRequest>,
    existing: Query<&Commit>,
    mut admission: CommitAdmission,
    requesters: Query<Option<&vmux_ecs::OperationState>>,
    schema: CommitSchema,
    mut replies: MessageWriter<WorkspaceCommitCompleted>,
    mut commands: Commands,
) {
    let mut seen = Vec::<WorkspaceCommitRequest>::new();
    for request in requests.read() {
        let previous = existing
            .iter()
            .find(|commit| commit.request.request_id == request.request_id)
            .map(|commit| &commit.request)
            .or_else(|| {
                seen.iter()
                    .find(|previous| previous.request_id == request.request_id)
            });
        if let Some(previous) = previous {
            if previous != request {
                replies.write(
                    Commit {
                        request: request.clone(),
                        store: Entity::PLACEHOLDER,
                        revision: 0,
                    }
                    .completed(Err(
                        "Commit request identity was reused for different work".into(),
                    )),
                );
            }
            continue;
        }
        seen.push(request.clone());
        let store = match admission.source(request) {
            Ok(store) => store,
            Err(error) => {
                replies.write(
                    Commit {
                        request: request.clone(),
                        store: Entity::PLACEHOLDER,
                        revision: 0,
                    }
                    .completed(Err(error)),
                );
                continue;
            }
        };
        let mut stores = admission.state.p1();
        let (_, _, _, mut state, filter, _) = stores.get_mut(store).unwrap();
        let result = match requesters.get(request.requester) {
            Err(_) => Err("Workspace commit requester is absent".to_string()),
            Ok(Some(vmux_ecs::OperationState::Canceled | vmux_ecs::OperationState::Failed)) => {
                Err("Workspace commit requester is canceled".to_string())
            }
            Ok(_) => schema
                .required(request.required, &filter.components)
                .map(|_| ()),
        };
        if let Err(error) = result {
            replies.write(
                Commit {
                    request: request.clone(),
                    store,
                    revision: 0,
                }
                .completed(Err(error)),
            );
            continue;
        }
        state.advance();
        let revision = state.dirty().unwrap();
        commands.spawn((
            Name::new("Accepted workspace commit"),
            vmux_ecs::Operation::bundle("workspace.commit")
                .unwrap()
                .running(),
            vmux_ecs::OperationRetained,
            Commit {
                request: request.clone(),
                store,
                revision,
            },
        ));
    }
}

fn retire(
    commits: Query<
        (Entity, &Commit, Has<CommitFlight>),
        (
            Without<CommitFinished>,
            Without<CommitOutcome>,
            Without<CommitRetired>,
        ),
    >,
    sources: Query<
        (
            &StorePath,
            &EntityTarget<ActiveProfile>,
            &State,
            &WorkspaceRestore,
        ),
        With<AutoSave<WorkspaceStore>>,
    >,
    requesters: Query<Option<&vmux_ecs::OperationState>>,
    profile: CurrentProfile,
    mut commands: Commands,
) {
    for (entity, commit, flight) in &commits {
        let current = sources
            .get(commit.store)
            .is_ok_and(|(path, source, state, restore)| {
                path.0 == commit.request.path
                    && source.entity() == commit.request.profile
                    && state.phase() == Phase::Ready
                    && restore.complete
            })
            && profile.entity() == Some(commit.request.profile)
            && profile
                .paths()
                .is_some_and(|paths| paths.store().join("store.ron") == commit.request.path);
        let requester = requesters.get(commit.request.requester).is_ok_and(|state| {
            !matches!(
                state,
                Some(vmux_ecs::OperationState::Canceled | vmux_ecs::OperationState::Failed)
            )
        });
        if current && requester {
            continue;
        }
        let error = "Workspace commit source or requester retired".to_string();
        if flight {
            commands.entity(entity).insert(CommitRetired(error));
        } else {
            commands.entity(entity).insert(CommitOutcome(Err(error)));
        }
    }
}

fn dispatch(
    commits: Query<
        &Commit,
        (
            Without<CommitFlight>,
            Without<CommitOutcome>,
            Without<CommitFinished>,
        ),
    >,
    sources: Query<
        (Entity, &State, &StorePath, &SnapshotFilter),
        (With<AutoSave<WorkspaceStore>>, Without<PendingWrite>),
    >,
    saved: SpaceSaveEntities,
    mut saver: SpaceSaver,
) {
    for (entity, state, path, filter) in &sources {
        if state.phase() != Phase::Ready || state.error().is_some() {
            continue;
        }
        if !commits.iter().any(|commit| commit.store == entity) {
            continue;
        }
        saver.autosave(
            entity,
            path.0.clone(),
            saved.excluded(),
            filter.components.clone(),
            true,
        );
    }
}

#[derive(Event)]
struct SnapshotCaptured {
    capture: vmux_ecs::Uuid,
    target: EntityTarget<AutoSave<WorkspaceStore>>,
    revision: u64,
    commit: bool,
    writer: Option<SnapshotWriter>,
}

fn capture(
    mut captured: On<SnapshotCaptured>,
    mut commits: Query<
        (Entity, &mut Commit),
        (
            Without<CommitFlight>,
            Without<CommitOutcome>,
            Without<CommitFinished>,
        ),
    >,
    schema: CommitSchema,
    profile: CurrentProfile,
    requesters: Query<Option<&vmux_ecs::OperationState>>,
    mut stores: Query<&mut State, With<AutoSave<WorkspaceStore>>>,
    restores: Query<&WorkspaceRestore, With<AutoSave<WorkspaceStore>>>,
    mut commands: Commands,
) {
    let Some(writer) = captured.writer.take() else {
        return;
    };
    let ready = stores.get_mut(captured.target.entity()).is_ok_and(|state| {
        state.phase() == Phase::Ready && state.saving() == Some(captured.revision)
    }) && restores
        .get(captured.target.entity())
        .is_ok_and(|restore| restore.complete);
    let mut included = false;
    for (entity, mut commit) in &mut commits {
        if commit.store != captured.target.entity()
            || commit.request.path != writer.path
            || commit.revision > captured.revision
        {
            continue;
        }
        let requester = requesters.get(commit.request.requester).is_ok_and(|state| {
            !matches!(
                state,
                Some(vmux_ecs::OperationState::Canceled | vmux_ecs::OperationState::Failed)
            )
        });
        let source = profile.entity() == Some(commit.request.profile)
            && profile
                .paths()
                .is_some_and(|paths| paths.store().join("store.ron") == writer.path);
        if !ready || !requester || !source || !schema.captured(commit.request.required, &writer) {
            commands.entity(entity).insert(CommitOutcome(Err(
                "Required workspace data was not captured".into(),
            )));
            continue;
        }
        commit.revision = captured.revision;
        commands.entity(entity).insert(CommitFlight {
            revision: captured.revision,
            capture: captured.capture.clone(),
        });
        included = true;
    }
    if captured.commit && !included {
        if let Ok(mut state) = stores.get_mut(captured.target.entity()) {
            state.retry(
                captured.revision,
                "Workspace commit has no current included requirement".into(),
            );
        }
        return;
    }
    commands.queue(SubmitSnapshot {
        capture: captured.capture.clone(),
        target: captured.target,
        revision: captured.revision,
        writer,
    });
}

fn receive_written(
    written: On<WriteCompleted>,
    commits: Query<(Entity, &Commit, &CommitFlight, Option<&CommitRetired>)>,
    mut commands: Commands,
) {
    for (entity, commit, flight, retired) in &commits {
        if commit.store != written.target.entity()
            || commit.request.path != written.path
            || flight.revision != written.revision
            || flight.capture != written.capture
        {
            continue;
        }
        commands.entity(entity).remove::<CommitFlight>();
        if let Some(retired) = retired {
            commands
                .entity(entity)
                .insert(CommitOutcome(Err(retired.0.clone())));
        } else if written.result.is_ok() {
            commands.entity(entity).insert(CommitOutcome(Ok(())));
        }
    }
}

fn settle(
    commits: Query<
        (Entity, &Commit, &CommitOutcome),
        (Without<CommitFlight>, Without<CommitFinished>),
    >,
    profile: CurrentProfile,
    requesters: Query<Option<&vmux_ecs::OperationState>>,
    sources: Query<
        (
            &StorePath,
            &EntityTarget<ActiveProfile>,
            &State,
            &WorkspaceRestore,
        ),
        With<AutoSave<WorkspaceStore>>,
    >,
    mut replies: MessageWriter<WorkspaceCommitCompleted>,
    mut commands: Commands,
) {
    for (entity, commit, outcome) in &commits {
        let mut result = outcome.0.clone();
        let requester = requesters.get(commit.request.requester).is_ok_and(|state| {
            !matches!(
                state,
                Some(vmux_ecs::OperationState::Canceled | vmux_ecs::OperationState::Failed)
            )
        });
        let source = sources
            .get(commit.store)
            .is_ok_and(|(path, profile, state, restore)| {
                path.0 == commit.request.path
                    && profile.entity() == commit.request.profile
                    && state.phase() == Phase::Ready
                    && restore.complete
            });
        if profile.entity() != Some(commit.request.profile)
            || !requester
            || !source
            || profile
                .paths()
                .is_none_or(|paths| paths.store().join("store.ron") != commit.request.path)
        {
            result = Err("Workspace commit delivery owner retired".into());
        }
        replies.write(commit.completed(result.clone()));
        commands.trigger(vmux_ecs::OperationCompleted { entity, result });
        commands
            .entity(entity)
            .remove::<vmux_ecs::OperationRetained>()
            .insert(CommitFinished);
    }
}

struct SchemaPlugin;

impl Plugin for SchemaPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(vmux_ecs::store::SchemaPlugin::<PageStore>::default())
            .track_store::<Save>();
    }
}

#[vmux_ecs::store(domain = WorkspaceStore, track;
    PageMetadata,
    vmux_ecs::WorkspaceKind,
    vmux_ecs::Cwd,
    vmux_ecs::Description,
    vmux_ecs::CollectionView,
    CreatedAt,
    vmux_ecs::LastActivatedAt,
    vmux_ecs::ArchivedAt,
    vmux_ecs::Order,
)]
struct PageStore;

#[derive(Component)]
struct ScopedPage;

type WorkspacePages<'w, 's> = Query<
    'w,
    's,
    (Entity, Option<&'static ChildOf>, Has<ScopedPage>, Has<Save>),
    With<PageMetadata>,
>;

fn scope_pages(
    pages: WorkspacePages,
    retired: Query<Entity, (With<ScopedPage>, Without<PageMetadata>)>,
    stacks: Query<(), With<vmux_layout::stack::Stack>>,
    mut commands: Commands,
) {
    for (entity, parent, scoped, saved) in &pages {
        if parent.is_some_and(|parent| stacks.contains(parent.parent())) {
            if !scoped || !saved {
                commands.entity(entity).insert((Save, ScopedPage));
            }
        } else if scoped {
            commands.entity(entity).remove::<(Save, ScopedPage)>();
        }
    }
    for entity in &retired {
        commands.entity(entity).remove::<(Save, ScopedPage)>();
    }
}

fn spawn(registry: Res<AppTypeRegistry>, profile: CurrentProfile, mut commands: Commands) {
    let components = WorkspaceStored::filter(&registry.read())
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
    let mut store = commands.spawn((
        Name::new("Space store"),
        AutoSave::<WorkspaceStore>::default(),
        AutoSavePolicy::new(
            std::time::Duration::from_millis(500),
            Some(std::time::Duration::from_secs(60)),
            BackoffPolicy::exponential(
                std::time::Duration::from_secs(60),
                std::time::Duration::from_secs(60),
            ),
        ),
        SnapshotFilter { components },
        WorkspaceRestore::default(),
        StorePath::new(path),
    ));
    if SessionEnvironment::is_test() {
        store.insert(AutoSavePaused);
    }
    if let Some(profile) = profile.entity() {
        store.insert(EntityTarget::<ActiveProfile>::new(profile));
    }
}

fn sync_gate(
    refused: Option<Res<WorkspaceLoadRefused>>,
    spaces: Query<(), With<Space>>,
    stores: Query<
        (
            Entity,
            &WorkspaceRestore,
            Option<&RestoreFailure>,
            Has<AutoSavePaused>,
        ),
        With<AutoSave<WorkspaceStore>>,
    >,
    mut commands: Commands,
) {
    let blocked = SessionEnvironment::is_test() || refused.is_some() || spaces.is_empty();
    for (entity, restore, failure, paused) in &stores {
        let should_pause =
            blocked || !restore.complete || failure.is_some_and(|failure| failure.backup.is_none());
        if should_pause && !paused {
            commands.entity(entity).insert(AutoSavePaused);
        } else if !should_pause && paused {
            commands.entity(entity).remove::<AutoSavePaused>();
        }
    }
}

fn write_autosave(
    request: On<StoreWriteRequest<WorkspaceStore>>,
    admission: SaveAdmission,
    stores: Query<(&SnapshotFilter, Has<PendingWrite>), With<AutoSave<WorkspaceStore>>>,
    save_entities: SpaceSaveEntities,
    mut saver: SpaceSaver,
    mut commands: Commands,
) {
    if !admission.accepts(&request.path) {
        commands.trigger(StoreWriteResult::<WorkspaceStore>::err(
            request.entity,
            request.revision,
            "Workspace save admission refused the write",
        ));
        return;
    }
    let Ok((filter, pending)) = stores.get(request.entity) else {
        return;
    };
    if request.flush {
        saver.save(
            request.path.clone(),
            save_entities.excluded(),
            filter.components.clone(),
        );
        commands.trigger(StoreWriteResult::<WorkspaceStore>::ok(
            request.entity,
            request.revision,
        ));
        return;
    }
    if pending {
        commands.trigger(StoreWriteResult::<WorkspaceStore>::err(
            request.entity,
            request.revision,
            "Workspace write already pending",
        ));
        return;
    }
    let save = SpaceSaver::snapshot(
        request.path.clone(),
        save_entities.excluded(),
        filter.components.clone(),
    );
    commands.queue(CaptureSnapshot {
        capture: vmux_ecs::Uuid(uuid::Uuid::new_v4().to_string()),
        target: EntityTarget::new(request.entity),
        revision: request.revision,
        path: request.path.clone(),
        save,
        commit: false,
    });
}

fn handle_save_requests(
    mut requests: MessageReader<WorkspaceSaveRequest>,
    save_entities: SpaceSaveEntities,
    store: Single<&SnapshotFilter, With<AutoSave<WorkspaceStore>>>,
    mut saver: SpaceSaver,
) {
    for request in requests.read() {
        saver.save(
            request.path.clone(),
            save_entities.excluded(),
            store.components.clone(),
        );
    }
}

#[derive(Component)]
struct RestorePending;

fn prune_restored(
    loaded: On<Loaded>,
    sessions: Query<(), With<vmux_session::Session>>,
    candidates: Query<EntityRef, (With<CreatedAt>, With<ChildOf>)>,
    store: Single<&StorePath, With<AutoSave<WorkspaceStore>>>,
    mut commands: Commands,
) {
    let mut parents = std::collections::HashMap::<Entity, Vec<Entity>>::new();
    for entity in loaded.entities() {
        let Ok(candidate) = candidates.get(entity) else {
            continue;
        };
        let expected = if candidate.contains::<Save>() { 3 } else { 2 };
        if candidate.archetype().components().len() != expected {
            continue;
        }
        let parent = candidate.get::<ChildOf>().unwrap().parent();
        if sessions.contains(parent) {
            parents.entry(parent).or_default().push(entity);
        }
    }
    if !parents.is_empty() {
        commands.queue(RestoredChildren {
            parents,
            store: store.0.clone(),
        });
    }
}

#[derive(Event)]
struct RestoredChildren {
    parents: std::collections::HashMap<Entity, Vec<Entity>>,
    store: PathBuf,
}

impl Command for RestoredChildren {
    type Out = ();

    fn apply(self, world: &mut World) {
        world.trigger(self);
    }
}

fn restore_children(
    request: On<RestoredChildren>,
    current: Query<&Children>,
    entities: Query<()>,
    mut commands: Commands,
) {
    let backup = request
        .store
        .with_extension("ron.before-empty-session-children");
    if request.store.is_file()
        && !backup.exists()
        && let Err(error) = std::fs::copy(&request.store, &backup)
    {
        warn!("Unable to back up workspace before restoring session children: {error}");
        return;
    }
    let mut removed = 0;
    for (parent, children) in &request.parents {
        let discarded = children
            .iter()
            .copied()
            .collect::<std::collections::HashSet<_>>();
        let Ok(current) = current.get(*parent) else {
            continue;
        };
        let mut retained = Vec::new();
        for child in current.iter() {
            if !discarded.contains(&child) {
                retained.push(child);
            }
        }
        commands
            .entity(*parent)
            .replace_related::<ChildOf>(&retained);
        for child in children {
            if entities.contains(*child) {
                commands.entity(*child).try_despawn();
                removed += 1;
            }
        }
    }
    if removed > 0 {
        info!("Removed {removed} empty restored session children");
        commands.trigger(StoreDirty);
    }
}

#[derive(EntityEvent)]
struct RestoreReady {
    #[event_target]
    store: Entity,
}

fn resolve_ready(
    request: On<RestoreReady>,
    mut stores: Query<&mut State, With<AutoSave<WorkspaceStore>>>,
) {
    if let Ok(mut state) = stores.get_mut(request.store) {
        state.resolve();
    }
}

fn mark_restore_pending(
    _trigger: On<Loaded>,
    store: Single<Entity, With<AutoSave<WorkspaceStore>>>,
    mut restore_requests: MessageWriter<LayoutRestoreRequest>,
    mut commands: Commands,
) {
    commands.entity(*store).insert(RestorePending);
    commands.trigger(RestoreReady { store: *store });
    restore_requests.write(LayoutRestoreRequest);
}

fn complete_restore(
    mut committed: MessageReader<LayoutRestoreCommitted>,
    mut restore: Single<&mut WorkspaceRestore>,
    mut state: Single<&mut State, With<AutoSave<WorkspaceStore>>>,
    store: Single<Entity, With<RestorePending>>,
    mut commands: Commands,
) {
    if committed.read().count() == 0 {
        return;
    }
    restore.complete = true;
    state.ready();
    commands.entity(*store).remove::<RestorePending>();
}

#[derive(Component)]
struct SnapshotFilter {
    components: WorldFilter,
}

#[derive(Component, PartialEq, Eq)]
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
    previous: Query<&HierarchyState>,
    mut commands: Commands,
) {
    let mut dirty = false;
    let mut removed = std::collections::HashSet::new();
    for entity in removed_saved.read() {
        removed.insert(entity);
        commands.entity(entity).try_remove::<HierarchyState>();
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
        if !removed.contains(&entity)
            && previous
                .get(entity)
                .is_ok_and(|previous| previous.matches(name, parent, children))
        {
            continue;
        }
        commands
            .entity(entity)
            .insert(HierarchyState::of(name, parent, children));
        dirty = true;
    }
    if dirty {
        commands.trigger(StoreDirty);
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

#[derive(Resource)]
struct WorkspaceLoadRefused;

struct Warning {
    message: &'static str,
    path: PathBuf,
}

impl Command for Warning {
    type Out = ();

    fn apply(self, _world: &mut World) {
        if cfg!(test) || SessionEnvironment::is_test() {
            return;
        }
        let path = self.path.to_string_lossy();
        let description = vmux_ui::i18n::translate_with(
            self.message,
            &[("path", vmux_ui::i18n::TranslationValue::String(&path))],
        );
        rfd::MessageDialog::new()
            .set_title(vmux_ui::i18n::translate("workspace-store-title"))
            .set_description(description)
            .set_level(rfd::MessageLevel::Warning)
            .show();
    }
}

impl SpaceSaver<'_, '_> {
    fn save(
        &mut self,
        path: PathBuf,
        excluded: impl IntoIterator<Item = Entity>,
        components: WorldFilter,
    ) {
        let save = Self::snapshot(path.clone(), excluded, components);
        self.commands.trigger(ImmediateSave {
            request: WorkspaceSaveRequest { path },
            save: Some(save),
        });
    }

    fn autosave(
        &mut self,
        entity: Entity,
        path: PathBuf,
        excluded: impl IntoIterator<Item = Entity>,
        components: WorldFilter,
        commit: bool,
    ) {
        let save = Self::snapshot(path.clone(), excluded, components);
        self.commands.trigger(AutoSnapshot {
            target: EntityTarget::new(entity),
            path,
            save: Some(save),
            commit,
        });
    }

    fn snapshot(
        path: PathBuf,
        excluded: impl IntoIterator<Item = Entity>,
        components: WorldFilter,
    ) -> SaveWorld {
        let mut save = SaveWorld::default_into_file(path);
        save.entities = EntityFilter::block(excluded);
        save.components = components;
        save
    }
}

#[derive(Event)]
struct AutoSnapshot {
    target: EntityTarget<AutoSave<WorkspaceStore>>,
    path: PathBuf,
    save: Option<SaveWorld>,
    commit: bool,
}

#[derive(Event)]
struct ImmediateSave {
    request: WorkspaceSaveRequest,
    save: Option<SaveWorld>,
}

#[derive(Event)]
struct Captured {
    target: EntityTarget<AutoSave<WorkspaceStore>>,
    pending: Option<PendingWrite>,
}

#[derive(Event)]
struct WriteCompleted {
    capture: vmux_ecs::Uuid,
    target: EntityTarget<AutoSave<WorkspaceStore>>,
    revision: u64,
    path: PathBuf,
    result: Result<(), String>,
}

#[derive(Event)]
struct DirectCompleted {
    result: Result<(), String>,
}

#[derive(Event)]
struct Orphaned {
    target: EntityTarget<AutoSave<WorkspaceStore>>,
    pending: Option<PendingWrite>,
}

#[derive(Component)]
struct OrphanWrite;

#[derive(Event)]
struct Written;

#[derive(bevy::ecs::system::SystemParam)]
struct SaveAdmission<'w, 's> {
    refused: Option<Res<'w, WorkspaceLoadRefused>>,
    stores: Query<'w, 's, (&'static StorePath, &'static State)>,
    failures: Query<'w, 's, (&'static StorePath, &'static RestoreFailure)>,
}

impl SaveAdmission<'_, '_> {
    fn accepts(&self, path: &Path) -> bool {
        if SessionEnvironment::is_test() || self.refused.is_some() {
            return false;
        }
        for (store, state) in &self.stores {
            if store.0 == path && state.phase() != Phase::Ready {
                return false;
            }
        }
        for (store, failure) in &self.failures {
            if store.0 == path && failure.backup.is_none() {
                warn!(
                    "Workspace save blocked until the failed restore can be backed up: {:?}",
                    store.0
                );
                return false;
            }
        }
        true
    }
}

fn begin_auto(
    mut request: On<AutoSnapshot>,
    mut state: ParamSet<(
        SaveAdmission,
        Query<(&mut State, Has<PendingWrite>), With<AutoSave<WorkspaceStore>>>,
    )>,
    mut commands: Commands,
) {
    let request = request.event_mut();
    if request.save.is_none() || !state.p0().accepts(&request.path) {
        return;
    }
    let mut stores = state.p1();
    let Ok((mut state, pending)) = stores.get_mut(request.target.entity()) else {
        return;
    };
    if pending {
        return;
    }
    let Some(revision) = state.begin() else {
        return;
    };
    let Some(save) = request.save.take() else {
        return;
    };
    commands.queue(CaptureSnapshot {
        capture: vmux_ecs::Uuid(uuid::Uuid::new_v4().to_string()),
        target: request.target,
        revision,
        path: request.path.clone(),
        save,
        commit: request.commit,
    });
}

fn begin_direct(
    mut request: On<ImmediateSave>,
    admission: SaveAdmission,
    mut pending: Query<(
        Entity,
        Option<&EntityTarget<AutoSave<WorkspaceStore>>>,
        Has<OrphanWrite>,
        &mut PendingWrite,
    )>,
    mut commands: Commands,
) {
    let request = request.event_mut();
    if request.save.is_none() || !admission.accepts(&request.request.path) {
        return;
    }
    for (entity, target, orphan, mut write) in &mut pending {
        if write.path != request.request.path {
            continue;
        }
        let result = write.finish();
        commands.trigger(WriteCompleted {
            capture: write.capture.clone(),
            target: target.copied().unwrap_or_else(|| EntityTarget::new(entity)),
            revision: write.revision,
            path: write.path.clone(),
            result,
        });
        if orphan {
            commands.entity(entity).despawn();
        } else {
            commands.entity(entity).remove::<PendingWrite>();
        }
    }
    let Some(save) = request.save.take() else {
        return;
    };
    commands.queue(SnapshotSave {
        request: WorkspaceSaveRequest {
            path: request.request.path.clone(),
        },
        save,
    });
}

fn captured(
    mut request: On<Captured>,
    stores: Query<&State, With<AutoSave<WorkspaceStore>>>,
    mut commands: Commands,
) {
    let request = request.event_mut();
    let Some(pending) = request.pending.take() else {
        return;
    };
    if stores
        .get(request.target.entity())
        .is_ok_and(|state| state.saving() == Some(pending.revision))
    {
        commands.entity(request.target.entity()).insert(pending);
    } else {
        commands.trigger(Orphaned {
            target: request.target,
            pending: Some(pending),
        });
    }
}

fn write_completed(
    request: On<WriteCompleted>,
    stores: Query<(&State, Option<&PendingWrite>), With<AutoSave<WorkspaceStore>>>,
    mut commands: Commands,
) {
    if let Err(error) = &request.result {
        warn!("Workspace save failed for {:?}: {error}", request.path);
    }
    let Ok((state, pending)) = stores.get(request.target.entity()) else {
        return;
    };
    if pending.is_some_and(|pending| pending.capture != request.capture || pending.result.is_some())
    {
        return;
    }
    let matches = state.saving() == Some(request.revision);
    commands.trigger(StoreWriteResult::<WorkspaceStore>::new(
        request.target.entity(),
        request.revision,
        request.result.clone(),
    ));
    if matches && request.result.is_ok() {
        commands.trigger(Written);
    }
}

fn direct_completed(request: On<DirectCompleted>, mut commands: Commands) {
    match &request.result {
        Ok(()) => {
            commands.trigger(Written);
        }
        Err(error) => warn!("{error}"),
    }
}

fn preserve_pending(
    mut world: bevy::ecs::world::DeferredWorld,
    context: bevy::ecs::lifecycle::HookContext,
) {
    let target = world
        .get::<EntityTarget<AutoSave<WorkspaceStore>>>(context.entity)
        .copied()
        .unwrap_or_else(|| EntityTarget::new(context.entity));
    let pending = {
        let Some(mut pending) = world.get_mut::<PendingWrite>(context.entity) else {
            return;
        };
        let Some(result) = pending.result.take() else {
            return;
        };
        PendingWrite {
            capture: pending.capture.clone(),
            revision: pending.revision,
            path: pending.path.clone(),
            result: Some(result),
        }
    };
    world.commands().trigger(Orphaned {
        target,
        pending: Some(pending),
    });
}

fn orphaned(mut request: On<Orphaned>, mut commands: Commands) {
    let request = request.event_mut();
    let Some(pending) = request.pending.take() else {
        return;
    };
    commands.spawn((
        vmux_ecs::Operation::bundle("workspace.orphan-write")
            .unwrap()
            .running(),
        OrphanWrite,
        request.target,
        pending,
    ));
}

fn drain_orphans(
    mut pending: Query<
        (
            Entity,
            &EntityTarget<AutoSave<WorkspaceStore>>,
            &mut PendingWrite,
        ),
        With<OrphanWrite>,
    >,
    mut commands: Commands,
) {
    for (entity, target, mut write) in &mut pending {
        let Some(result) = write.poll() else {
            continue;
        };
        commands.trigger(WriteCompleted {
            capture: write.capture.clone(),
            target: *target,
            revision: write.revision,
            path: write.path.clone(),
            result,
        });
        commands.entity(entity).despawn();
    }
}

fn flush_orphans(
    mut exits: MessageReader<AppExit>,
    mut pending: Query<
        (
            Entity,
            &EntityTarget<AutoSave<WorkspaceStore>>,
            &mut PendingWrite,
        ),
        With<OrphanWrite>,
    >,
    mut commands: Commands,
) {
    if exits.read().next().is_none() {
        return;
    }
    for (entity, target, mut write) in &mut pending {
        let result = write.finish();
        commands.trigger(WriteCompleted {
            capture: write.capture.clone(),
            target: *target,
            revision: write.revision,
            path: write.path.clone(),
            result,
        });
        commands.entity(entity).despawn();
    }
}

fn flush_attached(
    mut exits: MessageReader<AppExit>,
    mut pending: Query<(Entity, &mut PendingWrite), With<AutoSave<WorkspaceStore>>>,
    mut commands: Commands,
) {
    if exits.read().next().is_none() {
        return;
    }
    for (entity, mut write) in &mut pending {
        let result = write.finish();
        commands.trigger(WriteCompleted {
            capture: write.capture.clone(),
            target: EntityTarget::new(entity),
            revision: write.revision,
            path: write.path.clone(),
            result,
        });
        commands.entity(entity).remove::<PendingWrite>();
    }
}

#[derive(Component)]
#[component(on_discard = preserve_pending)]
struct PendingWrite {
    capture: vmux_ecs::Uuid,
    revision: u64,
    path: PathBuf,
    result: Option<Mutex<mpsc::Receiver<Result<(), String>>>>,
}

impl PendingWrite {
    fn poll(&mut self) -> Option<Result<(), String>> {
        let result = self.result.as_mut()?.get_mut().unwrap().try_recv();
        match result {
            Ok(result) => {
                self.result = None;
                Some(result)
            }
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                self.result = None;
                Some(Err("Workspace writer stopped before completion".to_string()))
            }
        }
    }

    fn finish(&mut self) -> Result<(), String> {
        self.result
            .take()
            .ok_or("workspace writer result already consumed")?
            .into_inner()
            .unwrap()
            .recv()
            .map_err(|error| error.to_string())?
    }
}

struct SnapshotWriter {
    saved: Saved,
    registry: AppTypeRegistry,
    path: PathBuf,
    filter: WorldFilter,
}

impl SnapshotWriter {
    fn write(self) -> Result<(), String> {
        let data = self
            .saved
            .world
            .serialize(&self.registry.read())
            .map_err(|error| error.to_string())?;
        Self::write_snapshot(&self.path, &data).map_err(|error| error.to_string())?;
        Self::write_schema_version(&self.path).map_err(|error| error.to_string())
    }

    fn write_snapshot(path: &Path, data: &str) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let pending = path.with_extension(format!("ron.{}.pending", std::process::id()));
        std::fs::write(&pending, data)?;
        if let Err(error) = std::fs::rename(&pending, path) {
            let _ = std::fs::remove_file(pending);
            return Err(error);
        }
        Ok(())
    }

    fn write_schema_version(path: &Path) -> std::io::Result<()> {
        vmux_path::AtomicFile::write(
            RestoreFailure::store_version(path),
            STORE_SCHEMA_VERSION.to_string().as_bytes(),
        )
    }
}

struct CaptureSnapshot {
    capture: vmux_ecs::Uuid,
    target: EntityTarget<AutoSave<WorkspaceStore>>,
    revision: u64,
    path: PathBuf,
    save: SaveWorld,
    commit: bool,
}

impl Command for CaptureSnapshot {
    type Out = ();

    fn apply(mut self, world: &mut World) {
        let mut entities = Vec::new();
        for entity in world.query_filtered::<Entity, With<Save>>().iter(world) {
            let included = match &self.save.entities {
                EntityFilter::Allow(allowed) => allowed.contains(&entity),
                EntityFilter::Block(blocked) => !blocked.contains(&entity),
            };
            if included {
                entities.push(entity);
            }
        }
        self.save.before_save(world);
        self.save.before_serialize(world, &entities);
        let registry = world.resource::<AppTypeRegistry>().clone();
        let filter = self.save.component_filter();
        let snapshot = {
            let registry = registry.read();
            DynamicWorldBuilder::from_world(world, &registry)
                .with_component_filter(filter.clone())
                .with_resource_filter(self.save.resource_filter())
                .extract_resources()
                .extract_entities(entities.iter().copied())
                .build()
        };
        let captured = Ok(Saved { world: snapshot });
        self.save.after_save(world, &captured);
        let Ok(saved) = captured else {
            return;
        };
        let writer = SnapshotWriter {
            saved,
            registry,
            path: self.path,
            filter,
        };
        world.trigger(SnapshotCaptured {
            capture: self.capture,
            target: self.target,
            revision: self.revision,
            commit: self.commit,
            writer: Some(writer),
        });
    }
}

struct SubmitSnapshot {
    capture: vmux_ecs::Uuid,
    target: EntityTarget<AutoSave<WorkspaceStore>>,
    revision: u64,
    writer: SnapshotWriter,
}

impl Command for SubmitSnapshot {
    type Out = ();

    fn apply(self, world: &mut World) {
        let path = self.writer.path.clone();
        let writer = self.writer;
        let proxy = world
            .get_resource::<EventLoopProxyWrapper>()
            .map(|proxy| (**proxy).clone());
        let (send, result) = mpsc::channel();
        IoTaskPool::get()
            .spawn(async move {
                let _ = send.send(writer.write());
                if let Some(proxy) = proxy {
                    let _ = proxy.send_event(WinitUserEvent::WakeUp);
                }
            })
            .detach();
        world.trigger(Captured {
            target: self.target,
            pending: Some(PendingWrite {
                capture: self.capture,
                revision: self.revision,
                path,
                result: Some(Mutex::new(result)),
            }),
        });
    }
}

struct SnapshotSave {
    request: WorkspaceSaveRequest,
    save: SaveWorld,
}

impl Command for SnapshotSave {
    type Out = ();

    fn apply(self, world: &mut World) {
        let path = self.request.path;
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let result = match SaveCommand(self.save).apply(world) {
            Ok(()) => SnapshotWriter::write_schema_version(&path)
                .map_err(|error| format!("Workspace schema save failed for {:?}: {error}", path)),
            Err(error) => Err(format!("Workspace save failed for {:?}: {error}", path)),
        };
        world.trigger(DirectCompleted { result });
    }
}

fn drain_writes(
    mut writes: Query<(Entity, &mut PendingWrite), With<AutoSave<WorkspaceStore>>>,
    mut commands: Commands,
) {
    for (entity, mut pending) in &mut writes {
        let Some(result) = pending.poll() else {
            continue;
        };
        commands.trigger(WriteCompleted {
            capture: pending.capture.clone(),
            target: EntityTarget::new(entity),
            revision: pending.revision,
            path: pending.path.clone(),
            result,
        });
        commands.entity(entity).remove::<PendingWrite>();
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

#[cfg(test)]
fn save_space_to_path(app: &mut App, path: PathBuf) {
    if !app.is_plugin_added::<WorkspacePersistencePlugin>() {
        app.add_plugins(WorkspacePersistencePlugin);
    }
    let world = app.world_mut();
    let components = WorkspaceStored::filter(&world.resource::<AppTypeRegistry>().read())
        .allow::<Save>()
        .allow::<ChildOf>()
        .allow::<Children>()
        .allow::<Name>();
    let save = SpaceSaver::snapshot(path.clone(), std::iter::empty(), components);
    world.trigger(ImmediateSave {
        request: WorkspaceSaveRequest { path },
        save: Some(save),
    });
    world.flush();
}

fn load_on_startup(
    registry: Res<AppTypeRegistry>,
    validators: WorkspaceStoreValidators,
    mut restore: Single<&mut WorkspaceRestore>,
    path: Single<&StorePath, With<AutoSave<WorkspaceStore>>>,
    owner: Single<Entity, With<AutoSave<WorkspaceStore>>>,
    mut commands: Commands,
) {
    if SessionEnvironment::is_test() {
        restore.complete = true;
        restore.store_present = false;
        return;
    }
    let path = &path.0;
    let version = RestoreFailure::schema_version(path);
    if path.exists() && version.is_some_and(|version| version > STORE_SCHEMA_VERSION) {
        error!(
            "Refusing newer workspace {:?}; the original store is unchanged.",
            path
        );
        restore.store_present = true;
        commands.insert_resource(WorkspaceLoadRefused);
        commands.write_message(AppExit::error());
        commands.queue(Warning {
            message: "workspace-store-refused",
            path: path.clone(),
        });
        return;
    }
    if let Ok(body) = std::fs::read_to_string(path) {
        let registry = registry.read();
        if version != Some(STORE_SCHEMA_VERSION)
            || validators.rejected_by(&body).is_some()
            || space_has_unregistered_types(&body, &registry)
        {
            match RestoreFailure::backup_for_reset(path) {
                Ok(backup) => {
                    warn!(
                        ?backup,
                        "Preserved an older or incompatible workspace before resetting"
                    );
                    commands.queue(Warning {
                        message: "workspace-store-recovered",
                        path: backup,
                    });
                }
                Err(error) => {
                    error!(%error, "Cannot preserve workspace {:?}; refusing startup", path);
                    commands.insert_resource(WorkspaceLoadRefused);
                    commands.write_message(AppExit::error());
                    commands.queue(Warning {
                        message: "workspace-store-refused",
                        path: path.clone(),
                    });
                    return;
                }
            }
        }
    }
    WorkspaceStoreSource::normalize_if_needed(path);
    let exists = path.exists();
    restore.store_present = exists;
    if exists {
        info!("Loading space from {:?}", path);
        let load = match std::fs::read_to_string(path)
            .ok()
            .and_then(|body| vmux_api::PageIcon::normalize_persisted(&body))
        {
            Some((body, unknown)) => {
                warn!(?unknown, "Replacing unavailable persisted page icons");
                LoadWorld::default_from_stream(std::io::Cursor::new(body.into_bytes()))
            }
            None => LoadWorld::default_from_file(path.clone()),
        };
        commands.queue(WorkspaceLoad {
            load: Some(load),
            store: *owner,
            path: path.clone(),
        });
    } else {
        restore.complete = true;
    }
}

#[derive(Event)]
struct WorkspaceLoad {
    load: Option<LoadWorld>,
    store: Entity,
    path: PathBuf,
}

impl Command for WorkspaceLoad {
    type Out = ();

    fn apply(self, world: &mut World) {
        world.trigger(self);
    }
}

fn begin_load(
    mut request: On<WorkspaceLoad>,
    mut stores: Query<&mut State, With<AutoSave<WorkspaceStore>>>,
    mut commands: Commands,
) {
    let request = request.event_mut();
    let Some(load) = request.load.take() else {
        return;
    };
    let Ok(mut state) = stores.get_mut(request.store) else {
        return;
    };
    state.materialize();
    commands.queue(DecodeSnapshot {
        target: EntityTarget::new(request.store),
        path: request.path.clone(),
        load,
    });
}

struct DecodeSnapshot {
    target: EntityTarget<AutoSave<WorkspaceStore>>,
    path: PathBuf,
    load: LoadWorld,
}

impl Command for DecodeSnapshot {
    type Out = ();

    fn apply(mut self, world: &mut World) {
        let snapshot = {
            let registry = world.resource::<AppTypeRegistry>().read();
            let mut assets = world.resource::<AssetServer>().clone();
            self.snapshot(&registry, &mut assets)
        };
        world.trigger(DecodedSnapshot {
            target: self.target,
            path: self.path,
            load: Some(self.load),
            snapshot: Some(snapshot),
        });
    }
}

#[derive(Event)]
struct DecodedSnapshot {
    target: EntityTarget<AutoSave<WorkspaceStore>>,
    path: PathBuf,
    load: Option<LoadWorld>,
    snapshot: Option<Result<bevy_world_serialization::DynamicWorld, String>>,
}

fn decoded_snapshot(
    mut request: On<DecodedSnapshot>,
    validators: Query<&WorkspaceRestoreValidator>,
    stores: Query<(), With<AutoSave<WorkspaceStore>>>,
    mut commands: Commands,
) {
    let request = request.event_mut();
    if !stores.contains(request.target.entity()) {
        return;
    }
    let Some(result) = request.snapshot.take() else {
        return;
    };
    let snapshot = match result {
        Ok(snapshot) => snapshot,
        Err(error) => {
            commands.trigger(WorkspaceRestoreFailed {
                store: request.target.entity(),
                path: request.path.clone(),
                error,
            });
            return;
        }
    };
    for validator in &validators {
        if let Err(error) = (validator.validate)(&snapshot) {
            commands.trigger(WorkspaceRestoreFailed {
                store: request.target.entity(),
                path: request.path.clone(),
                error: format!("{}: {error}", validator.name),
            });
            return;
        }
    }
    let Some(mut load) = request.load.take() else {
        return;
    };
    load.input = LoadInput::World(snapshot);
    commands.queue(MaterializeSnapshot {
        target: request.target,
        path: request.path.clone(),
        load,
    });
}

struct MaterializeSnapshot {
    target: EntityTarget<AutoSave<WorkspaceStore>>,
    path: PathBuf,
    load: LoadWorld,
}

impl Command for MaterializeSnapshot {
    type Out = ();

    fn apply(self, world: &mut World) {
        let mut baseline = std::collections::HashSet::new();
        for archetype in world.archetypes().iter() {
            for entity in archetype.entities() {
                baseline.insert(entity.id());
            }
        }
        let result = LoadCommand(self.load)
            .apply(world)
            .map_err(|error| error.to_string());
        if result.is_err() {
            let mut resources = Vec::new();
            for (_, entity) in world.resource_entities().iter() {
                if !baseline.contains(&entity) {
                    resources.push(entity);
                }
            }
            for entity in resources {
                world.entity_mut(entity).remove::<IsResource>();
            }
            world.flush();
            let mut discarded = Vec::new();
            for archetype in world.archetypes().iter() {
                for entity in archetype.entities() {
                    if !baseline.contains(&entity.id()) {
                        discarded.push(entity.id());
                    }
                }
            }
            for entity in discarded {
                world.despawn(entity);
            }
        }
        world.trigger(Materialized {
            target: self.target,
            path: self.path,
            result,
        });
    }
}

#[derive(Event)]
struct Materialized {
    target: EntityTarget<AutoSave<WorkspaceStore>>,
    path: PathBuf,
    result: Result<(), String>,
}

fn materialized(request: On<Materialized>, mut commands: Commands) {
    if let Err(error) = &request.result {
        commands.trigger(WorkspaceRestoreFailed {
            store: request.target.entity(),
            path: request.path.clone(),
            error: error.clone(),
        });
    }
}

impl DecodeSnapshot {
    fn snapshot(
        &mut self,
        registry: &bevy::reflect::TypeRegistry,
        assets: &mut AssetServer,
    ) -> Result<bevy_world_serialization::DynamicWorld, String> {
        let mut bytes = Vec::new();
        match self.load.input.consume() {
            Some(LoadInput::World(snapshot)) => {
                return super::restored::Snapshot(snapshot).recover();
            }
            Some(LoadInput::File(path)) => {
                bytes = std::fs::read(path).map_err(|error| error.to_string())?;
            }
            Some(LoadInput::Stream(mut stream)) => {
                stream
                    .read_to_end(&mut bytes)
                    .map_err(|error| error.to_string())?;
            }
            _ => return Err("workspace load input is unavailable".to_string()),
        }
        let mut deserializer =
            ron::Deserializer::from_bytes(&bytes).map_err(|error| error.to_string())?;
        let snapshot = bevy_world_serialization::serde::WorldDeserializer {
            type_registry: registry,
            load_from_path: assets,
        }
        .deserialize(&mut deserializer)
        .map_err(|error| error.to_string())?;
        super::restored::Snapshot(snapshot).recover()
    }
}

#[derive(EntityEvent)]
struct WorkspaceRestoreFailed {
    #[event_target]
    store: Entity,
    path: PathBuf,
    error: String,
}

impl Command for WorkspaceRestoreFailed {
    type Out = ();

    fn apply(self, world: &mut World) {
        world.trigger(self);
    }
}

fn restore_failed(
    request: On<WorkspaceRestoreFailed>,
    mut stores: Query<
        (Option<&mut WorkspaceRestore>, Option<&mut State>),
        With<AutoSave<WorkspaceStore>>,
    >,
    mut exits: MessageWriter<AppExit>,
    mut commands: Commands,
) {
    warn!(
        "Workspace restore failed for {:?}: {}",
        request.path, request.error
    );
    let failure = RestoreFailure::preserve(&request.path);
    match &failure.backup {
        Some(backup) => {
            commands.queue(Warning {
                message: "workspace-store-recovered",
                path: backup.clone(),
            });
        }
        None => {
            commands.insert_resource(WorkspaceLoadRefused);
            exits.write(AppExit::error());
            commands.queue(Warning {
                message: "workspace-store-refused",
                path: request.path.clone(),
            });
        }
    }
    let Ok((restore, state)) = stores.get_mut(request.store) else {
        return;
    };
    if let Some(mut restore) = restore {
        restore.store_present = false;
        restore.complete = true;
    }
    if let Some(mut state) = state {
        if failure.backup.is_some() {
            state.ready();
        } else {
            state.fail(request.error.clone());
        }
    }
    commands.entity(request.store).insert(failure);
}

#[derive(Component)]
struct RestoreFailure {
    backup: Option<PathBuf>,
}

impl RestoreFailure {
    fn preserve(path: &Path) -> Self {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let backup = path.with_extension(format!("ron.restore-failed-{stamp}"));
        match Self::copy(path, &backup) {
            Ok(()) => {
                warn!("Preserved failed workspace restore at {backup:?}");
                Self {
                    backup: Some(backup),
                }
            }
            Err(error) => {
                warn!("Unable to back up failed workspace restore from {path:?}: {error}");
                Self { backup: None }
            }
        }
    }

    fn copy(path: &Path, backup: &Path) -> std::io::Result<()> {
        let body = std::fs::read(path)?;
        vmux_path::AtomicFile::write(backup, &body)?;
        let version = Self::store_version(path);
        match std::fs::read(version) {
            Ok(version) => vmux_path::AtomicFile::write(Self::version(backup), &version),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        }
    }

    fn version(backup: &Path) -> PathBuf {
        let mut path = backup.as_os_str().to_os_string();
        path.push(".version");
        PathBuf::from(path)
    }

    fn store_version(path: &Path) -> PathBuf {
        path.parent()
            .map(|parent| parent.join("store.version"))
            .unwrap_or_else(|| PathBuf::from("store.version"))
    }

    fn schema_version(path: &Path) -> Option<u32> {
        std::fs::read_to_string(Self::store_version(path))
            .ok()
            .and_then(|value| value.trim().parse::<u32>().ok())
    }

    fn backup_for_reset(path: &Path) -> std::io::Result<PathBuf> {
        for sequence in 0.. {
            let backup = path.with_extension(format!("ron.pre-v{STORE_SCHEMA_VERSION}.{sequence}"));
            let mut destination = match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&backup)
            {
                Ok(file) => file,
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            };
            let mut source = std::fs::File::open(path)?;
            std::io::copy(&mut source, &mut destination)?;
            destination.sync_all()?;
            match std::fs::File::open(Self::store_version(path)) {
                Ok(mut version) => {
                    let mut name = backup.as_os_str().to_owned();
                    name.push(".version");
                    let mut destination = std::fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(PathBuf::from(name))?;
                    std::io::copy(&mut version, &mut destination)?;
                    destination.sync_all()?;
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
            #[cfg(unix)]
            if let Some(parent) = backup.parent() {
                std::fs::File::open(parent)?.sync_all()?;
            }
            std::fs::remove_file(path)?;
            return Ok(backup);
        }
        unreachable!()
    }
}

struct WorkspaceStoreSource<'a>(&'a str);

impl WorkspaceStoreSource<'_> {
    fn normalize_if_needed(path: &Path) {
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

    #[derive(Component, Clone, Debug, Default, Reflect)]
    #[reflect(Component)]
    #[require(Save)]
    struct CommitValue {
        text: String,
    }

    #[derive(Component, Clone, Reflect)]
    #[reflect(opaque)]
    #[reflect(Component, Serialize, Deserialize)]
    #[require(Save)]
    struct CommitGateValue(std::sync::Arc<SerializationGate>);

    impl serde::Serialize for CommitGateValue {
        fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            if !self.0.enter() {
                return Err(serde::ser::Error::custom(
                    "Workspace serialization did not drain",
                ));
            }
            serializer.serialize_u32(1)
        }
    }

    impl<'de> serde::Deserialize<'de> for CommitGateValue {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            let _ = <u32 as serde::Deserialize>::deserialize(deserializer)?;
            Ok(Self(std::sync::Arc::new(SerializationGate::default())))
        }
    }

    #[derive(Resource, Default)]
    struct CommitReplies(Vec<WorkspaceCommitCompleted>);

    impl CommitReplies {
        fn record(
            mut requests: MessageReader<WorkspaceCommitCompleted>,
            mut replies: ResMut<Self>,
        ) {
            for request in requests.read() {
                replies.0.push(request.clone());
            }
        }
    }

    struct CommitFixture {
        app: App,
        clock: Clock,
        profile: Entity,
        store: Entity,
        requester: Entity,
        value: Entity,
        path: PathBuf,
        _directory: tempfile::TempDir,
    }

    impl CommitFixture {
        fn new() -> Self {
            let directory = tempfile::tempdir().unwrap();
            let profile = vmux_ecs::profile::Profile::named("workspace-commit-fixture");
            let paths = ProfilePaths::at(profile.clone(), directory.path().to_path_buf());
            let clock = Clock::fake(1_700_000_000_000);
            let mut app = App::new();
            app.insert_resource(clock.clone())
                .add_plugins((
                    MinimalPlugins,
                    AssetPlugin::default(),
                    vmux_ecs::PrimitivesPlugin,
                ))
                .register_stored::<Space>()
                .register_stored::<CommitValue>()
                .register_stored::<CommitGateValue>()
                .init_resource::<CommitReplies>()
                .add_systems(Last, CommitReplies::record)
                .add_plugins(WorkspaceStorePlugin);
            let profile = app
                .world_mut()
                .spawn((
                    ActiveProfile(profile),
                    Name::new("Workspace commit fixture profile"),
                    vmux_ecs::profile::ProfileDirectories(paths),
                ))
                .id();
            let requester = app
                .world_mut()
                .spawn(
                    vmux_ecs::Operation::bundle("fixture.commit")
                        .unwrap()
                        .running(),
                )
                .id();
            app.world_mut().spawn((Save, Space, Name::new("Workspace")));
            let value = app
                .world_mut()
                .spawn(CommitValue {
                    text: "before".into(),
                })
                .id();
            app.update();
            let store = app
                .world_mut()
                .query_filtered::<Entity, With<AutoSave<WorkspaceStore>>>()
                .single(app.world())
                .unwrap();
            let path = app.world().get::<StorePath>(store).unwrap().0.clone();
            assert!(path.starts_with(directory.path()));
            assert!(!path.exists());
            Self {
                app,
                clock,
                profile,
                store,
                requester,
                value,
                path,
                _directory: directory,
            }
        }

        fn request(&mut self) -> WorkspaceCommitRequest {
            let request = WorkspaceCommitRequest {
                request_id: vmux_ecs::Uuid(uuid::Uuid::new_v4().to_string()),
                requester: self.requester,
                profile: self.profile,
                path: self.path.clone(),
                required: self.value,
            };
            self.app.world_mut().write_message(request.clone());
            request
        }

        fn reply(&self, request: &WorkspaceCommitRequest) -> Option<&WorkspaceCommitCompleted> {
            self.app
                .world()
                .resource::<CommitReplies>()
                .0
                .iter()
                .find(|reply| reply.request_id == request.request_id)
        }

        fn finish(&mut self, request: &WorkspaceCommitRequest) {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while self.reply(request).is_none() {
                assert!(
                    std::time::Instant::now() < deadline,
                    "Workspace commit did not settle"
                );
                self.app.update();
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        }

        fn restored(&self) -> String {
            let registry = self.app.world().resource::<AppTypeRegistry>().clone();
            let registry = registry.read();
            let mut assets = self.app.world().resource::<AssetServer>().clone();
            let bytes = std::fs::read(&self.path).unwrap();
            let mut deserializer = ron::Deserializer::from_bytes(&bytes).unwrap();
            let saved = bevy_world_serialization::serde::WorldDeserializer {
                type_registry: &registry,
                load_from_path: &mut assets,
            }
            .deserialize(&mut deserializer)
            .unwrap();
            let mut world = World::new();
            saved
                .write_to_world_with(&mut world, &mut Default::default(), &registry)
                .unwrap();
            world
                .query::<&CommitValue>()
                .single(&world)
                .unwrap()
                .text
                .clone()
        }

        fn block(&mut self) -> std::sync::Arc<SerializationGate> {
            let gate = std::sync::Arc::new(SerializationGate::default());
            self.app.world_mut().spawn(CommitGateValue(gate.clone()));
            gate
        }

        fn advance_time(&mut self) {
            self.clock.advance(std::time::Duration::from_secs(61));
        }
    }

    #[test]
    fn workspace_commit_correlates_native_success_and_restored_data() {
        let mut fixture = CommitFixture::new();
        let request = fixture.request();
        fixture.finish(&request);
        let reply = fixture.reply(&request).unwrap();
        assert_eq!(
            (reply.requester, reply.profile, reply.store, &reply.path),
            (
                fixture.requester,
                fixture.profile,
                fixture.store,
                &fixture.path
            )
        );
        assert!(reply.result.is_ok());
        assert!(reply.revision > 0);
        assert_eq!(fixture.restored(), "before");
        fixture.app.world_mut().write_message(request.clone());
        fixture.app.update();
        assert_eq!(fixture.app.world().resource::<CommitReplies>().0.len(), 1);
    }

    #[test]
    fn workspace_commit_coalesces_independent_waiters_into_one_snapshot() {
        let mut fixture = CommitFixture::new();
        let first = fixture.request();
        let second = fixture.request();
        fixture.finish(&second);
        assert!(fixture.reply(&first).unwrap().result.is_ok());
        assert_eq!(
            fixture.reply(&first).unwrap().revision,
            fixture.reply(&second).unwrap().revision
        );
        assert_eq!(fixture.restored(), "before");
    }

    #[test]
    fn workspace_commit_refuses_required_data_excluded_from_the_actual_capture() {
        let mut fixture = CommitFixture::new();
        fixture
            .app
            .world_mut()
            .spawn((Window::default(), PrimaryWindow));
        let secondary = fixture.app.world_mut().spawn(Window::default()).id();
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.value)
            .insert(HostWindow(secondary));
        let request = fixture.request();
        fixture.finish(&request);
        assert!(fixture.reply(&request).unwrap().result.is_err());
        assert!(!fixture.path.exists());
        assert!(
            fixture
                .app
                .world()
                .get::<PendingWrite>(fixture.store)
                .is_none()
        );
    }

    #[test]
    fn workspace_commit_refuses_missing_or_omitted_required_data_without_io() {
        for kind in ["missing", "not-stored", "omitted", "unregistered"] {
            let mut fixture = CommitFixture::new();
            match kind {
                "missing" => {
                    fixture.app.world_mut().despawn(fixture.value);
                }
                "not-stored" => {
                    fixture
                        .app
                        .world_mut()
                        .entity_mut(fixture.value)
                        .remove::<Save>();
                }
                "omitted" => {
                    let mut filter = fixture
                        .app
                        .world_mut()
                        .get_mut::<SnapshotFilter>(fixture.store)
                        .unwrap();
                    filter.components = filter.components.clone().deny::<CommitValue>();
                }
                "unregistered" => {
                    fixture.value = fixture
                        .app
                        .world_mut()
                        .spawn((Save, Name::new("No stored data")))
                        .id();
                }
                _ => unreachable!(),
            }
            let request = fixture.request();
            fixture.finish(&request);
            assert!(fixture.reply(&request).unwrap().result.is_err(), "{kind}");
            assert!(!fixture.path.exists(), "{kind}");
            assert!(
                fixture
                    .app
                    .world()
                    .get::<PendingWrite>(fixture.store)
                    .is_none()
            );
        }
    }

    #[test]
    fn workspace_commit_rejects_wrong_source_and_cancellation_before_submit() {
        for kind in [
            "profile",
            "path",
            "requester",
            "canceled",
            "restore",
            "incomplete",
            "refused",
        ] {
            let mut fixture = CommitFixture::new();
            let mut request = WorkspaceCommitRequest {
                request_id: vmux_ecs::Uuid(uuid::Uuid::new_v4().to_string()),
                requester: fixture.requester,
                profile: fixture.profile,
                path: fixture.path.clone(),
                required: fixture.value,
            };
            match kind {
                "profile" => {
                    request.profile = fixture.app.world_mut().spawn_empty().id();
                }
                "path" => {
                    request.path = fixture.path.with_extension("other");
                }
                "requester" => {
                    fixture.app.world_mut().despawn(fixture.requester);
                }
                "canceled" => {
                    fixture
                        .app
                        .world_mut()
                        .entity_mut(fixture.requester)
                        .insert(vmux_ecs::OperationState::Canceled);
                }
                "restore" => {
                    fixture
                        .app
                        .world_mut()
                        .get_mut::<State>(fixture.store)
                        .unwrap()
                        .resolve();
                }
                "incomplete" => {
                    fixture
                        .app
                        .world_mut()
                        .get_mut::<WorkspaceRestore>(fixture.store)
                        .unwrap()
                        .complete = false;
                }
                "refused" => {
                    fixture.app.insert_resource(WorkspaceLoadRefused);
                }
                _ => unreachable!(),
            }
            fixture.app.world_mut().write_message(request.clone());
            fixture.finish(&request);
            assert!(fixture.reply(&request).unwrap().result.is_err(), "{kind}");
            assert!(!fixture.path.exists(), "{kind}");
        }
    }

    #[test]
    fn workspace_commit_an_older_native_receipt_cannot_complete_a_newer_request() {
        let mut fixture = CommitFixture::new();
        let gate = fixture.block();
        let first = fixture.request();
        fixture.app.update();
        gate.wait_started();
        fixture
            .app
            .world_mut()
            .get_mut::<CommitValue>(fixture.value)
            .unwrap()
            .text = "after".into();
        let second = fixture.request();
        fixture.app.update();
        assert!(fixture.reply(&second).is_none());
        gate.release();
        fixture.finish(&second);
        assert!(fixture.reply(&first).unwrap().revision < fixture.reply(&second).unwrap().revision);
        assert_eq!(fixture.restored(), "after");
    }

    #[test]
    fn workspace_commit_requester_retirement_waits_for_submitted_io() {
        let mut fixture = CommitFixture::new();
        let gate = fixture.block();
        let request = fixture.request();
        fixture.app.update();
        gate.wait_started();
        fixture.app.world_mut().despawn(fixture.requester);
        fixture.app.update();
        assert!(fixture.reply(&request).is_none());
        assert!(
            fixture
                .app
                .world()
                .get::<PendingWrite>(fixture.store)
                .is_some()
        );
        gate.release();
        fixture.finish(&request);
        assert!(fixture.reply(&request).unwrap().result.is_err());
        assert_eq!(fixture.restored(), "before");
    }

    #[test]
    fn workspace_commit_in_place_profile_replacement_refuses_stale_delivery_after_drain() {
        let mut fixture = CommitFixture::new();
        let gate = fixture.block();
        let request = fixture.request();
        fixture.app.update();
        gate.wait_started();
        let replacement = ProfilePaths::at(
            vmux_ecs::profile::Profile::named("replacement"),
            fixture._directory.path().join("replacement"),
        );
        let replacement_path = replacement.store().join("store.ron");
        fixture.app.world_mut().entity_mut(fixture.profile).insert((
            ActiveProfile(vmux_ecs::profile::Profile::named("replacement")),
            vmux_ecs::profile::ProfileDirectories(replacement),
        ));
        fixture.app.update();
        assert!(fixture.reply(&request).is_none());
        gate.release();
        fixture.finish(&request);
        assert!(fixture.reply(&request).unwrap().result.is_err());
        assert_eq!(fixture.restored(), "before");
        assert!(!replacement_path.exists());
    }

    #[test]
    fn workspace_commit_store_retirement_keeps_orphan_custody_and_original_path() {
        let mut fixture = CommitFixture::new();
        let gate = fixture.block();
        let request = fixture.request();
        fixture.app.update();
        gate.wait_started();
        fixture.app.world_mut().despawn(fixture.store);
        fixture.app.update();
        assert!(fixture.reply(&request).is_none());
        assert_eq!(
            fixture
                .app
                .world_mut()
                .query_filtered::<Entity, With<OrphanWrite>>()
                .iter(fixture.app.world())
                .count(),
            1
        );
        gate.release();
        fixture.finish(&request);
        assert!(fixture.reply(&request).unwrap().result.is_err());
        assert_eq!(fixture.reply(&request).unwrap().path, fixture.path);
        assert_eq!(fixture.restored(), "before");
        assert_eq!(
            fixture
                .app
                .world_mut()
                .query_filtered::<Entity, With<OrphanWrite>>()
                .iter(fixture.app.world())
                .count(),
            0
        );
    }

    #[test]
    fn workspace_commit_partial_schema_failure_retries_the_same_identity() {
        let mut fixture = CommitFixture::new();
        let version = RestoreFailure::store_version(&fixture.path);
        std::fs::create_dir_all(&version).unwrap();
        let request = fixture.request();
        fixture.app.update();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while fixture
            .app
            .world()
            .get::<State>(fixture.store)
            .unwrap()
            .error()
            .is_none()
        {
            assert!(std::time::Instant::now() < deadline);
            fixture.app.update();
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert!(fixture.path.is_file());
        assert!(fixture.reply(&request).is_none());
        std::fs::remove_dir(version).unwrap();
        fixture.advance_time();
        fixture.finish(&request);
        assert!(fixture.reply(&request).unwrap().result.is_ok());
        assert_eq!(fixture.app.world().resource::<CommitReplies>().0.len(), 1);
        assert_eq!(fixture.restored(), "before");
    }

    #[test]
    fn workspace_commit_a_receipt_from_a_failed_capture_cannot_complete_its_retry() {
        let mut fixture = CommitFixture::new();
        let version = RestoreFailure::store_version(&fixture.path);
        std::fs::create_dir_all(&version).unwrap();
        let request = fixture.request();
        fixture.app.update();
        let pending = fixture
            .app
            .world()
            .get::<PendingWrite>(fixture.store)
            .unwrap();
        let capture = pending.capture.clone();
        let revision = pending.revision;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while fixture
            .app
            .world()
            .get::<State>(fixture.store)
            .unwrap()
            .error()
            .is_none()
        {
            assert!(std::time::Instant::now() < deadline);
            fixture.app.update();
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        std::fs::remove_dir(version).unwrap();
        fixture.advance_time();
        fixture.app.update();
        fixture.app.world_mut().trigger(WriteCompleted {
            capture,
            target: EntityTarget::new(fixture.store),
            revision,
            path: fixture.path.clone(),
            result: Ok(()),
        });
        assert!(fixture.reply(&request).is_none());
        fixture.finish(&request);
        assert!(fixture.reply(&request).unwrap().result.is_ok());
        assert_eq!(fixture.restored(), "before");
    }

    #[test]
    fn explicit_workspace_saves_wait_for_runtime_restore_commit() {
        if HomeEnvGuard::isolated() {
            return;
        }
        let mut fixture = WorkspaceSaveFixture::new("explicit-waits-for-restore");
        fixture
            .app
            .world_mut()
            .get_mut::<State>(fixture.store)
            .unwrap()
            .resolve();
        fixture.app.world_mut().write_message(WorkspaceSaveRequest {
            path: fixture.path.clone(),
        });
        fixture.app.update();
        assert!(!fixture.path.exists());
        assert!(
            fixture
                .app
                .world()
                .get::<PendingWrite>(fixture.store)
                .is_none()
        );
        fixture
            .app
            .world_mut()
            .get_mut::<State>(fixture.store)
            .unwrap()
            .ready();
        fixture.app.world_mut().write_message(WorkspaceSaveRequest {
            path: fixture.path.clone(),
        });
        fixture.app.update();
        assert!(fixture.path.is_file());
    }
    #[derive(Resource, Default)]
    struct SaveCount(u32);

    impl SaveCount {
        fn record(_: On<Written>, mut count: ResMut<Self>) {
            count.0 += 1;
        }
    }

    struct WorkspaceSaveFixture {
        app: App,
        clock: Clock,
        store: Entity,
        path: PathBuf,
        _home: HomeEnvGuard,
    }

    struct WorkspaceRestoreFixture {
        app: App,
        store: Entity,
        path: PathBuf,
        _home: HomeEnvGuard,
    }

    impl WorkspaceRestoreFixture {
        fn live_entities(&self) -> std::collections::HashSet<Entity> {
            let mut entities = std::collections::HashSet::new();
            for archetype in self.app.world().archetypes().iter() {
                for entity in archetype.entities() {
                    entities.insert(entity.id());
                }
            }
            entities
        }

        fn new(name: &str) -> Self {
            let home = HomeEnvGuard::use_temp_home(name);
            let mut app = App::new();
            app.add_plugins((
                MinimalPlugins,
                AssetPlugin::default(),
                (vmux_ecs::PrimitivesPlugin, SchemaPlugin),
            ))
            .register_stored::<SpaceId>()
            .register_stored::<PaneId>()
            .register_stored::<vmux_session::SessionId>()
            .add_plugins(WorkspaceStorePlugin);
            app.world_mut()
                .spawn(WorkspaceRestoreValidator::unique::<SpaceId>("Space ID"));
            app.world_mut()
                .spawn(WorkspaceRestoreValidator::unique::<PaneId>("Pane ID"));
            app.world_mut()
                .spawn(WorkspaceRestoreValidator::unique::<vmux_session::SessionId>("Session ID"));
            app.update();
            let store = app
                .world_mut()
                .query_filtered::<Entity, With<AutoSave<WorkspaceStore>>>()
                .single(app.world())
                .unwrap();
            let path = app.world().get::<StorePath>(store).unwrap().0.clone();
            Self {
                app,
                store,
                path,
                _home: home,
            }
        }

        fn load(&mut self, snapshot: bevy_world_serialization::DynamicWorld) -> String {
            let body = snapshot
                .serialize(&self.app.world().resource::<AppTypeRegistry>().read())
                .unwrap();
            vmux_path::AtomicFile::write(&self.path, body.as_bytes()).unwrap();
            SnapshotWriter::write_schema_version(&self.path).unwrap();
            self.app.world_mut().commands().queue(WorkspaceLoad {
                load: Some(LoadWorld::default_from_file(self.path.clone())),
                store: self.store,
                path: self.path.clone(),
            });
            self.app.update();
            body
        }

        fn preserved(&self) -> PathBuf {
            self.app
                .world()
                .get::<RestoreFailure>(self.store)
                .unwrap()
                .backup
                .clone()
                .unwrap()
        }
    }

    #[test]
    fn duplicate_restored_ids_are_rejected_before_live_state_is_replaced() {
        if HomeEnvGuard::isolated() {
            return;
        }
        for kind in ["space", "pane", "session"] {
            let mut fixture = WorkspaceRestoreFixture::new("duplicate-restored-id");
            let live = fixture.app.world_mut().spawn(SpaceId("shared".into())).id();
            let mut entities = Vec::new();
            for entity in [Entity::from_bits(1), Entity::from_bits(2)] {
                let component: Box<dyn bevy::reflect::PartialReflect> = match kind {
                    "space" => Box::new(SpaceId("shared".into())),
                    "pane" => Box::new(PaneId("shared".into())),
                    _ => Box::new(vmux_session::SessionId::new("shared")),
                };
                entities.push(bevy_world_serialization::DynamicEntity {
                    entity,
                    components: vec![component],
                });
            }
            let body = fixture.load(bevy_world_serialization::DynamicWorld {
                resources: Vec::new(),
                entities,
            });
            assert!(fixture.app.world().get::<SpaceId>(live).is_some());
            assert!(
                fixture
                    .app
                    .world()
                    .resource::<Messages<LayoutRestoreRequest>>()
                    .is_empty()
            );
            assert_eq!(std::fs::read_to_string(&fixture.path).unwrap(), body);
            assert_eq!(std::fs::read_to_string(fixture.preserved()).unwrap(), body);
            assert_eq!(
                std::fs::read_to_string(RestoreFailure::version(&fixture.preserved())).unwrap(),
                std::fs::read_to_string(RestoreFailure::store_version(&fixture.path)).unwrap()
            );
        }
    }

    #[test]
    fn validated_ids_are_scoped_to_the_snapshot_and_then_request_layout_restore() {
        if HomeEnvGuard::isolated() {
            return;
        }
        let mut fixture = WorkspaceRestoreFixture::new("valid-restored-id");
        fixture.app.world_mut().spawn(SpaceId("shared".into()));
        fixture.load(bevy_world_serialization::DynamicWorld {
            resources: Vec::new(),
            entities: vec![bevy_world_serialization::DynamicEntity {
                entity: Entity::from_bits(1),
                components: vec![Box::new(SpaceId("shared".into()))],
            }],
        });
        assert!(
            fixture
                .app
                .world()
                .get::<RestoreFailure>(fixture.store)
                .is_none()
        );
        assert_eq!(
            fixture
                .app
                .world_mut()
                .query::<&SpaceId>()
                .iter(fixture.app.world())
                .count(),
            1
        );
        assert_eq!(
            fixture
                .app
                .world()
                .resource::<Messages<LayoutRestoreRequest>>()
                .len(),
            1
        );
    }

    #[derive(Reflect)]
    struct Unspawnable;

    #[test]
    fn failed_materialization_removes_partial_entities_and_preserves_the_store() {
        if HomeEnvGuard::isolated() {
            return;
        }
        let mut fixture = WorkspaceRestoreFixture::new("failed-materialization");
        fixture.app.register_type::<Unspawnable>();
        let live = fixture
            .app
            .world_mut()
            .spawn(Name::new("live runtime"))
            .id();
        let baseline = fixture.live_entities();
        let body = fixture.load(bevy_world_serialization::DynamicWorld {
            resources: Vec::new(),
            entities: vec![
                bevy_world_serialization::DynamicEntity {
                    entity: Entity::from_bits(1),
                    components: vec![Box::new(Unspawnable)],
                },
                bevy_world_serialization::DynamicEntity {
                    entity: Entity::from_bits(2),
                    components: Vec::new(),
                },
            ],
        });
        assert_eq!(fixture.live_entities(), baseline);
        assert!(fixture.app.world().get::<Name>(live).is_some());
        assert!(
            fixture
                .app
                .world()
                .resource::<Messages<LayoutRestoreRequest>>()
                .is_empty()
        );
        assert_eq!(std::fs::read_to_string(fixture.preserved()).unwrap(), body);
        let restore = fixture
            .app
            .world()
            .get::<WorkspaceRestore>(fixture.store)
            .unwrap();
        assert!(restore.complete);
        assert!(!restore.store_present);
    }

    impl WorkspaceSaveFixture {
        fn finish_write(&mut self) {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
            while self.app.world().get::<PendingWrite>(self.store).is_some() {
                assert!(
                    std::time::Instant::now() < deadline,
                    "workspace writer did not finish"
                );
                std::thread::sleep(std::time::Duration::from_millis(1));
                self.app.update();
            }
        }

        fn new(name: &str) -> Self {
            let home = HomeEnvGuard::use_temp_home(name);
            let clock = Clock::fake(1_700_000_000_000);
            let mut app = App::new();
            app.insert_resource(clock.clone())
                .add_plugins((MinimalPlugins, (vmux_ecs::PrimitivesPlugin, SchemaPlugin)))
                .register_stored::<Space>()
                .insert_resource(test_settings())
                .init_resource::<SaveCount>()
                .add_observer(SaveCount::record)
                .add_plugins(WorkspaceStorePlugin);
            app.world_mut().spawn((Save, Space, Name::new("Workspace")));
            app.update();
            let store = app
                .world_mut()
                .query_filtered::<Entity, With<AutoSave<WorkspaceStore>>>()
                .single(app.world())
                .unwrap();
            let path = app.world().get::<StorePath>(store).unwrap().0.clone();
            assert!(!path.exists());
            Self {
                app,
                clock,
                store,
                path,
                _home: home,
            }
        }
    }

    #[test]
    fn exit_flushes_pending_workspace_changes_before_debounce() {
        if HomeEnvGuard::isolated() {
            return;
        }
        let mut fixture = WorkspaceSaveFixture::new("exit-flushes-pending-layout");
        fixture
            .app
            .insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
                std::time::Duration::ZERO,
            ));
        fixture.app.world_mut().trigger(StoreDirty);
        fixture.app.world_mut().write_message(AppExit::Success);
        fixture.app.update();
        assert!(fixture.path.exists());
        assert_eq!(fixture.app.world().resource::<SaveCount>().0, 1);
        assert!(
            std::fs::read_to_string(&fixture.path)
                .unwrap()
                .contains("Workspace")
        );
    }

    #[test]
    fn workspace_claims_stack_pages_without_claiming_another_roots_shared_data() {
        if HomeEnvGuard::isolated() {
            return;
        }
        let mut fixture = WorkspaceSaveFixture::new("scoped-workspace-pages");
        let stack = fixture
            .app
            .world_mut()
            .spawn(vmux_layout::stack::Stack::default())
            .id();
        let page = fixture
            .app
            .world_mut()
            .spawn((
                PageMetadata {
                    url: "file:///workspace".into(),
                    ..Default::default()
                },
                vmux_ecs::Cwd("/workspace".into()),
                ChildOf(stack),
            ))
            .id();
        let restored_page = fixture
            .app
            .world_mut()
            .spawn((
                Save,
                PageMetadata {
                    url: "file:///restored".into(),
                    ..Default::default()
                },
                ChildOf(stack),
            ))
            .id();
        let other_root = fixture
            .app
            .world_mut()
            .spawn(Name::new("Project store"))
            .id();
        let project = fixture
            .app
            .world_mut()
            .spawn((
                Name::new("Independent project"),
                vmux_ecs::Cwd("/project".into()),
                CreatedAt(1),
                vmux_ecs::Order(0),
                ChildOf(other_root),
            ))
            .id();
        assert!(fixture.app.world().get::<Save>(page).is_none());
        assert!(fixture.app.world().get::<Save>(project).is_none());
        fixture.app.update();
        assert!(fixture.app.world().get::<Save>(page).is_some());
        assert!(fixture.app.world().get::<Save>(project).is_none());
        fixture.finish_write();
        fixture
            .app
            .world_mut()
            .entity_mut(page)
            .insert(ChildOf(other_root));
        fixture
            .app
            .world_mut()
            .entity_mut(restored_page)
            .insert(ChildOf(other_root));
        fixture.app.update();
        assert!(fixture.app.world().get::<Save>(page).is_none());
        assert!(fixture.app.world().get::<Save>(restored_page).is_none());
        fixture.finish_write();
    }

    #[test]
    fn exit_waits_for_pending_write_and_saves_the_latest_workspace() {
        if HomeEnvGuard::isolated() {
            return;
        }
        let mut fixture = WorkspaceSaveFixture::new("exit-drains-pending-layout");
        let (sender, result) = mpsc::channel();
        let pending = fixture.path.clone();
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.store)
            .insert(PendingWrite {
                capture: vmux_ecs::Uuid(uuid::Uuid::new_v4().to_string()),
                revision: 1,
                path: pending.clone(),
                result: Some(Mutex::new(result)),
            });
        let writer = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(20));
            std::fs::write(pending, b"older snapshot").unwrap();
            sender.send(Ok(())).unwrap();
        });
        fixture
            .app
            .insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
                std::time::Duration::ZERO,
            ));
        fixture.app.world_mut().write_message(AppExit::Success);
        fixture.app.update();
        writer.join().unwrap();
        assert!(
            std::fs::read_to_string(&fixture.path)
                .unwrap()
                .contains("Workspace")
        );
        assert!(
            fixture
                .app
                .world()
                .get::<PendingWrite>(fixture.store)
                .is_none()
        );
    }

    #[test]
    fn an_explicit_save_drains_detached_io_and_retires_the_operation() {
        if HomeEnvGuard::isolated() {
            return;
        }
        let mut fixture = WorkspaceSaveFixture::new("explicit-drains-detached-write");
        let revision = {
            let mut state = fixture
                .app
                .world_mut()
                .get_mut::<State>(fixture.store)
                .unwrap();
            state.advance();
            state.begin().unwrap()
        };
        let (sender, result) = mpsc::channel();
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.store)
            .insert(PendingWrite {
                capture: vmux_ecs::Uuid(uuid::Uuid::new_v4().to_string()),
                revision,
                path: fixture.path.clone(),
                result: Some(Mutex::new(result)),
            });
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.store)
            .remove::<PendingWrite>();
        fixture.app.world_mut().flush();
        assert!(sender.send(Ok(())).is_ok());
        fixture.app.world_mut().write_message(WorkspaceSaveRequest {
            path: fixture.path.clone(),
        });
        fixture.app.update();
        assert!(
            std::fs::read_to_string(&fixture.path)
                .unwrap()
                .contains("Workspace")
        );
        assert!(
            fixture
                .app
                .world_mut()
                .query_filtered::<Entity, With<vmux_ecs::Operation>>()
                .iter(fixture.app.world())
                .next()
                .is_none()
        );
        assert!(
            fixture
                .app
                .world()
                .get::<State>(fixture.store)
                .unwrap()
                .saving()
                .is_none()
        );
    }

    #[test]
    fn exit_drains_an_attached_writer_after_the_last_space_is_removed() {
        if HomeEnvGuard::isolated() {
            return;
        }
        let mut fixture = WorkspaceSaveFixture::new("exit-without-space");
        let (sender, result) = mpsc::channel();
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.store)
            .insert(PendingWrite {
                capture: vmux_ecs::Uuid(uuid::Uuid::new_v4().to_string()),
                revision: 1,
                path: fixture.path.clone(),
                result: Some(Mutex::new(result)),
            });
        let spaces: Vec<_> = fixture
            .app
            .world_mut()
            .query_filtered::<Entity, With<Space>>()
            .iter(fixture.app.world())
            .collect();
        for entity in spaces {
            fixture.app.world_mut().despawn(entity);
        }
        let path = fixture.path.clone();
        let writer = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(20));
            std::fs::write(path, "accepted snapshot").unwrap();
            sender.send(Ok(())).unwrap();
        });
        fixture.app.world_mut().write_message(AppExit::Success);
        fixture.app.update();
        assert_eq!(
            std::fs::read_to_string(&fixture.path).unwrap(),
            "accepted snapshot"
        );
        writer.join().unwrap();
    }

    #[test]
    fn clean_workspaces_do_not_write_periodic_snapshots() {
        if HomeEnvGuard::isolated() {
            return;
        }
        let mut fixture = WorkspaceSaveFixture::new("clean-periodic-autosave");
        fixture
            .app
            .world_mut()
            .get_mut::<State>(fixture.store)
            .unwrap()
            .clone_from(&State::default());

        fixture.clock.advance(std::time::Duration::from_secs(61));
        fixture.app.update();

        assert_eq!(fixture.app.world().resource::<SaveCount>().0, 0);
        assert!(!fixture.path.exists());
    }

    #[test]
    fn retiring_the_store_keeps_inflight_completion_connected_without_publishing_success() {
        if HomeEnvGuard::isolated() {
            return;
        }
        let mut fixture = WorkspaceSaveFixture::new("retired-write-owner");
        let revision = {
            let mut state = fixture
                .app
                .world_mut()
                .get_mut::<State>(fixture.store)
                .unwrap();
            state.advance();
            state.begin().unwrap()
        };
        let (sender, result) = mpsc::channel();
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.store)
            .insert(PendingWrite {
                capture: vmux_ecs::Uuid(uuid::Uuid::new_v4().to_string()),
                revision,
                path: fixture.path.clone(),
                result: Some(Mutex::new(result)),
            });
        fixture.app.world_mut().despawn(fixture.store);
        fixture.app.world_mut().flush();
        assert!(sender.send(Ok(())).is_ok());
        fixture.app.update();
        assert_eq!(fixture.app.world().resource::<SaveCount>().0, 0);
    }

    #[test]
    fn failed_restore_without_a_backup_cannot_be_overwritten_by_automatic_or_explicit_saves() {
        if HomeEnvGuard::isolated() {
            return;
        }
        let mut fixture = WorkspaceSaveFixture::new("unprotected-restore");
        vmux_path::AtomicFile::write(&fixture.path, b"original failed restore").unwrap();
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.store)
            .insert(RestoreFailure { backup: None });
        fixture.clock.advance(std::time::Duration::from_secs(61));
        fixture.app.update();
        assert!(
            fixture
                .app
                .world()
                .get::<PendingWrite>(fixture.store)
                .is_none()
        );
        fixture.app.world_mut().write_message(WorkspaceSaveRequest {
            path: fixture.path.clone(),
        });
        fixture.app.update();
        assert_eq!(
            std::fs::read_to_string(&fixture.path).unwrap(),
            "original failed restore"
        );
        assert_eq!(fixture.app.world().resource::<SaveCount>().0, 0);
    }

    #[test]
    fn debounce_and_periodic_deadlines_write_one_snapshot() {
        if HomeEnvGuard::isolated() {
            return;
        }
        let mut fixture = WorkspaceSaveFixture::new("coalesced-autosave");

        fixture.clock.advance(std::time::Duration::from_secs(61));
        fixture.app.update();
        fixture.finish_write();

        assert_eq!(fixture.app.world().resource::<SaveCount>().0, 1);
        assert!(fixture.path.exists());

        fixture.clock.advance(std::time::Duration::from_secs(61));
        fixture.app.update();

        assert_eq!(fixture.app.world().resource::<SaveCount>().0, 1);
    }

    #[test]
    fn failed_workspace_writes_retry_after_storage_recovers_without_another_edit() {
        if HomeEnvGuard::isolated() {
            return;
        }
        let mut fixture = WorkspaceSaveFixture::new("failed-autosave-retry");
        std::fs::create_dir_all(&fixture.path).unwrap();
        fixture.clock.advance(std::time::Duration::from_secs(61));
        fixture.app.update();
        fixture.finish_write();
        assert!(
            fixture
                .app
                .world()
                .get::<State>(fixture.store)
                .unwrap()
                .is_dirty()
        );
        assert_eq!(fixture.app.world().resource::<SaveCount>().0, 0);
        std::fs::remove_dir(&fixture.path).unwrap();

        fixture.clock.advance(std::time::Duration::from_secs(61));
        fixture.app.update();
        fixture.finish_write();

        assert!(fixture.path.is_file());
        assert!(
            !fixture
                .app
                .world()
                .get::<State>(fixture.store)
                .unwrap()
                .is_dirty()
        );
        assert_eq!(fixture.app.world().resource::<SaveCount>().0, 1);
    }

    #[derive(Component)]
    struct LiveChild;

    #[test]
    fn restored_timestamp_only_session_children_are_removed_without_losing_conversation_content() {
        if HomeEnvGuard::isolated() {
            return;
        }
        let mut fixture = WorkspaceSaveFixture::new("empty-session-children");
        std::fs::write(&fixture.path, "original workspace snapshot").unwrap();
        let session = fixture.app.world_mut().spawn(vmux_session::Session).id();
        let empty = fixture
            .app
            .world_mut()
            .spawn((CreatedAt(1), ChildOf(session)))
            .id();
        let live = fixture
            .app
            .world_mut()
            .spawn((CreatedAt(2), ChildOf(session), LiveChild))
            .id();
        let fresh = fixture
            .app
            .world_mut()
            .spawn((CreatedAt(3), ChildOf(session)))
            .id();
        let message = vmux_api::session::Message::user("Keep this conversation");
        let content = fixture
            .app
            .world_mut()
            .spawn((
                CreatedAt(4),
                ChildOf(session),
                vmux_session::ConversationEvent,
                vmux_session::MessageContent(message.clone()),
            ))
            .id();
        let other = fixture.app.world_mut().spawn_empty().id();
        let unrelated = fixture
            .app
            .world_mut()
            .spawn((CreatedAt(5), ChildOf(other)))
            .id();
        fixture.app.world_mut().trigger(Loaded {
            entity_map: [empty, live, content, unrelated]
                .into_iter()
                .map(|entity| (entity, entity))
                .collect(),
        });
        fixture.app.world_mut().flush();
        assert!(fixture.app.world().get_entity(empty).is_err());
        for retained in [live, fresh, content, unrelated] {
            assert!(fixture.app.world().get_entity(retained).is_ok());
        }
        assert_eq!(
            fixture
                .app
                .world()
                .get::<vmux_session::MessageContent>(content)
                .unwrap()
                .0,
            message
        );
        assert_eq!(
            fixture
                .app
                .world()
                .get::<Children>(session)
                .unwrap()
                .iter()
                .collect::<std::collections::HashSet<_>>(),
            [live, fresh, content]
                .into_iter()
                .collect::<std::collections::HashSet<_>>(),
        );
        let backup = fixture
            .path
            .with_extension("ron.before-empty-session-children");
        assert_eq!(
            std::fs::read_to_string(backup).unwrap(),
            "original workspace snapshot"
        );
    }

    #[derive(Default)]
    struct SerializationGate {
        state: Mutex<(bool, bool)>,
        changed: std::sync::Condvar,
    }

    impl SerializationGate {
        fn enter(&self) -> bool {
            let mut state = self.state.lock().unwrap();
            state.0 = true;
            self.changed.notify_all();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
            while !state.1 {
                let remaining = deadline.saturating_duration_since(std::time::Instant::now());
                if remaining.is_zero() {
                    return false;
                }
                state = self.changed.wait_timeout(state, remaining).unwrap().0;
            }
            true
        }

        fn wait_started(&self) {
            let state = self.state.lock().unwrap();
            let (state, _) = self
                .changed
                .wait_timeout_while(state, std::time::Duration::from_secs(3), |state| !state.0)
                .unwrap();
            assert!(state.0, "workspace serialization never started");
        }

        fn release(&self) {
            self.state.lock().unwrap().1 = true;
            self.changed.notify_all();
        }
    }

    #[derive(Component, Clone, Reflect)]
    #[reflect(opaque)]
    #[reflect(Component, Serialize)]
    #[require(Save)]
    struct BlockingValue(std::sync::Arc<SerializationGate>);

    impl serde::Serialize for BlockingValue {
        fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            if !self.0.enter() {
                return Err(serde::ser::Error::custom(
                    "serialization was blocked on the input thread",
                ));
            }
            serializer.serialize_u32(1)
        }
    }

    #[test]
    fn background_serialization_keeps_input_available_and_preserves_edits_made_during_a_write() {
        if HomeEnvGuard::isolated() {
            return;
        }
        let mut fixture = WorkspaceSaveFixture::new("background-save");
        std::fs::write(&fixture.path, "previous complete snapshot").unwrap();
        let gate = std::sync::Arc::new(SerializationGate::default());
        fixture.app.register_stored::<BlockingValue>();
        {
            let mut filter = fixture
                .app
                .world_mut()
                .get_mut::<SnapshotFilter>(fixture.store)
                .unwrap();
            filter.components = filter.components.clone().allow::<BlockingValue>();
        }
        fixture.app.world_mut().spawn(BlockingValue(gate.clone()));
        fixture.app.update();
        fixture.clock.advance(std::time::Duration::from_secs(61));
        fixture.app.update();
        assert!(
            fixture
                .app
                .world()
                .get::<PendingWrite>(fixture.store)
                .is_some()
        );
        gate.wait_started();
        let workspace = fixture
            .app
            .world_mut()
            .query_filtered::<Entity, With<Space>>()
            .single(fixture.app.world())
            .unwrap();
        fixture
            .app
            .world_mut()
            .entity_mut(workspace)
            .insert(Name::new("Changed while saving"));
        fixture.app.update();
        assert!(
            fixture
                .app
                .world()
                .get::<State>(fixture.store)
                .unwrap()
                .is_dirty()
        );
        assert_eq!(
            std::fs::read_to_string(&fixture.path).unwrap(),
            "previous complete snapshot"
        );
        gate.release();
        fixture.finish_write();
        assert!(
            fixture
                .app
                .world()
                .get::<State>(fixture.store)
                .unwrap()
                .is_dirty()
        );
        assert!(
            !std::fs::read_to_string(&fixture.path)
                .unwrap()
                .contains("Changed while saving")
        );
        fixture.clock.advance(std::time::Duration::from_secs(61));
        fixture.app.update();
        fixture.finish_write();
        assert!(
            std::fs::read_to_string(&fixture.path)
                .unwrap()
                .contains("Changed while saving")
        );
        assert!(
            !fixture
                .app
                .world()
                .get::<State>(fixture.store)
                .unwrap()
                .is_dirty()
        );
    }

    #[test]
    fn adding_archived_page_marks_store_dirty() {
        let mut app = App::new();
        app.add_plugins(AutoSavePlugin::<WorkspaceStore>::default())
            .register_stored::<ArchivedPage>();
        let auto_save = app
            .world_mut()
            .spawn(AutoSave::<WorkspaceStore>::default())
            .id();
        app.update();
        app.world_mut()
            .get_mut::<State>(auto_save)
            .unwrap()
            .clone_from(&State::default());
        app.world_mut().spawn((Save, ArchivedPage::default()));
        app.update();
        assert!(app.world().get::<State>(auto_save).unwrap().is_dirty());
    }

    #[test]
    fn unchanged_saved_names_do_not_restart_autosave() {
        if HomeEnvGuard::isolated() {
            return;
        }
        let _home = HomeEnvGuard::use_temp_home("saved-name-autosave");
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins((vmux_ecs::PrimitivesPlugin, SchemaPlugin))
            .insert_resource(test_settings())
            .add_plugins(WorkspaceStorePlugin);
        let entity = app.world_mut().spawn((Save, Name::new("unchanged"))).id();
        app.update();
        let store = app
            .world_mut()
            .query_filtered::<Entity, With<AutoSave<WorkspaceStore>>>()
            .single(app.world())
            .unwrap();
        app.world_mut()
            .get_mut::<State>(store)
            .unwrap()
            .clone_from(&State::default());
        app.world_mut()
            .entity_mut(entity)
            .insert(Name::new("unchanged"));
        app.update();
        assert!(!app.world().get::<State>(store).unwrap().is_dirty());
        app.world_mut()
            .entity_mut(entity)
            .insert(Name::new("renamed"));
        app.update();
        assert!(app.world().get::<State>(store).unwrap().is_dirty());
    }

    #[test]
    fn adding_visit_marks_store_dirty() {
        let mut app = App::new();
        app.add_plugins(AutoSavePlugin::<WorkspaceStore>::default())
            .register_stored::<vmux_history::Visit>();
        let auto_save = app
            .world_mut()
            .spawn(AutoSave::<WorkspaceStore>::default())
            .id();
        app.update();
        app.world_mut()
            .get_mut::<State>(auto_save)
            .unwrap()
            .clone_from(&State::default());
        app.world_mut().spawn((Save, vmux_history::Visit));
        app.update();
        assert!(app.world().get::<State>(auto_save).unwrap().is_dirty());
    }

    #[test]
    fn changing_tab_startup_dir_marks_store_dirty() {
        let mut app = App::new();
        app.add_plugins(AutoSavePlugin::<WorkspaceStore>::default())
            .register_stored::<Tab>();
        let auto_save = app
            .world_mut()
            .spawn(AutoSave::<WorkspaceStore>::default())
            .id();
        let tab = app.world_mut().spawn((Save, Tab::default())).id();
        app.update();
        app.world_mut()
            .get_mut::<State>(auto_save)
            .unwrap()
            .clone_from(&State::default());
        app.world_mut()
            .entity_mut(tab)
            .get_mut::<Tab>()
            .unwrap()
            .startup_dir = Some("/tmp/rebound".into());

        app.update();

        assert!(app.world().get::<State>(auto_save).unwrap().is_dirty());
    }

    #[test]
    fn adding_tab_workspace_marks_store_dirty() {
        let mut app = App::new();
        app.add_plugins(AutoSavePlugin::<WorkspaceStore>::default())
            .register_stored::<TabWorkspace>();
        let auto_save = app
            .world_mut()
            .spawn(AutoSave::<WorkspaceStore>::default())
            .id();
        let tab = app.world_mut().spawn((Save, Tab::default())).id();
        app.update();
        app.world_mut()
            .get_mut::<State>(auto_save)
            .unwrap()
            .clone_from(&State::default());
        app.world_mut().entity_mut(tab).insert(TabWorkspace {
            project_dir: "/tmp/project".into(),
        });

        app.update();

        assert!(app.world().get::<State>(auto_save).unwrap().is_dirty());
    }

    #[test]
    fn removing_tab_worktree_marks_store_dirty() {
        let mut app = App::new();
        app.add_plugins(AutoSavePlugin::<WorkspaceStore>::default())
            .register_stored::<Tab>()
            .register_stored::<TabWorktree>();
        let auto_save = app
            .world_mut()
            .spawn(AutoSave::<WorkspaceStore>::default())
            .id();
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
            .get_mut::<State>(auto_save)
            .unwrap()
            .clone_from(&State::default());
        app.world_mut().entity_mut(tab).remove::<TabWorktree>();

        app.update();

        assert!(app.world().get::<State>(auto_save).unwrap().is_dirty());
    }

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    struct HomeEnvGuard {
        _guard: std::sync::MutexGuard<'static, ()>,
        old_home: Option<std::ffi::OsString>,
        old_tmpdir: Option<std::ffi::OsString>,
    }

    impl HomeEnvGuard {
        fn isolated() -> bool {
            let thread = std::thread::current();
            let name = thread.name().expect("test harness thread has no name");
            if std::env::var("VMUX_STORE_TEST_CHILD").as_deref() == Ok(name) {
                return false;
            }
            let home = tempfile::tempdir().expect("isolated test home");
            let output =
                std::process::Command::new(std::env::current_exe().expect("test executable"))
                    .args(["--exact", name, "--nocapture", "--test-threads=1"])
                    .env("VMUX_STORE_TEST_CHILD", name)
                    .env_remove("VMUX_DATA_DIR")
                    .env("HOME", home.path())
                    .env("TMPDIR", home.path())
                    .output()
                    .expect("isolated test child");
            assert!(
                output.status.success(),
                "isolated test failed: {}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            true
        }

        fn use_temp_home(name: &str) -> Self {
            let thread = std::thread::current();
            assert_eq!(
                std::env::var("VMUX_STORE_TEST_CHILD").ok().as_deref(),
                thread.name(),
                "home fixture requires an isolated test subprocess"
            );
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
                pane: PaneSettings::default(),
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
            .add_plugins((vmux_ecs::PrimitivesPlugin, SchemaPlugin))
            .register_stored::<vmux_ecs::Visit>()
            .register_stored::<vmux_ecs::Url>()
            .register_stored::<vmux_ecs::VisitCount>()
            .register_stored::<vmux_ecs::LastVisitedAt>()
            .register_stored::<vmux_ecs::VisitedUrl>()
            .register_stored::<vmux_ecs::TransitionType>()
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
                CreatedAt(500),
            ))
            .id();

        app_save.world_mut().spawn((
            Save,
            vmux_ecs::Visit,
            vmux_ecs::VisitedUrl(url_e),
            CreatedAt(900),
            vmux_ecs::TransitionType::Typed,
        ));

        save_space_to_path(&mut app_save, path.clone());
        app_save.update();

        assert!(path.exists(), "save file should exist");

        let mut app_load = App::new();
        app_load
            .add_plugins((MinimalPlugins, AssetPlugin::default()))
            .add_plugins((vmux_ecs::PrimitivesPlugin, SchemaPlugin))
            .register_stored::<vmux_ecs::Visit>()
            .register_stored::<vmux_ecs::Url>()
            .register_stored::<vmux_ecs::VisitCount>()
            .register_stored::<vmux_ecs::LastVisitedAt>()
            .register_stored::<vmux_ecs::VisitedUrl>()
            .register_stored::<vmux_ecs::TransitionType>()
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
            .add_plugins((vmux_ecs::PrimitivesPlugin, SchemaPlugin))
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
        save_space_to_path(&mut app_save, path.clone());
        app_save.update();

        let current = std::fs::read_to_string(&path).expect("saved store");
        let legacy = current.replacen("Builtin(Project)", "Builtin(GitBranch)", 1);
        assert_ne!(legacy, current, "project icon serialized differently");
        std::fs::write(&path, legacy).expect("legacy store");

        let mut app_load = App::new();
        app_load
            .add_plugins((MinimalPlugins, AssetPlugin::default()))
            .add_plugins((vmux_ecs::PrimitivesPlugin, SchemaPlugin))
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
    fn bookmark_card_choices_survive_workspace_save_and_load() {
        for expanded in [false, true] {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("store.ron");
            let mut saved = App::new();
            saved
                .add_plugins(MinimalPlugins)
                .add_plugins(vmux_ecs::PrimitivesPlugin)
                .register_stored::<Space>()
                .register_stored::<vmux_layout::side_sheet::SideSheetSectionsExpanded>()
                .add_observer(save_on_default_event);
            saved.world_mut().spawn((
                Space,
                vmux_layout::side_sheet::SideSheetSectionsExpanded {
                    bookmarks: expanded,
                    ..Default::default()
                },
            ));
            save_space_to_path(&mut saved, path.clone());
            saved.update();

            let mut restored = App::new();
            restored
                .add_plugins((MinimalPlugins, AssetPlugin::default()))
                .add_plugins(vmux_ecs::PrimitivesPlugin)
                .register_stored::<Space>()
                .register_stored::<vmux_layout::side_sheet::SideSheetSectionsExpanded>()
                .add_observer(load_on_default_event);
            restored.update();
            restored
                .world_mut()
                .commands()
                .trigger_load(LoadWorld::default_from_file(path));
            restored.update();
            let sections = restored
                .world_mut()
                .query_filtered::<&vmux_layout::side_sheet::SideSheetSectionsExpanded, With<Space>>(
                )
                .single(restored.world())
                .unwrap();
            assert_eq!(sections.bookmarks, expanded);
        }
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
            .add_plugins((vmux_ecs::PrimitivesPlugin, SchemaPlugin))
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
        app_save.add_plugins((vmux_ecs::PrimitivesPlugin, SchemaPlugin));
        app_save
            .register_stored::<WindowGeometry>()
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

        save_space_to_path(&mut app_save, path.clone());
        app_save.update();
        assert!(path.exists(), "store file should exist");

        let mut app_load = App::new();
        app_load
            .add_plugins((MinimalPlugins, AssetPlugin::default()))
            .add_plugins((vmux_ecs::PrimitivesPlugin, SchemaPlugin));
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
        if HomeEnvGuard::isolated() {
            return;
        }
        let _home = HomeEnvGuard::use_temp_home("custom-save-schema-version");
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("custom-store.ron");

        let mut app_save = App::new();
        app_save
            .add_plugins(MinimalPlugins)
            .add_plugins((vmux_ecs::PrimitivesPlugin, SchemaPlugin))
            .register_stored::<Space>()
            .register_stored::<SpaceId>()
            .register_stored::<WindowGeometry>()
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

        save_space_to_path(&mut app_save, path.clone());
        app_save.update();

        assert!(path.exists(), "custom store should be saved");
        assert!(
            dir.path().join("store.version").exists(),
            "schema version should be written next to custom store"
        );
        assert!(
            !RestoreFailure::store_version(&ProfilePaths::current().store().join("store.ron"))
                .exists(),
            "custom save must not write default store.version"
        );
    }

    #[test]
    fn pane_id_and_position_round_trip_through_store() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("store.ron");

        let mut app_save = App::new();
        app_save.add_plugins(MinimalPlugins);
        app_save.add_plugins((vmux_ecs::PrimitivesPlugin, SchemaPlugin));
        app_save
            .register_stored::<PaneId>()
            .register_stored::<ArchivedPage>()
            .register_stored::<ArchivedPagePosition>()
            .register_stored::<ArchivedTabPage>()
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
        save_space_to_path(&mut app_save, path.clone());
        app_save.update();
        assert!(path.exists());

        let mut app_load = App::new();
        app_load
            .add_plugins((MinimalPlugins, AssetPlugin::default()))
            .add_plugins((vmux_ecs::PrimitivesPlugin, SchemaPlugin))
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
            .add_plugins((vmux_ecs::PrimitivesPlugin, SchemaPlugin));
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
        let key = <PageMetadata as bevy::reflect::TypePath>::type_path();
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
        "PageMetadata": (),
      },
    ),
  },
)
"#;

        let normalized = WorkspaceStoreSource(body)
            .without_resources()
            .expect("legacy resources");

        assert!(normalized.contains("resources: {}"));
        assert!(normalized.contains("PageMetadata"));
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
        let (normalized, unknown) =
            vmux_api::PageIcon::normalize_persisted(body).expect("unknown icons");

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
        assert_eq!(vmux_api::PageIcon::normalize_persisted(body), None);
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
        let (normalized, _) = vmux_api::PageIcon::normalize_persisted(body).expect("unknown icon");
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .add_plugins((vmux_ecs::PrimitivesPlugin, SchemaPlugin))
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
    fn newer_store_schema_is_preserved_and_startup_refused() {
        if HomeEnvGuard::isolated() {
            return;
        }
        let _home = HomeEnvGuard::use_temp_home("newer-store-is-preserved-on-startup");
        let store = ProfilePaths::current().store().join("store.ron");
        let version = RestoreFailure::store_version(&store);
        let newer = (STORE_SCHEMA_VERSION + 1).to_string();
        for body in [b"(resources: {}, entities: {})".as_slice(), &[0xff]] {
            std::fs::write(&store, body).unwrap();
            std::fs::write(&version, &newer).unwrap();
            let mut app = App::new();
            app.add_plugins(MinimalPlugins)
                .insert_resource(test_settings())
                .add_plugins(WorkspaceStorePlugin);
            app.update();
            assert_eq!(std::fs::read_to_string(&version).unwrap(), newer);
            assert_eq!(std::fs::read(&store).unwrap(), body);
            assert!(app.world().contains_resource::<WorkspaceLoadRefused>());
            assert_eq!(app.should_exit(), Some(AppExit::error()));
        }
    }

    #[test]
    fn incompatible_store_is_backed_up_before_starting_a_new_workspace() {
        if HomeEnvGuard::isolated() {
            return;
        }
        let _home = HomeEnvGuard::use_temp_home("incompatible-store-is-backed-up-on-startup");
        let store = ProfilePaths::current().store().join("store.ron");
        let version = RestoreFailure::store_version(&store);
        let original = store_body_with_key("vmux_desktop::ghost::DoesNotExist");
        std::fs::write(&store, &original).unwrap();
        std::fs::write(&version, STORE_SCHEMA_VERSION.to_string()).unwrap();

        let mut app = App::new();
        app.add_plugins((MinimalPlugins, (vmux_ecs::PrimitivesPlugin, SchemaPlugin)))
            .register_stored::<Space>()
            .insert_resource(test_settings())
            .add_plugins(WorkspaceStorePlugin);
        app.update();

        let backup = store.with_extension(format!("ron.pre-v{STORE_SCHEMA_VERSION}.0"));
        assert_eq!(std::fs::read_to_string(&backup).unwrap(), original);
        assert!(!store.exists());
        assert!(!app.world().contains_resource::<WorkspaceLoadRefused>());
        assert_eq!(app.should_exit(), None);
        app.world_mut()
            .spawn((Save, Space, Name::new("New workspace")));
        app.world_mut().write_message(AppExit::Success);
        app.update();
        assert!(
            std::fs::read_to_string(&store)
                .unwrap()
                .contains("New workspace")
        );
        assert_eq!(
            RestoreFailure::schema_version(&store),
            Some(STORE_SCHEMA_VERSION)
        );
        assert_eq!(std::fs::read_to_string(&backup).unwrap(), original);
    }

    #[test]
    fn missing_older_and_invalid_schema_versions_recover_without_overwriting_backups() {
        if HomeEnvGuard::isolated() {
            return;
        }
        let _home = HomeEnvGuard::use_temp_home("older-workspace-schema-backups");
        let store = ProfilePaths::current().store().join("store.ron");
        let store_version = RestoreFailure::store_version(&store);
        for (sequence, version) in [None, Some("3"), Some("invalid")].into_iter().enumerate() {
            if let Some(version) = version {
                std::fs::write(&store_version, version).unwrap();
            }
            let original = format!("(resources: {{}}, entities: {{}})\n{sequence}");
            std::fs::write(&store, &original).unwrap();
            let mut app = App::new();
            app.add_plugins(MinimalPlugins)
                .insert_resource(test_settings())
                .add_plugins(WorkspaceStorePlugin);
            app.update();
            let backup =
                store.with_extension(format!("ron.pre-v{STORE_SCHEMA_VERSION}.{sequence}"));
            assert_eq!(std::fs::read_to_string(&backup).unwrap(), original);
            let mut schema = backup.as_os_str().to_owned();
            schema.push(".version");
            let schema = PathBuf::from(schema);
            if let Some(version) = version {
                assert_eq!(std::fs::read_to_string(schema).unwrap(), version);
            } else {
                assert!(!schema.exists());
            }
            assert!(!store.exists());
            assert!(!app.world().contains_resource::<WorkspaceLoadRefused>());
            assert_eq!(app.should_exit(), None);
        }
        assert_eq!(
            std::fs::read_to_string(
                store.with_extension(format!("ron.pre-v{STORE_SCHEMA_VERSION}.1.version"))
            )
            .unwrap(),
            "3"
        );
    }

    #[test]
    fn failed_schema_backup_refuses_startup_without_removing_the_original_store() {
        if HomeEnvGuard::isolated() {
            return;
        }
        let _home = HomeEnvGuard::use_temp_home("failed-workspace-schema-backup");
        let store = ProfilePaths::current().store().join("store.ron");
        let version = RestoreFailure::store_version(&store);
        let original = "(resources: {}, entities: {})";
        std::fs::write(&store, original).unwrap();
        std::fs::write(&version, "3").unwrap();
        std::fs::create_dir(
            store.with_extension(format!("ron.pre-v{STORE_SCHEMA_VERSION}.0.version")),
        )
        .unwrap();
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(test_settings())
            .add_plugins(WorkspaceStorePlugin);
        app.update();
        assert_eq!(std::fs::read_to_string(&store).unwrap(), original);
        assert_eq!(std::fs::read_to_string(&version).unwrap(), "3");
        assert!(app.world().contains_resource::<WorkspaceLoadRefused>());
        assert_eq!(app.should_exit(), Some(AppExit::error()));
    }

    #[test]
    fn truncated_and_invalid_registered_components_recover_from_current_schema_stores() {
        if HomeEnvGuard::isolated() {
            return;
        }
        let _home = HomeEnvGuard::use_temp_home("invalid-current-workspace-store");
        let store = ProfilePaths::current().store().join("store.ron");
        let version = RestoreFailure::store_version(&store);
        let invalid_component = store_body_with_key("bevy_ecs::name::Name");
        for body in ["(resources: {}, entities: {", invalid_component.as_str()] {
            std::fs::write(&store, body).unwrap();
            std::fs::write(&version, STORE_SCHEMA_VERSION.to_string()).unwrap();
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, AssetPlugin::default()))
                .register_type::<Name>()
                .insert_resource(test_settings())
                .add_plugins(WorkspaceStorePlugin);
            app.update();
            let failure = app
                .world_mut()
                .query::<&RestoreFailure>()
                .single(app.world())
                .unwrap();
            let backup = failure.backup.as_ref().unwrap();
            assert_eq!(std::fs::read_to_string(backup).unwrap(), body);
            assert_eq!(std::fs::read_to_string(&store).unwrap(), body);
            assert!(!app.world().contains_resource::<WorkspaceLoadRefused>());
            assert_eq!(app.should_exit(), None);
            let restore = app
                .world_mut()
                .query::<&WorkspaceRestore>()
                .single(app.world())
                .unwrap();
            assert!(restore.complete);
            assert!(!restore.store_present);
        }
    }

    #[test]
    fn failed_schema_writes_retry_instead_of_reporting_a_saved_workspace() {
        if HomeEnvGuard::isolated() {
            return;
        }
        let mut fixture = WorkspaceSaveFixture::new("failed-workspace-schema-write");
        let version = RestoreFailure::store_version(&fixture.path);
        std::fs::create_dir(&version).unwrap();
        fixture.clock.advance(std::time::Duration::from_secs(61));
        fixture.app.update();
        fixture.finish_write();
        assert!(
            fixture
                .app
                .world()
                .get::<State>(fixture.store)
                .unwrap()
                .is_dirty()
        );
        assert_eq!(fixture.app.world().resource::<SaveCount>().0, 0);
        std::fs::remove_dir(version).unwrap();
        fixture.clock.advance(std::time::Duration::from_secs(61));
        fixture.app.update();
        fixture.finish_write();
        assert!(
            !fixture
                .app
                .world()
                .get::<State>(fixture.store)
                .unwrap()
                .is_dirty()
        );
        assert_eq!(fixture.app.world().resource::<SaveCount>().0, 1);
        assert_eq!(
            RestoreFailure::schema_version(&fixture.path),
            Some(STORE_SCHEMA_VERSION)
        );
    }

    #[test]
    fn auto_save_system_skips_save_without_space() {
        if HomeEnvGuard::isolated() {
            return;
        }
        let mut fixture = WorkspaceSaveFixture::new("auto-save-system-skips-without-space");
        let spaces = fixture
            .app
            .world_mut()
            .query_filtered::<Entity, With<Space>>()
            .iter(fixture.app.world())
            .collect::<Vec<_>>();
        for entity in spaces {
            fixture.app.world_mut().entity_mut(entity).remove::<Space>();
        }
        fixture.clock.advance(std::time::Duration::from_secs(61));
        fixture.app.update();
        fixture.finish_write();
        assert!(
            !fixture.path.exists(),
            "auto_save must skip when no Space exists"
        );
        assert_eq!(fixture.app.world().resource::<SaveCount>().0, 0);
    }
}
