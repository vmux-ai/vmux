use std::path::PathBuf;

#[cfg(host)]
use bevy_app::{App, Plugin, PostUpdate, PreUpdate, Update};
use bevy_ecs::prelude::*;
#[cfg(host)]
use bevy_ecs::system::SystemParam;
use bevy_reflect::Reflect;
use moonshine_save::prelude::Save;
use serde::{Deserialize, Serialize};
#[cfg(host)]
use std::collections::{HashMap, HashSet};
use vmux_api::{Id, session::Session};
use vmux_connector::{
    ExternalId, ExternalLink, ExternalLinkIdentity, ExternalScopeRef, ProviderKey, ResourceKind,
};
#[cfg(host)]
use vmux_ecs::persistence::PersistenceAppExt;
#[cfg(host)]
use vmux_ecs::{
    ActivateRequest, CreatedAt, EntityTarget, LastActivatedAt, Operation, OperationKind,
    OperationState, OperationTargetRevision, Order, PageMetadata, Revision,
};
use vmux_ecs::{ArchivedAt, Cwd, Description, RemoteUpdatedAt, UnixMillis};

#[cfg(host)]
use crate::{AgentConversationTitle, TitlePending};

#[cfg(host)]
pub(crate) struct EntityPlugin;

#[cfg(host)]
impl Plugin for EntityPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<vmux_ecs::OperationPlugin>() {
            app.add_plugins(vmux_ecs::OperationPlugin);
        }
        app.world_mut()
            .register_required_components::<Session, Save>();
        app.world_mut()
            .register_required_components::<Session, Cwd>();
        app.world_mut()
            .register_required_components::<Session, Description>();
        app.register_persisted::<Session>()
            .register_persisted::<Id<Session>>()
            .register_persisted::<AgentRef>()
            .register_persisted::<ProviderWorkflowState>()
            .register_persisted::<ProviderArchive>()
            .register_persisted::<ProviderSessionTombstone>()
            .register_persisted::<AgentConversationTitle>()
            .register_persisted::<TitlePending>()
            .init_resource::<ProviderSessionTombstones>()
            .init_resource::<ProviderArchiveRevision>()
            .add_message::<CreateRequest>()
            .add_message::<ArchiveRequest>()
            .add_message::<ProviderArchiveRequest>()
            .add_message::<ProviderArchiveWriteRequest>()
            .add_message::<ProviderArchiveCompleted>()
            .add_message::<ProviderSessionRestoreRequest>()
            .add_message::<CleanupRequest>()
            .add_observer(activate)
            .add_systems(
                Update,
                (
                    create,
                    archive,
                    archive_provider,
                    finish_provider_archives,
                    sync_tombstones,
                    restore_tombstones,
                    cleanup,
                )
                    .chain()
                    .in_set(MutationSet),
            )
            .add_systems(
                PostUpdate,
                (
                    canonicalize_provider_sessions,
                    bevy_ecs::schedule::ApplyDeferred,
                )
                    .chain()
                    .in_set(CanonicalizeSet),
            );
    }
}

#[cfg(host)]
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct MutationSet;

#[cfg(host)]
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct CanonicalizeSet;

#[cfg(host)]
pub(crate) struct MetadataPlugin;

#[cfg(host)]
impl Plugin for MetadataPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<RenameRequest>()
            .add_message::<DescriptionUpdateRequest>()
            .add_systems(
                Update,
                (rename, update_description, sync_conversation_title).in_set(MutationSet),
            )
            .add_systems(Update, project_views.after(MutationSet));
    }
}

#[cfg(host)]
pub(crate) struct WorkflowStatePlugin;

#[cfg(host)]
impl Plugin for WorkflowStatePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Configuration::load())
            .register_persisted::<WorkflowState>()
            .register_persisted::<Id<WorkflowState>>()
            .register_persisted::<WorkflowStateRef>()
            .register_persisted::<WorkflowStateChangedAt>()
            .register_persisted::<TerminalWorkflowState>()
            .init_resource::<WorkflowStateIndex>()
            .init_resource::<IndexedWorkflowStates>()
            .add_message::<WorkflowStateChangeRequest>()
            .add_message::<ProviderWorkflowStateChangeRequest>()
            .add_systems(
                PreUpdate,
                (
                    ensure_workflow_states,
                    bevy_ecs::schedule::ApplyDeferred,
                    rebuild_workflow_state_index,
                )
                    .chain(),
            )
            .add_systems(
                Update,
                (change_workflow_state, change_provider_workflow_state).in_set(MutationSet),
            )
            .add_systems(
                PostUpdate,
                (
                    bevy_ecs::schedule::ApplyDeferred,
                    rebuild_workflow_state_index,
                    resolve_workflow_state_targets,
                )
                    .chain()
                    .after(CanonicalizeSet),
            );
    }
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
#[relationship(relationship_target = SessionDuplicates)]
pub struct SessionDuplicate(Entity);

impl SessionDuplicate {
    pub const fn entity(&self) -> Entity {
        self.0
    }
}

#[derive(Component, Debug)]
#[relationship_target(relationship = SessionDuplicate)]
pub struct SessionDuplicates(Vec<Entity>);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Reflect)]
#[type_path = "vmux_session"]
pub enum Agent {}

#[derive(Component, Clone, Debug, PartialEq, Eq, Hash, Reflect, Serialize, Deserialize)]
#[component(immutable)]
#[reflect(Component)]
#[require(Save)]
#[type_path = "vmux_session"]
pub struct AgentRef(Id<Agent>);

impl AgentRef {
    pub fn new(value: impl Into<String>) -> Self {
        Self(Id::new(value))
    }

    pub fn id(&self) -> &Id<Agent> {
        &self.0
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct AgentFamily(String);

impl AgentFamily {
    pub fn of(agent: &str) -> Self {
        let normalized = agent.to_ascii_lowercase();
        if normalized.contains("codex") {
            return Self("codex".to_string());
        }
        if normalized.contains("claude") {
            return Self("claude".to_string());
        }
        if normalized.contains("mistral") || normalized.contains("vibe") {
            return Self("vibe".to_string());
        }
        Self(normalized)
    }

    pub fn name(&self, fallback: &str) -> String {
        match self.0.as_str() {
            "codex" => "Codex".to_string(),
            "claude" => "Claude".to_string(),
            "vibe" => "Mistral Vibe".to_string(),
            _ if !fallback.trim().is_empty() => fallback.to_string(),
            _ => self.0.clone(),
        }
    }
}

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
#[reflect(Component)]
#[require(Save)]
#[type_path = "vmux_session"]
pub struct WorkflowState;

#[derive(Component, Clone, Debug, PartialEq, Eq, Reflect, Serialize, Deserialize)]
#[component(immutable)]
#[reflect(Component)]
#[require(Save)]
#[type_path = "vmux_session"]
pub struct WorkflowStateRef(Id<WorkflowState>);

impl WorkflowStateRef {
    pub fn new(id: Id<WorkflowState>) -> Self {
        Self(id)
    }

    pub fn id(&self) -> &Id<WorkflowState> {
        &self.0
    }
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
#[relationship(relationship_target = WorkflowStateSources)]
pub struct WorkflowStateTarget(Entity);

impl WorkflowStateTarget {
    pub const fn new(entity: Entity) -> Self {
        Self(entity)
    }

    pub const fn entity(&self) -> Entity {
        self.0
    }
}

#[derive(Component, Debug)]
#[relationship_target(relationship = WorkflowStateTarget)]
pub struct WorkflowStateSources(Vec<Entity>);

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Reflect)]
#[reflect(Component)]
#[require(Save)]
#[type_path = "vmux_session"]
pub struct WorkflowStateChangedAt(UnixMillis);

impl WorkflowStateChangedAt {
    pub const fn new(milliseconds: i64) -> Self {
        Self(UnixMillis(milliseconds))
    }

    pub const fn milliseconds(self) -> i64 {
        self.0.0
    }

    fn record_transition(&mut self, now: UnixMillis) {
        self.0 = now.max(UnixMillis(self.0.0.saturating_add(1)));
    }
}

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
#[reflect(Component)]
#[require(Save)]
#[type_path = "vmux_session"]
pub struct ProviderWorkflowState;

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
#[reflect(Component)]
#[require(Save)]
#[type_path = "vmux_session"]
pub struct ProviderArchive;

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
#[reflect(Component)]
#[require(Save)]
#[type_path = "vmux_session"]
pub struct ProviderSessionTombstone;

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
#[reflect(Component)]
#[require(Save)]
#[type_path = "vmux_session"]
pub struct TerminalWorkflowState;

#[derive(Message, Clone, Debug)]
pub struct CreateRequest {
    id: Id<Session>,
    pub name: String,
    pub description: String,
    pub cwd: PathBuf,
    pub agent: Option<AgentRef>,
    created_at: Option<i64>,
    activated_at: Option<i64>,
}

impl CreateRequest {
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        cwd: PathBuf,
        agent: Option<AgentRef>,
    ) -> Self {
        Self {
            id: Id::<Session>::new(uuid::Uuid::new_v4().to_string()),
            name: name.into(),
            description: description.into(),
            cwd,
            agent,
            created_at: None,
            activated_at: None,
        }
    }

    pub fn imported(
        name: impl Into<String>,
        description: impl Into<String>,
        cwd: PathBuf,
        agent: AgentRef,
        created_at: i64,
        activated_at: i64,
    ) -> Self {
        Self {
            id: Id::<Session>::new(uuid::Uuid::new_v4().to_string()),
            name: name.into(),
            description: description.into(),
            cwd,
            agent: Some(agent),
            created_at: Some(created_at),
            activated_at: Some(activated_at),
        }
    }

    pub fn id(&self) -> &Id<Session> {
        &self.id
    }
}

#[derive(EntityEvent, Clone, Debug)]
pub struct Created {
    #[event_target]
    pub entity: Entity,
    pub id: Id<Session>,
}

#[derive(Message, Clone, Debug)]
pub struct RenameRequest {
    pub id: Id<Session>,
    pub name: String,
}

#[derive(Message, Clone, Debug)]
pub struct DescriptionUpdateRequest {
    pub id: Id<Session>,
    pub description: String,
}

#[derive(Message, Clone, Debug)]
pub struct WorkflowStateChangeRequest {
    pub id: Id<Session>,
    pub workflow_state: Id<WorkflowState>,
}

#[derive(Message, Clone, Debug)]
pub struct ArchiveRequest {
    pub id: Id<Session>,
    pub archived_at: Option<i64>,
}

#[derive(Message, Clone, Debug)]
pub struct ProviderArchiveRequest {
    pub identity: ExternalLinkIdentity,
    pub archived: bool,
    pub source_updated_at: i64,
    pub native: bool,
}

#[derive(Message, Clone, Debug)]
pub struct ProviderArchiveWriteRequest {
    pub identity: ExternalLinkIdentity,
    pub archived: bool,
    pub revision: u64,
}

#[derive(Message, Clone, Debug)]
pub struct ProviderArchiveCompleted {
    pub identity: ExternalLinkIdentity,
    pub archived: bool,
    pub revision: u64,
    pub success: bool,
}

#[derive(Message, Clone, Debug)]
pub struct ProviderSessionRestoreRequest {
    pub identities: Vec<ExternalLinkIdentity>,
}

#[derive(Message, Clone, Debug)]
pub struct ProviderWorkflowStateChangeRequest {
    pub identity: ExternalLinkIdentity,
    pub workflow_state: Id<WorkflowState>,
    pub source_updated_at: i64,
}

#[derive(Message, Clone, Debug)]
pub struct CleanupRequest {
    pub targets: Vec<CleanupTarget>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum CleanupTarget {
    Session(Id<Session>),
    Provider(ExternalLinkIdentity),
}

#[cfg(host)]
#[derive(Resource, Default)]
struct ProviderArchiveRevision(u64);

#[cfg(host)]
impl ProviderArchiveRevision {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(1).max(1);
        self.0
    }
}

#[cfg(host)]
#[derive(Component)]
pub(crate) struct ProviderArchiveOperation {
    identity: ExternalLinkIdentity,
    archived: bool,
    source_updated_at: i64,
    revision: u64,
    in_flight: u64,
}

#[cfg(host)]
impl ProviderArchiveOperation {
    fn write(&self) -> ProviderArchiveWriteRequest {
        ProviderArchiveWriteRequest {
            identity: self.identity.clone(),
            archived: self.archived,
            revision: self.revision,
        }
    }

    pub(crate) fn identity(&self) -> &ExternalLinkIdentity {
        &self.identity
    }

    pub(crate) fn archived_at(&self) -> i64 {
        if self.archived {
            self.source_updated_at.max(1)
        } else {
            0
        }
    }
}

#[cfg(host)]
#[derive(SystemParam)]
pub(crate) struct ProviderArchiveOperations<'w, 's> {
    operations: Query<
        'w,
        's,
        (&'static ProviderArchiveOperation, &'static OperationState),
        With<Operation>,
    >,
    changed: Query<
        'w,
        's,
        (),
        (
            With<Operation>,
            Or<(
                Added<ProviderArchiveOperation>,
                Changed<ProviderArchiveOperation>,
                Added<OperationState>,
                Changed<OperationState>,
            )>,
        ),
    >,
}

#[cfg(host)]
impl ProviderArchiveOperations<'_, '_> {
    pub(crate) fn archived_at(&self, identity: &ExternalLinkIdentity) -> Option<i64> {
        self.operations.iter().find_map(|(operation, state)| {
            (operation.identity() == identity
                && matches!(state, OperationState::Pending | OperationState::Running))
            .then(|| operation.archived_at())
        })
    }

    pub(crate) fn is_changed(&self) -> bool {
        !self.changed.is_empty()
    }
}

#[cfg(host)]
#[derive(Resource, Default)]
pub struct ProviderSessionTombstones {
    entries: HashMap<ExternalLinkIdentity, Option<Entity>>,
}

#[cfg(host)]
impl ProviderSessionTombstones {
    pub fn new(identities: impl IntoIterator<Item = ExternalLinkIdentity>) -> Self {
        let mut tombstones = Self::default();
        for identity in identities {
            tombstones.entries.insert(identity, None);
        }
        tombstones
    }

    pub fn contains(&self, identity: &ExternalLinkIdentity) -> bool {
        self.entries.contains_key(identity)
    }

    fn insert(&mut self, identity: ExternalLinkIdentity, entity: Entity) {
        self.entries.insert(identity, Some(entity));
    }

    fn remove(&mut self, identity: &ExternalLinkIdentity) -> Option<Entity> {
        self.entries.remove(identity).flatten()
    }

    fn replace(&mut self, entries: HashMap<ExternalLinkIdentity, Option<Entity>>) {
        if self.entries != entries {
            self.entries = entries;
        }
    }
}

#[cfg(host)]
#[derive(SystemParam)]
struct TombstoneMutations<'w, 's> {
    index: ResMut<'w, ProviderSessionTombstones>,
    commands: Commands<'w, 's>,
}

#[cfg(host)]
impl TombstoneMutations<'_, '_> {
    fn cleanup(&mut self, entity: Entity) {
        self.commands.trigger(Cleanup { entity });
        self.commands.entity(entity).despawn();
    }

    fn dismiss(&mut self, identities: impl IntoIterator<Item = ExternalLinkIdentity>) {
        for identity in identities {
            if self.index.contains(&identity) {
                continue;
            }
            let entity = self
                .commands
                .spawn((
                    ProviderSessionTombstone,
                    identity.provider().clone(),
                    ExternalScopeRef::new(identity.scope().clone()),
                    identity.kind().clone(),
                    identity.external_id().clone(),
                ))
                .id();
            self.index.insert(identity, entity);
        }
    }

    fn restore(&mut self, identities: impl IntoIterator<Item = ExternalLinkIdentity>) {
        for identity in identities {
            if let Some(entity) = self.index.remove(&identity) {
                self.commands.entity(entity).despawn();
            }
        }
    }
}

#[derive(EntityEvent, Clone, Copy, Debug)]
pub struct Cleanup {
    #[event_target]
    pub entity: Entity,
}

#[cfg(host)]
#[derive(Resource)]
struct Configuration {
    default_workflow_state: Id<WorkflowState>,
    stages: Vec<StageSeed>,
}

#[cfg(host)]
impl Configuration {
    fn load() -> Self {
        let file: FeatureFile = ron::from_str(include_str!("feature.ron"))
            .expect("embedded Session configuration must be valid RON");
        file.session.validate();
        Self {
            default_workflow_state: Id::new(file.session.default_stage),
            stages: file.session.stages,
        }
    }
}

#[cfg(host)]
#[derive(Deserialize)]
struct FeatureFile {
    session: SessionConfiguration,
}

#[cfg(host)]
#[derive(Deserialize)]
struct SessionConfiguration {
    default_stage: String,
    stages: Vec<StageSeed>,
}

#[cfg(host)]
impl SessionConfiguration {
    fn validate(&self) {
        let mut ids = HashSet::new();
        let mut orders = HashSet::new();
        for stage in &self.stages {
            assert!(
                !stage.id.trim().is_empty(),
                "Session stage ID must not be empty"
            );
            assert!(ids.insert(stage.id.as_str()), "duplicate Session stage ID");
            assert!(orders.insert(stage.order), "duplicate Session stage order");
        }
        assert!(
            ids.contains(self.default_stage.as_str()),
            "default Session stage must reference a configured stage"
        );
    }
}

#[cfg(host)]
#[derive(Clone, Deserialize)]
struct StageSeed {
    id: String,
    name: String,
    terminal: bool,
    order: u32,
}

#[cfg(host)]
#[derive(Resource, Default)]
pub struct WorkflowStateIndex {
    entities: HashMap<Id<WorkflowState>, Entity>,
    duplicates: HashSet<Id<WorkflowState>>,
}

#[cfg(host)]
impl WorkflowStateIndex {
    pub fn entity(
        &self,
        id: &Id<WorkflowState>,
    ) -> Result<Option<Entity>, WorkflowStateLookupError> {
        if self.duplicates.contains(id) {
            return Err(WorkflowStateLookupError::Duplicate(id.clone()));
        }
        Ok(self.entities.get(id).copied())
    }
}

#[cfg(host)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkflowStateLookupError {
    Duplicate(Id<WorkflowState>),
}

#[cfg(host)]
impl std::fmt::Display for WorkflowStateLookupError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Duplicate(id) => write!(formatter, "duplicate workflow state ID `{id}`"),
        }
    }
}

#[cfg(host)]
#[derive(Resource, Default)]
struct IndexedWorkflowStates(HashSet<Entity>);

#[cfg(host)]
fn rebuild_workflow_state_index(
    states: Query<(Entity, &Id<WorkflowState>), With<WorkflowState>>,
    changed: Query<
        (),
        (
            With<WorkflowState>,
            Or<(Added<WorkflowState>, Changed<Id<WorkflowState>>)>,
        ),
    >,
    mut removed_states: RemovedComponents<WorkflowState>,
    mut removed_ids: RemovedComponents<Id<WorkflowState>>,
    mut current: ResMut<WorkflowStateIndex>,
    mut indexed: ResMut<IndexedWorkflowStates>,
) {
    let removed_state = removed_states
        .read()
        .any(|entity| indexed.0.contains(&entity));
    let removed_id = removed_ids.read().any(|entity| indexed.0.contains(&entity));
    if changed.is_empty() && !removed_state && !removed_id {
        return;
    }

    let mut next = WorkflowStateIndex::default();
    let mut next_indexed = HashSet::new();
    for (entity, id) in &states {
        next_indexed.insert(entity);
        if next.entities.insert(id.clone(), entity).is_some() {
            next.duplicates.insert(id.clone());
        }
    }
    *current = next;
    indexed.0 = next_indexed;
}

#[cfg(host)]
fn resolve_workflow_state_targets(
    index: Res<WorkflowStateIndex>,
    references: Query<(Entity, Ref<WorkflowStateRef>, Option<&WorkflowStateTarget>)>,
    mut commands: Commands,
) {
    for (entity, reference, current) in &references {
        if !index.is_changed() && !reference.is_changed() {
            continue;
        }
        let target = index.entity(reference.id()).ok().flatten();
        match (target, current.map(WorkflowStateTarget::entity)) {
            (Some(target), Some(current)) if target == current => {}
            (Some(target), _) => {
                commands
                    .entity(entity)
                    .insert(WorkflowStateTarget::new(target));
            }
            (None, Some(_)) => {
                commands.entity(entity).remove::<WorkflowStateTarget>();
            }
            (None, None) => {}
        }
    }
}

#[cfg(host)]
fn ensure_workflow_states(
    configuration: Res<Configuration>,
    definitions: Query<
        (
            Entity,
            &Id<WorkflowState>,
            &Name,
            &Order,
            Has<TerminalWorkflowState>,
        ),
        With<WorkflowState>,
    >,
    mut commands: Commands,
) {
    for stage in &configuration.stages {
        if definitions
            .iter()
            .any(|(_, id, _, _, _)| id.as_str() == stage.id)
        {
            continue;
        }
        let mut entity = commands.spawn((
            WorkflowState,
            Id::<WorkflowState>::new(stage.id.clone()),
            Name::new(stage.name.clone()),
            Order(stage.order),
        ));
        if stage.terminal {
            entity.insert(TerminalWorkflowState);
        }
    }
}

#[cfg(host)]
fn create(
    mut requests: MessageReader<CreateRequest>,
    configuration: Res<Configuration>,
    existing: Query<&Id<Session>, With<Session>>,
    mut commands: Commands,
) {
    if requests.is_empty() {
        return;
    }
    let mut ids = existing.iter().cloned().collect::<HashSet<_>>();
    for request in requests.read() {
        if !ids.insert(request.id.clone()) {
            continue;
        }
        let now = UnixMillis::now().0;
        let created_at = request.created_at.unwrap_or(now);
        let activated_at = request.activated_at.unwrap_or(now);
        let mut session = commands.spawn((
            Session,
            request.id.clone(),
            Name::new(request.name.clone()),
            Description(request.description.clone()),
            Cwd(request.cwd.clone()),
            CreatedAt::new(created_at),
            LastActivatedAt::new(activated_at),
            WorkflowStateRef::new(configuration.default_workflow_state.clone()),
            WorkflowStateChangedAt::new(activated_at),
        ));
        if let Some(agent) = request.agent.clone() {
            session.insert(agent);
        }
        let entity = session.id();
        commands.trigger(Created {
            entity,
            id: request.id.clone(),
        });
    }
}

#[cfg(host)]
fn activate(
    trigger: On<ActivateRequest>,
    parents: Query<&ChildOf>,
    targets: Query<&EntityTarget<Session>>,
    mut sessions: Query<&mut LastActivatedAt, With<Session>>,
) {
    let mut current = trigger.event_target();
    loop {
        if let Ok(mut activated_at) = sessions.get_mut(current) {
            *activated_at = LastActivatedAt::now();
            return;
        }
        if let Ok(target) = targets.get(current)
            && let Ok(mut activated_at) = sessions.get_mut(target.entity())
        {
            *activated_at = LastActivatedAt::now();
            return;
        }
        let Ok(parent) = parents.get(current) else {
            return;
        };
        current = parent.parent();
    }
}

#[cfg(host)]
fn project_views(
    sessions: Query<(&Id<Session>, Ref<Name>), With<Session>>,
    views: Query<(Entity, Ref<EntityTarget<Session>>, Option<&Children>)>,
    mut metadata: Query<&mut PageMetadata>,
) {
    for (view, target, children) in &views {
        let Ok((id, name)) = sessions.get(target.entity()) else {
            continue;
        };
        if !target.is_added() && !target.is_changed() && !name.is_changed() {
            continue;
        }
        let url = crate::Route::Session(id.clone()).url();
        for entity in
            std::iter::once(view).chain(children.into_iter().flat_map(|children| children.iter()))
        {
            let Ok(mut page) = metadata.get_mut(entity) else {
                continue;
            };
            page.title = name.as_str().to_string();
            page.url.clone_from(&url);
        }
    }
}

#[cfg(host)]
fn sync_conversation_title(
    mut sessions: Query<
        (&AgentConversationTitle, &mut Name),
        (With<Session>, Changed<AgentConversationTitle>),
    >,
) {
    for (title, mut name) in &mut sessions {
        let title = title.0.trim();
        if !title.is_empty() && name.as_str() != title {
            *name = Name::new(title.to_string());
        }
    }
}

#[cfg(host)]
fn rename(
    mut requests: MessageReader<RenameRequest>,
    mut sessions: Query<
        (
            Entity,
            &Id<Session>,
            &mut Name,
            Option<&mut AgentConversationTitle>,
        ),
        With<Session>,
    >,
    mut commands: Commands,
) {
    for request in requests.read() {
        for (entity, id, mut name, title) in &mut sessions {
            if id == &request.id {
                *name = Name::new(request.name.clone());
                if let Some(mut title) = title {
                    title.0.clone_from(&request.name);
                } else {
                    commands
                        .entity(entity)
                        .insert(AgentConversationTitle(request.name.clone()));
                }
                commands.entity(entity).remove::<TitlePending>();
            }
        }
    }
}

#[cfg(host)]
fn update_description(
    mut requests: MessageReader<DescriptionUpdateRequest>,
    mut sessions: Query<(&Id<Session>, &mut Description), With<Session>>,
) {
    for request in requests.read() {
        for (id, mut description) in &mut sessions {
            if id == &request.id {
                description.0.clone_from(&request.description);
            }
        }
    }
}

#[cfg(host)]
fn change_workflow_state(
    mut requests: MessageReader<WorkflowStateChangeRequest>,
    index: Res<WorkflowStateIndex>,
    mut sessions: Query<
        (
            Entity,
            &Id<Session>,
            &WorkflowStateRef,
            &mut WorkflowStateChangedAt,
        ),
        With<Session>,
    >,
    mut commands: Commands,
) {
    for request in requests.read() {
        let Ok(Some(target)) = index.entity(&request.workflow_state) else {
            continue;
        };
        for (entity, id, current, mut changed_at) in &mut sessions {
            if id == &request.id && current.id() != &request.workflow_state {
                commands.entity(entity).insert((
                    WorkflowStateRef::new(request.workflow_state.clone()),
                    WorkflowStateTarget::new(target),
                ));
                changed_at.record_transition(UnixMillis::now());
            }
        }
    }
}

#[cfg(host)]
fn archive(
    mut requests: MessageReader<ArchiveRequest>,
    sessions: Query<(Entity, &Id<Session>, Option<&ArchivedAt>), With<Session>>,
    mut commands: Commands,
) {
    for request in requests.read() {
        for (entity, id, current) in &sessions {
            if id != &request.id {
                continue;
            }
            if let Some(archived_at) = request.archived_at.map(|value| value.max(1)) {
                if current.is_none_or(|current| current.milliseconds() != archived_at) {
                    commands.entity(entity).insert(ArchivedAt::new(archived_at));
                }
            } else if current.is_some() {
                commands.entity(entity).remove::<ArchivedAt>();
            }
        }
    }
}

#[cfg(host)]
fn archive_provider(
    mut requests: MessageReader<ProviderArchiveRequest>,
    mut revision: ResMut<ProviderArchiveRevision>,
    mut operations: Query<
        (Entity, &mut ProviderArchiveOperation, &mut OperationState),
        With<Operation>,
    >,
    mut overrides: Query<
        (
            Entity,
            &ProviderKey,
            &ExternalScopeRef,
            &ResourceKind,
            &ExternalId,
            &RemoteUpdatedAt,
            Option<&ArchivedAt>,
        ),
        With<ProviderArchive>,
    >,
    mut writes: MessageWriter<ProviderArchiveWriteRequest>,
    mut commands: Commands,
) {
    let mut latest = HashMap::<ExternalLinkIdentity, ProviderArchiveRequest>::new();
    for request in requests.read().cloned() {
        latest.insert(request.identity.clone(), request);
    }
    for request in latest.into_values() {
        let next_archived_at = request.source_updated_at.max(1);
        let mut found = false;
        for (entity, provider, scope, kind, id, source_updated_at, current_archived_at) in
            &mut overrides
        {
            let identity = ExternalLinkIdentity::new(
                provider.clone(),
                scope.id().clone(),
                kind.clone(),
                id.clone(),
            );
            if identity != request.identity {
                continue;
            }
            if source_updated_at.0.0 != request.source_updated_at {
                commands
                    .entity(entity)
                    .insert(RemoteUpdatedAt(UnixMillis(request.source_updated_at)));
            }
            if request.archived && current_archived_at.is_none() {
                commands
                    .entity(entity)
                    .insert(ArchivedAt::new(next_archived_at));
            } else if !request.archived && current_archived_at.is_some() {
                commands.entity(entity).remove::<ArchivedAt>();
            }
            found = true;
            break;
        }
        if !found {
            let mut entity = commands.spawn((
                ProviderArchive,
                request.identity.provider().clone(),
                ExternalScopeRef::new(request.identity.scope().clone()),
                request.identity.kind().clone(),
                request.identity.external_id().clone(),
                RemoteUpdatedAt(UnixMillis(request.source_updated_at)),
            ));
            if request.archived {
                entity.insert(ArchivedAt::new(next_archived_at));
            }
        }
        if !request.native {
            continue;
        }
        let next_revision = revision.next();
        let mut accepted = false;
        for (entity, mut operation, mut state) in &mut operations {
            if operation.identity != request.identity {
                continue;
            }
            operation.archived = request.archived;
            operation.source_updated_at = request.source_updated_at;
            operation.revision = next_revision;
            commands
                .entity(entity)
                .insert(OperationTargetRevision(Revision::new(
                    request.source_updated_at.max(1) as u64,
                )));
            if !matches!(*state, OperationState::Pending | OperationState::Running) {
                operation.in_flight = next_revision;
                *state = OperationState::Running;
                writes.write(operation.write());
            }
            accepted = true;
            break;
        }
        if accepted {
            continue;
        }
        let operation = ProviderArchiveOperation {
            identity: request.identity.clone(),
            archived: request.archived,
            source_updated_at: request.source_updated_at,
            revision: next_revision,
            in_flight: next_revision,
        };
        writes.write(operation.write());
        commands.spawn((
            Operation,
            Id::<Operation>::new(uuid::Uuid::new_v4().to_string()),
            OperationKind::new("provider_archive").unwrap(),
            OperationState::Running,
            OperationTargetRevision(Revision::new(request.source_updated_at.max(1) as u64)),
            operation,
        ));
    }
}

#[cfg(host)]
fn finish_provider_archives(
    mut completed: MessageReader<ProviderArchiveCompleted>,
    mut operations: Query<
        (Entity, &mut ProviderArchiveOperation, &mut OperationState),
        With<Operation>,
    >,
    mut writes: MessageWriter<ProviderArchiveWriteRequest>,
    mut commands: Commands,
) {
    for result in completed.read() {
        for (entity, mut operation, mut state) in &mut operations {
            if operation.identity != result.identity || operation.in_flight != result.revision {
                continue;
            }
            if operation.revision != result.revision {
                operation.in_flight = operation.revision;
                writes.write(operation.write());
                break;
            }
            *state = if result.success {
                OperationState::Succeeded
            } else {
                commands.entity(entity).insert(
                    vmux_ecs::OperationError::new("provider archive write failed").unwrap(),
                );
                OperationState::Failed
            };
            break;
        }
    }
}

#[cfg(host)]
fn sync_tombstones(
    tombstones: Query<
        (
            Entity,
            &ProviderKey,
            &ExternalScopeRef,
            &ResourceKind,
            &ExternalId,
        ),
        With<ProviderSessionTombstone>,
    >,
    changed: Query<
        (),
        (
            With<ProviderSessionTombstone>,
            Or<(
                Added<ProviderSessionTombstone>,
                Changed<ProviderKey>,
                Changed<ExternalScopeRef>,
                Changed<ResourceKind>,
                Changed<ExternalId>,
            )>,
        ),
    >,
    mut removed_tombstones: RemovedComponents<ProviderSessionTombstone>,
    mut index: ResMut<ProviderSessionTombstones>,
) {
    let removed = removed_tombstones.read().next().is_some();
    if !index.is_added() && changed.is_empty() && !removed {
        return;
    }
    let mut entries = HashMap::<ExternalLinkIdentity, Option<Entity>>::new();
    for (entity, provider, scope, kind, id) in &tombstones {
        entries.insert(
            ExternalLinkIdentity::new(
                provider.clone(),
                scope.id().clone(),
                kind.clone(),
                id.clone(),
            ),
            Some(entity),
        );
    }
    index.replace(entries);
}

#[cfg(host)]
fn restore_tombstones(
    mut requests: MessageReader<ProviderSessionRestoreRequest>,
    mut tombstones: TombstoneMutations,
) {
    for request in requests.read() {
        tombstones.restore(request.identities.iter().cloned());
    }
}

#[cfg(host)]
fn change_provider_workflow_state(
    mut requests: MessageReader<ProviderWorkflowStateChangeRequest>,
    index: Res<WorkflowStateIndex>,
    mut stages: Query<
        (
            Entity,
            &ProviderKey,
            &ExternalScopeRef,
            &ResourceKind,
            &ExternalId,
            &WorkflowStateRef,
            &mut WorkflowStateChangedAt,
            &RemoteUpdatedAt,
        ),
        With<ProviderWorkflowState>,
    >,
    mut commands: Commands,
) {
    for request in requests.read() {
        let Ok(Some(target)) = index.entity(&request.workflow_state) else {
            continue;
        };
        let now = UnixMillis::now();
        let mut found = false;
        for (entity, provider, scope, kind, id, current, mut changed_at, source_updated_at) in
            &mut stages
        {
            let identity = ExternalLinkIdentity::new(
                provider.clone(),
                scope.id().clone(),
                kind.clone(),
                id.clone(),
            );
            if identity != request.identity {
                continue;
            }
            found = true;
            if current.id() != &request.workflow_state {
                commands.entity(entity).insert((
                    WorkflowStateRef::new(request.workflow_state.clone()),
                    WorkflowStateTarget::new(target),
                ));
            }
            changed_at.record_transition(now);
            if source_updated_at.0.0 != request.source_updated_at {
                commands
                    .entity(entity)
                    .insert(RemoteUpdatedAt(UnixMillis(request.source_updated_at)));
            }
            break;
        }
        if !found {
            commands.spawn((
                ProviderWorkflowState,
                request.identity.provider().clone(),
                ExternalScopeRef::new(request.identity.scope().clone()),
                request.identity.kind().clone(),
                request.identity.external_id().clone(),
                WorkflowStateRef::new(request.workflow_state.clone()),
                WorkflowStateTarget::new(target),
                WorkflowStateChangedAt(now),
                RemoteUpdatedAt(UnixMillis(request.source_updated_at)),
            ));
        }
    }
}

#[cfg(host)]
fn cleanup(
    mut requests: MessageReader<CleanupRequest>,
    sessions: Query<(Entity, &Id<Session>), With<Session>>,
    links: Query<
        (
            &ChildOf,
            &ProviderKey,
            &ExternalScopeRef,
            &ResourceKind,
            &ExternalId,
        ),
        With<ExternalLink>,
    >,
    mut tombstones: TombstoneMutations,
) {
    let mut requested_sessions = HashSet::new();
    let mut dismissed_providers = HashSet::new();
    for request in requests.read() {
        for target in &request.targets {
            match target {
                CleanupTarget::Session(id) => {
                    requested_sessions.insert(id.clone());
                }
                CleanupTarget::Provider(key) => {
                    dismissed_providers.insert(key.clone());
                }
            }
        }
    }
    if requested_sessions.is_empty() && dismissed_providers.is_empty() {
        return;
    }
    let linked = links
        .iter()
        .filter(|(_, _, _, kind, _)| kind.as_str() == "session")
        .map(|(owner, provider, scope, kind, id)| {
            (
                owner.parent(),
                ExternalLinkIdentity::new(
                    provider.clone(),
                    scope.id().clone(),
                    kind.clone(),
                    id.clone(),
                ),
            )
        })
        .collect::<HashMap<_, _>>();
    let sessions = sessions
        .iter()
        .map(|(entity, id)| (entity, id.clone(), linked.get(&entity).cloned()))
        .collect::<Vec<_>>();
    for (_, id, provider) in &sessions {
        if requested_sessions.contains(id)
            && let Some(provider) = provider
        {
            dismissed_providers.insert(provider.clone());
        }
    }
    for (entity, id, provider) in sessions {
        if requested_sessions.contains(&id)
            || provider.is_some_and(|provider| dismissed_providers.contains(&provider))
        {
            tombstones.cleanup(entity);
        }
    }
    tombstones.dismiss(dismissed_providers);
}

#[cfg(host)]
struct ProviderSessionCandidate {
    entity: Entity,
    duplicate: Option<Entity>,
    archived: bool,
    active: bool,
    targeted: bool,
    activated_at: i64,
    stage_changed_at: i64,
}

#[cfg(host)]
#[derive(SystemParam)]
struct CanonicalizationChanges<'w, 's> {
    sessions: Query<
        'w,
        's,
        (),
        (
            With<Session>,
            Or<(
                Added<Session>,
                Changed<LastActivatedAt>,
                Changed<WorkflowStateChangedAt>,
                Changed<ArchivedAt>,
                Changed<crate::RunState>,
            )>,
        ),
    >,
    links: Query<
        'w,
        's,
        (),
        (
            With<ExternalLink>,
            Or<(
                Added<ExternalLink>,
                Changed<ProviderKey>,
                Changed<ExternalScopeRef>,
                Changed<ResourceKind>,
                Changed<ExternalId>,
                Changed<ChildOf>,
            )>,
        ),
    >,
    targets: Query<'w, 's, (), Or<(Added<EntityTarget<Session>>, Changed<EntityTarget<Session>>)>>,
    removed_sessions: RemovedComponents<'w, 's, Session>,
    removed_links: RemovedComponents<'w, 's, ExternalLink>,
    removed_providers: RemovedComponents<'w, 's, ProviderKey>,
    removed_scopes: RemovedComponents<'w, 's, ExternalScopeRef>,
    removed_kinds: RemovedComponents<'w, 's, ResourceKind>,
    removed_external_ids: RemovedComponents<'w, 's, ExternalId>,
    removed_owners: RemovedComponents<'w, 's, ChildOf>,
    removed_activated_at: RemovedComponents<'w, 's, LastActivatedAt>,
    removed_stage_changed_at: RemovedComponents<'w, 's, WorkflowStateChangedAt>,
    removed_archived_at: RemovedComponents<'w, 's, ArchivedAt>,
    removed_run_state: RemovedComponents<'w, 's, crate::RunState>,
    removed_targets: RemovedComponents<'w, 's, EntityTarget<Session>>,
}

#[cfg(host)]
impl CanonicalizationChanges<'_, '_> {
    fn any(&mut self) -> bool {
        !self.sessions.is_empty()
            || !self.links.is_empty()
            || !self.targets.is_empty()
            || self.removed_sessions.read().count() > 0
            || self.removed_links.read().count() > 0
            || self.removed_providers.read().count() > 0
            || self.removed_scopes.read().count() > 0
            || self.removed_kinds.read().count() > 0
            || self.removed_external_ids.read().count() > 0
            || self.removed_owners.read().count() > 0
            || self.removed_activated_at.read().count() > 0
            || self.removed_stage_changed_at.read().count() > 0
            || self.removed_archived_at.read().count() > 0
            || self.removed_run_state.read().count() > 0
            || self.removed_targets.read().count() > 0
    }
}

#[cfg(host)]
impl ProviderSessionCandidate {
    fn priority(&self) -> (bool, bool, bool, i64, i64, u32) {
        (
            !self.archived,
            self.active,
            self.targeted,
            self.activated_at,
            self.stage_changed_at,
            self.entity.index().index(),
        )
    }
}

#[cfg(host)]
fn canonicalize_provider_sessions(
    mut changes: CanonicalizationChanges,
    sessions: Query<
        (
            Option<&LastActivatedAt>,
            Option<&WorkflowStateChangedAt>,
            Has<ArchivedAt>,
            Option<&crate::RunState>,
            Option<&SessionDuplicate>,
        ),
        With<Session>,
    >,
    links: Query<
        (
            &ChildOf,
            &ProviderKey,
            &ExternalScopeRef,
            &ResourceKind,
            &ExternalId,
        ),
        With<ExternalLink>,
    >,
    duplicates: Query<(Entity, &SessionDuplicate), With<Session>>,
    targets: Query<&EntityTarget<Session>>,
    mut commands: Commands,
) {
    if !changes.any() {
        return;
    }
    let targeted = targets
        .iter()
        .map(EntityTarget::entity)
        .collect::<HashSet<_>>();
    let mut groups = HashMap::<ExternalLinkIdentity, Vec<ProviderSessionCandidate>>::new();
    for (owner, provider, scope, kind, id) in &links {
        if kind.as_str() != "session" {
            continue;
        }
        let entity = owner.parent();
        let Ok((activated_at, stage_changed_at, archived, runtime, duplicate)) =
            sessions.get(entity)
        else {
            continue;
        };
        groups
            .entry(ExternalLinkIdentity::new(
                provider.clone(),
                scope.id().clone(),
                kind.clone(),
                id.clone(),
            ))
            .or_default()
            .push(ProviderSessionCandidate {
                entity,
                duplicate: duplicate.map(SessionDuplicate::entity),
                archived,
                active: matches!(
                    runtime,
                    Some(crate::RunState::Streaming | crate::RunState::AwaitingApproval { .. })
                ),
                targeted: targeted.contains(&entity),
                activated_at: activated_at
                    .map(|value| value.milliseconds())
                    .unwrap_or_default(),
                stage_changed_at: stage_changed_at
                    .map(|value| value.milliseconds())
                    .unwrap_or_default(),
            });
    }
    let mut expected = HashMap::<Entity, Entity>::new();
    for group in groups.into_values() {
        let Some(canonical) = group
            .iter()
            .max_by_key(|candidate| candidate.priority())
            .map(|candidate| candidate.entity)
        else {
            continue;
        };
        for candidate in group {
            if candidate.entity == canonical {
                continue;
            }
            expected.insert(candidate.entity, canonical);
            if candidate.duplicate != Some(canonical) {
                commands
                    .entity(candidate.entity)
                    .insert(SessionDuplicate(canonical));
            }
        }
    }
    for (entity, _) in &duplicates {
        if !expected.contains_key(&entity) {
            commands.entity(entity).remove::<SessionDuplicate>();
        }
    }
}

#[cfg(all(test, host))]
mod tests {
    use super::*;
    use bevy_ecs::reflect::AppTypeRegistry;
    use vmux_connector::ExternalScope;

    #[derive(Resource, Default)]
    struct Cleaned(Vec<Entity>);

    impl Cleaned {
        fn record(trigger: On<Cleanup>, mut cleaned: ResMut<Cleaned>) {
            cleaned.0.push(trigger.event_target());
        }
    }

    struct ProviderFixture;

    impl ProviderFixture {
        fn identity(kind: &str, id: &str) -> ExternalLinkIdentity {
            let provider = kind
                .split_once(':')
                .map(|(provider, _)| provider)
                .unwrap_or(kind);
            ExternalLinkIdentity::new(
                ProviderKey::new(provider).unwrap(),
                vmux_api::Id::<ExternalScope>::new(kind),
                ResourceKind::session(),
                ExternalId::new(id).unwrap(),
            )
        }
    }

    #[test]
    fn agent_clients_share_their_provider_family() {
        for id in ["codex", "codex-cli", "Codex ACP"] {
            assert_eq!(AgentFamily::of(id), AgentFamily::of("codex"));
            assert_eq!(AgentFamily::of(id).name(id), "Codex");
        }
        for id in ["claude", "claude-code", "Claude Desktop"] {
            assert_eq!(AgentFamily::of(id), AgentFamily::of("claude"));
            assert_eq!(AgentFamily::of(id).name(id), "Claude");
        }
        for id in ["vibe", "mistral-vibe", "Vibe Desktop"] {
            assert_eq!(AgentFamily::of(id), AgentFamily::of("vibe"));
            assert_eq!(AgentFamily::of(id).name(id), "Mistral Vibe");
        }
    }

    #[test]
    fn creation_uses_configured_default_workflow_state_and_common_components() {
        let mut app = App::new();
        app.add_plugins((EntityPlugin, MetadataPlugin, WorkflowStatePlugin));
        let request = CreateRequest::new(
            "Task",
            "Description",
            PathBuf::from("/tmp/project"),
            Some(AgentRef::new("codex")),
        );
        let expected_id = request.id.clone();
        app.world_mut()
            .resource_mut::<Messages<CreateRequest>>()
            .write(request);

        app.update();

        let mut sessions = app.world_mut().query_filtered::<(
            &Id<Session>,
            &Name,
            &Description,
            &Cwd,
            &WorkflowStateRef,
            &AgentRef,
        ), With<Session>>();
        let (id, name, description, cwd, state, agent) = sessions.single(app.world()).unwrap();
        assert_eq!(id, &expected_id);
        assert_eq!(name.as_str(), "Task");
        assert_eq!(description.0, "Description");
        assert_eq!(cwd.0, PathBuf::from("/tmp/project"));
        assert_eq!(state.id().as_str(), "in_progress");
        assert_eq!(agent.as_str(), "codex");
    }

    #[test]
    fn provider_import_preserves_source_time() {
        let mut app = App::new();
        app.add_plugins((EntityPlugin, WorkflowStatePlugin));
        let request = CreateRequest::imported(
            "Imported",
            "Provider session",
            PathBuf::from("/tmp/project"),
            AgentRef::new("codex"),
            100,
            200,
        );
        app.world_mut()
            .resource_mut::<Messages<CreateRequest>>()
            .write(request);

        app.update();

        let mut sessions = app
            .world_mut()
            .query_filtered::<(&CreatedAt, &LastActivatedAt), With<Session>>();
        let (created, activated) = sessions.single(app.world()).unwrap();
        assert_eq!(created.milliseconds(), 100);
        assert_eq!(activated.milliseconds(), 200);
    }

    #[test]
    fn workflow_states_are_available_after_update() {
        let mut app = App::new();
        app.add_plugins(WorkflowStatePlugin);

        app.update();
        app.update();

        let mut definitions = app
            .world_mut()
            .query_filtered::<(&Id<WorkflowState>, &Order), With<WorkflowState>>()
            .iter(app.world())
            .map(|(id, order)| (id.as_str().to_string(), order.0))
            .collect::<Vec<_>>();
        definitions.sort_by_key(|(_, order)| *order);
        assert_eq!(
            definitions,
            vec![
                ("in_progress".into(), 10),
                ("in_review".into(), 20),
                ("done".into(), 30),
                ("canceled".into(), 40),
            ]
        );
    }

    #[test]
    fn workflow_state_relationship_repairs_after_target_replacement() {
        let mut app = App::new();
        app.add_plugins(WorkflowStatePlugin);
        let state_id = Id::<WorkflowState>::new("custom");
        let original = app
            .world_mut()
            .spawn((WorkflowState, state_id.clone()))
            .id();
        let session = app
            .world_mut()
            .spawn((
                Session,
                WorkflowStateRef::new(state_id.clone()),
                WorkflowStateChangedAt::new(1),
            ))
            .id();

        app.update();

        assert_eq!(
            app.world()
                .get::<WorkflowStateTarget>(session)
                .map(WorkflowStateTarget::entity),
            Some(original)
        );

        app.world_mut().despawn(original);
        app.update();

        assert!(app.world().get::<WorkflowStateRef>(session).is_some());
        assert!(app.world().get::<WorkflowStateTarget>(session).is_none());

        let replacement = app.world_mut().spawn((WorkflowState, state_id)).id();
        app.update();

        assert_eq!(
            app.world()
                .get::<WorkflowStateTarget>(session)
                .map(WorkflowStateTarget::entity),
            Some(replacement)
        );
    }

    #[test]
    fn duplicate_workflow_state_ids_are_explicit() {
        let mut app = App::new();
        app.add_plugins(WorkflowStatePlugin);
        let id = Id::<WorkflowState>::new("duplicate");
        app.world_mut().spawn((WorkflowState, id.clone()));
        app.world_mut().spawn((WorkflowState, id.clone()));

        app.update();

        assert_eq!(
            app.world().resource::<WorkflowStateIndex>().entity(&id),
            Err(WorkflowStateLookupError::Duplicate(id))
        );
    }

    #[test]
    fn archive_is_independent_from_stage() {
        let mut app = App::new();
        app.add_plugins((EntityPlugin, WorkflowStatePlugin));
        let session = app
            .world_mut()
            .spawn((Session, Id::<Session>::new("session-1")))
            .id();

        app.world_mut()
            .resource_mut::<Messages<ArchiveRequest>>()
            .write(ArchiveRequest {
                id: Id::<Session>::new("session-1"),
                archived_at: Some(10),
            });
        app.update();
        assert_eq!(
            app.world()
                .get::<ArchivedAt>(session)
                .unwrap()
                .milliseconds(),
            10
        );

        app.world_mut()
            .resource_mut::<Messages<ArchiveRequest>>()
            .write(ArchiveRequest {
                id: Id::<Session>::new("session-1"),
                archived_at: None,
            });
        app.update();
        assert!(app.world().get::<ArchivedAt>(session).is_none());
    }

    #[test]
    fn workflow_state_change_rejects_unknown_state_and_records_transition_time() {
        let mut app = App::new();
        app.add_plugins(WorkflowStatePlugin);
        let session = app
            .world_mut()
            .spawn((
                Session,
                Id::<Session>::new("session-1"),
                WorkflowStateRef::new(Id::<WorkflowState>::new("in_progress")),
                WorkflowStateChangedAt::new(1),
            ))
            .id();
        app.update();

        app.world_mut()
            .resource_mut::<Messages<WorkflowStateChangeRequest>>()
            .write(WorkflowStateChangeRequest {
                id: Id::<Session>::new("session-1"),
                workflow_state: Id::new("missing"),
            });
        app.update();
        assert_eq!(
            app.world()
                .get::<WorkflowStateRef>(session)
                .unwrap()
                .id()
                .as_str(),
            "in_progress"
        );
        assert_eq!(
            app.world()
                .get::<WorkflowStateChangedAt>(session)
                .unwrap()
                .milliseconds(),
            1
        );

        app.world_mut()
            .resource_mut::<Messages<WorkflowStateChangeRequest>>()
            .write(WorkflowStateChangeRequest {
                id: Id::<Session>::new("session-1"),
                workflow_state: Id::new("done"),
            });
        app.update();
        assert_eq!(
            app.world()
                .get::<WorkflowStateRef>(session)
                .unwrap()
                .id()
                .as_str(),
            "done"
        );
        assert!(app.world().get::<WorkflowStateTarget>(session).is_some());
        assert!(
            app.world()
                .get::<WorkflowStateChangedAt>(session)
                .unwrap()
                .milliseconds()
                > 1
        );
    }

    #[test]
    fn provider_workflow_state_change_creates_and_updates_one_durable_override() {
        let mut app = App::new();
        app.add_plugins((EntityPlugin, WorkflowStatePlugin));
        app.update();

        app.world_mut()
            .resource_mut::<Messages<ProviderWorkflowStateChangeRequest>>()
            .write(ProviderWorkflowStateChangeRequest {
                identity: ProviderFixture::identity("codex", "provider-session"),
                workflow_state: Id::new("done"),
                source_updated_at: 10,
            });
        app.update();

        let mut overrides = app.world_mut().query_filtered::<(
            Entity,
            &ProviderKey,
            &ExternalScopeRef,
            &ExternalId,
            &WorkflowStateRef,
            &WorkflowStateChangedAt,
        ), With<ProviderWorkflowState>>();
        let (entity, provider, scope, id, state, changed_at) =
            overrides.single(app.world()).unwrap();
        assert_eq!(provider.as_str(), "codex");
        assert_eq!(scope.id().as_str(), "codex");
        assert_eq!(id.as_str(), "provider-session");
        assert_eq!(state.id().as_str(), "done");
        assert!(changed_at.milliseconds() > 0);

        app.world_mut()
            .resource_mut::<Messages<ProviderWorkflowStateChangeRequest>>()
            .write(ProviderWorkflowStateChangeRequest {
                identity: ProviderFixture::identity("codex", "provider-session"),
                workflow_state: Id::new("in_review"),
                source_updated_at: 20,
            });
        app.update();

        let mut overrides = app
            .world_mut()
            .query_filtered::<(Entity, &WorkflowStateRef), With<ProviderWorkflowState>>();
        let (updated, state) = overrides.single(app.world()).unwrap();
        assert_eq!(updated, entity);
        assert_eq!(state.id().as_str(), "in_review");

        *app.world_mut()
            .get_mut::<WorkflowStateChangedAt>(entity)
            .unwrap() = WorkflowStateChangedAt::new(1);
        app.world_mut()
            .resource_mut::<Messages<ProviderWorkflowStateChangeRequest>>()
            .write(ProviderWorkflowStateChangeRequest {
                identity: ProviderFixture::identity("codex", "provider-session"),
                workflow_state: Id::new("in_review"),
                source_updated_at: 30,
            });
        app.update();

        assert!(
            app.world()
                .get::<WorkflowStateChangedAt>(entity)
                .unwrap()
                .milliseconds()
                > 1
        );
    }

    #[test]
    fn provider_archive_change_creates_and_updates_one_durable_override() {
        let mut app = App::new();
        app.add_plugins((EntityPlugin, WorkflowStatePlugin));
        let request = ProviderArchiveRequest {
            identity: ProviderFixture::identity("acp:codex", "provider-session"),
            archived: true,
            source_updated_at: 10,
            native: false,
        };

        app.world_mut()
            .resource_mut::<Messages<ProviderArchiveRequest>>()
            .write(request.clone());
        app.update();

        let mut overrides = app.world_mut().query_filtered::<(
            Entity,
            &RemoteUpdatedAt,
            Option<&ArchivedAt>,
        ), With<ProviderArchive>>();
        let (entity, source_updated_at, archived_at) = overrides.single(app.world()).unwrap();
        assert_eq!(source_updated_at.0.0, 10);
        assert!(archived_at.is_some());

        app.world_mut()
            .resource_mut::<Messages<ProviderArchiveRequest>>()
            .write(ProviderArchiveRequest {
                archived: false,
                source_updated_at: 20,
                ..request
            });
        app.update();

        let mut overrides = app
            .world_mut()
            .query_filtered::<(Entity, &RemoteUpdatedAt, Has<ArchivedAt>), With<ProviderArchive>>();
        let (updated, source_updated_at, archived) = overrides.single(app.world()).unwrap();
        assert_eq!(updated, entity);
        assert_eq!(source_updated_at.0.0, 20);
        assert!(!archived);
    }

    #[test]
    fn native_provider_archive_persists_before_the_provider_write_finishes() {
        let mut app = App::new();
        app.add_plugins((EntityPlugin, WorkflowStatePlugin));
        app.world_mut()
            .resource_mut::<Messages<ProviderArchiveRequest>>()
            .write(ProviderArchiveRequest {
                identity: ProviderFixture::identity("acp:codex", "provider-session"),
                archived: true,
                source_updated_at: 40,
                native: true,
            });

        app.update();

        let mut overrides = app
            .world_mut()
            .query_filtered::<(&RemoteUpdatedAt, &ArchivedAt), With<ProviderArchive>>();
        let (source_updated_at, archived_at) = overrides.single(app.world()).unwrap();
        assert_eq!(source_updated_at.0.0, 40);
        assert_eq!(archived_at.milliseconds(), 40);
        let writes = app
            .world_mut()
            .resource_mut::<Messages<ProviderArchiveWriteRequest>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(writes.len(), 1);
        assert!(writes[0].archived);
    }

    #[test]
    fn duplicate_creation_requests_create_one_session() {
        let mut app = App::new();
        app.add_plugins((EntityPlugin, WorkflowStatePlugin));
        let request = CreateRequest::new("Task", "", PathBuf::new(), None);
        app.world_mut()
            .resource_mut::<Messages<CreateRequest>>()
            .write_batch([request.clone(), request]);

        app.update();

        let count = app
            .world_mut()
            .query_filtered::<Entity, With<Session>>()
            .iter(app.world())
            .count();
        assert_eq!(count, 1);
    }

    #[test]
    fn cleanup_request_targets_the_canonical_session() {
        let mut app = App::new();
        app.add_plugins((EntityPlugin, WorkflowStatePlugin))
            .init_resource::<Cleaned>()
            .add_observer(Cleaned::record);
        let session = app
            .world_mut()
            .spawn((Session, Id::<Session>::new("session-1")))
            .id();
        app.world_mut()
            .resource_mut::<Messages<CleanupRequest>>()
            .write(CleanupRequest {
                targets: vec![CleanupTarget::Session(Id::<Session>::new("session-1"))],
            });

        app.update();

        assert_eq!(app.world().resource::<Cleaned>().0, vec![session]);
        assert!(app.world().get_entity(session).is_err());
    }

    #[test]
    fn cleanup_removes_linked_provider_copies_and_can_restore_visibility() {
        let mut app = App::new();
        app.add_plugins((vmux_connector::ConnectorPlugin, EntityPlugin, StagePlugin));
        let identity = ProviderFixture::identity("acp:codex", "provider-session");
        for id in ["canonical", "duplicate"] {
            let session = app
                .world_mut()
                .spawn((Session, Id::<Session>::new(id)))
                .id();
            app.world_mut().spawn((
                ExternalLink,
                identity.provider().clone(),
                ExternalScopeRef::new(identity.scope().clone()),
                identity.kind().clone(),
                identity.external_id().clone(),
                ChildOf(session),
            ));
        }
        app.world_mut()
            .resource_mut::<Messages<CleanupRequest>>()
            .write(CleanupRequest {
                targets: vec![CleanupTarget::Session(Id::<Session>::new("canonical"))],
            });

        app.update();

        let count = app
            .world_mut()
            .query_filtered::<Entity, With<Session>>()
            .iter(app.world())
            .count();
        assert_eq!(count, 0);
        assert!(
            app.world()
                .resource::<ProviderSessionTombstones>()
                .contains(&identity)
        );

        app.world_mut()
            .resource_mut::<Messages<ProviderSessionRestoreRequest>>()
            .write(ProviderSessionRestoreRequest {
                identities: vec![identity.clone()],
            });
        app.update();

        assert!(
            !app.world()
                .resource::<ProviderSessionTombstones>()
                .contains(&identity)
        );
        let tombstones = app
            .world_mut()
            .query_filtered::<Entity, With<ProviderSessionTombstone>>()
            .iter(app.world())
            .count();
        assert_eq!(tombstones, 0);
    }

    #[test]
    fn domain_plugin_registers_its_durable_components() {
        let mut app = App::new();
        app.add_plugins(crate::DomainPlugin);
        let registry = app.world().resource::<AppTypeRegistry>().read();
        let filter = vmux_ecs::persistence::WorkspacePersisted::filter(&registry);

        assert!(filter.is_allowed::<Session>());
        assert!(filter.is_allowed::<Id<Session>>());
        assert!(filter.is_allowed::<AgentRef>());
        assert!(filter.is_allowed::<ProviderWorkflowState>());
        assert!(filter.is_allowed::<ProviderArchive>());
        assert!(filter.is_allowed::<ProviderSessionTombstone>());
        assert!(filter.is_allowed::<AgentConversationTitle>());
        assert!(filter.is_allowed::<TitlePending>());
        assert!(filter.is_allowed::<WorkflowState>());
        assert!(filter.is_allowed::<Id<WorkflowState>>());
        assert!(filter.is_allowed::<WorkflowStateRef>());
        assert!(filter.is_allowed::<WorkflowStateChangedAt>());
        assert!(filter.is_allowed::<TerminalWorkflowState>());
    }

    #[test]
    fn activating_a_session_view_updates_the_session_timestamp() {
        let mut app = App::new();
        app.add_plugins(EntityPlugin);
        let session = app
            .world_mut()
            .spawn((Session, LastActivatedAt::new(1)))
            .id();
        let stack = app
            .world_mut()
            .spawn(EntityTarget::<Session>::new(session))
            .id();

        app.world_mut().trigger(ActivateRequest { entity: stack });

        assert!(
            app.world()
                .get::<LastActivatedAt>(session)
                .unwrap()
                .milliseconds()
                > 1
        );
    }

    #[test]
    fn renamed_session_updates_view_metadata() {
        let mut app = App::new();
        app.add_plugins((EntityPlugin, MetadataPlugin, WorkflowStatePlugin));
        let session = app
            .world_mut()
            .spawn((
                Session,
                Id::<Session>::new("session-1"),
                Name::new("Before"),
            ))
            .id();
        let stack = app
            .world_mut()
            .spawn((
                EntityTarget::<Session>::new(session),
                PageMetadata {
                    url: String::new(),
                    title: String::new(),
                    icon: Default::default(),
                    bg_color: None,
                },
            ))
            .id();
        app.update();

        app.world_mut()
            .resource_mut::<Messages<RenameRequest>>()
            .write(RenameRequest {
                id: Id::<Session>::new("session-1"),
                name: "After".into(),
            });
        app.update();

        let metadata = app.world().get::<PageMetadata>(stack).unwrap();
        assert_eq!(metadata.title, "After");
        assert_eq!(metadata.url, "vmux://sessions/session-1");
        assert_eq!(
            app.world().get::<AgentConversationTitle>(session),
            Some(&AgentConversationTitle("After".into()))
        );
    }
}
