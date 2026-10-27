use std::{borrow::Borrow, path::PathBuf};

#[cfg(host)]
use bevy_app::{App, Plugin, PostUpdate, Update};
use bevy_ecs::prelude::*;
#[cfg(host)]
use bevy_ecs::system::SystemParam;
use bevy_reflect::Reflect;
use moonshine_save::prelude::Save;
use serde::{Deserialize, Serialize};
#[cfg(host)]
use std::collections::{HashMap, HashSet};
use vmux_api::session::SessionId;
#[cfg(host)]
use vmux_ecs::persistence::PersistenceAppExt;
#[cfg(host)]
use vmux_ecs::{ActivateRequest, CreatedAt, EntityTarget, LastActivatedAt, Order, PageMetadata};
use vmux_ecs::{ArchivedAt, Cwd, Description};

#[cfg(host)]
use crate::{AgentConversationTitle, TitlePending};

#[cfg(host)]
pub(crate) struct EntityPlugin;

#[cfg(host)]
impl Plugin for EntityPlugin {
    fn build(&self, app: &mut App) {
        app.register_persisted::<Session>()
            .register_persisted::<SessionId>()
            .register_persisted::<AgentId>()
            .register_persisted::<ProviderStage>()
            .register_persisted::<ProviderArchive>()
            .register_persisted::<ProviderSessionTombstone>()
            .register_persisted::<SourceUpdatedAt>()
            .register_persisted::<ProviderKind>()
            .register_persisted::<ProviderSessionId>()
            .register_persisted::<AgentConversationTitle>()
            .register_persisted::<TitlePending>()
            .register_persisted::<LocalTask>()
            .init_resource::<ProviderArchiveOperations>()
            .init_resource::<ProviderSessionTombstones>()
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
pub(crate) struct StagePlugin;

#[cfg(host)]
impl Plugin for StagePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Configuration::load())
            .register_persisted::<Stage>()
            .register_persisted::<StageChangedAt>()
            .register_type::<StageId>()
            .register_type::<StageDefinition>()
            .add_message::<StageChangeRequest>()
            .add_message::<ProviderStageChangeRequest>()
            .add_systems(
                Update,
                (ensure_stages, change_stage, change_provider_stage).in_set(MutationSet),
            );
    }
}

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
#[reflect(Component)]
#[require(Save, Cwd, Description)]
#[type_path = "vmux_session"]
pub struct Session;

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct SessionDuplicate(pub Entity);

#[derive(
    Component, Clone, Debug, Default, PartialEq, Eq, Hash, Reflect, Serialize, Deserialize,
)]
#[reflect(Component)]
#[require(Save)]
#[type_path = "vmux_session"]
pub struct AgentId(pub String);

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

#[derive(
    Component, Clone, Debug, Default, PartialEq, Eq, Hash, Reflect, Serialize, Deserialize,
)]
#[reflect(Component)]
#[type_path = "vmux_session"]
pub struct StageId(pub String);

#[derive(Component, Clone, Debug, Default, PartialEq, Eq, Reflect, Serialize, Deserialize)]
#[reflect(Component)]
#[require(Save)]
#[type_path = "vmux_session"]
pub struct Stage(pub StageId);

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
#[reflect(Component)]
#[require(Save)]
#[type_path = "vmux_session"]
pub struct StageChangedAt(pub i64);

impl StageChangedAt {
    fn record_transition(&mut self, now: i64) {
        self.0 = now.max(self.0.saturating_add(1));
    }
}

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
#[reflect(Component)]
#[require(Save)]
#[type_path = "vmux_session"]
pub struct LocalTask;

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
#[reflect(Component)]
#[require(Save, SourceUpdatedAt)]
#[type_path = "vmux_session"]
pub struct ProviderStage;

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
#[reflect(Component)]
#[require(Save, SourceUpdatedAt)]
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
pub struct SourceUpdatedAt(pub i64);

#[derive(
    Component, Clone, Debug, Default, PartialEq, Eq, Hash, Reflect, Serialize, Deserialize,
)]
#[reflect(Component)]
#[require(Save)]
#[type_path = "vmux_session"]
pub struct ProviderKind(pub String);

impl Borrow<str> for ProviderKind {
    fn borrow(&self) -> &str {
        &self.0
    }
}

#[derive(
    Component, Clone, Debug, Default, PartialEq, Eq, Hash, Reflect, Serialize, Deserialize,
)]
#[reflect(Component)]
#[require(Save)]
#[type_path = "vmux_session"]
pub struct ProviderSessionId(pub String);

impl Borrow<str> for ProviderSessionId {
    fn borrow(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct ProviderSessionKey {
    pub kind: ProviderKind,
    pub id: ProviderSessionId,
}

impl ProviderSessionKey {
    pub fn new(kind: ProviderKind, id: ProviderSessionId) -> Self {
        Self { kind, id }
    }

    pub fn from_components(kind: &ProviderKind, id: &ProviderSessionId) -> Self {
        Self::new(kind.clone(), id.clone())
    }

    pub fn matches(&self, kind: &str, id: &str) -> bool {
        self.kind.0 == kind && self.id.0 == id
    }
}

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
#[reflect(Component)]
#[type_path = "vmux_session"]
pub struct StageDefinition;

#[derive(Message, Clone, Debug)]
pub struct CreateRequest {
    id: SessionId,
    pub name: String,
    pub description: String,
    pub cwd: PathBuf,
    pub agent: Option<AgentId>,
    created_at: Option<i64>,
    activated_at: Option<i64>,
    local: bool,
}

impl CreateRequest {
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        cwd: PathBuf,
        agent: Option<AgentId>,
    ) -> Self {
        Self {
            id: SessionId(uuid::Uuid::new_v4().to_string()),
            name: name.into(),
            description: description.into(),
            cwd,
            agent,
            created_at: None,
            activated_at: None,
            local: true,
        }
    }

    pub fn imported(
        name: impl Into<String>,
        description: impl Into<String>,
        cwd: PathBuf,
        agent: AgentId,
        created_at: i64,
        activated_at: i64,
    ) -> Self {
        Self {
            id: SessionId(uuid::Uuid::new_v4().to_string()),
            name: name.into(),
            description: description.into(),
            cwd,
            agent: Some(agent),
            created_at: Some(created_at),
            activated_at: Some(activated_at),
            local: false,
        }
    }

    pub fn id(&self) -> &SessionId {
        &self.id
    }
}

#[derive(EntityEvent, Clone, Debug)]
pub struct Created {
    #[event_target]
    pub entity: Entity,
    pub id: SessionId,
}

#[derive(Message, Clone, Debug)]
pub struct RenameRequest {
    pub id: SessionId,
    pub name: String,
}

#[derive(Message, Clone, Debug)]
pub struct DescriptionUpdateRequest {
    pub id: SessionId,
    pub description: String,
}

#[derive(Message, Clone, Debug)]
pub struct StageChangeRequest {
    pub id: SessionId,
    pub stage: StageId,
}

#[derive(Message, Clone, Debug)]
pub struct ArchiveRequest {
    pub id: SessionId,
    pub archived_at: Option<i64>,
}

#[derive(Message, Clone, Debug)]
pub struct ProviderArchiveRequest {
    pub kind: ProviderKind,
    pub id: ProviderSessionId,
    pub archived: bool,
    pub source_updated_at: i64,
    pub native: bool,
}

#[derive(Message, Clone, Debug)]
pub struct ProviderArchiveWriteRequest {
    pub kind: ProviderKind,
    pub id: ProviderSessionId,
    pub archived: bool,
    pub revision: u64,
}

#[derive(Message, Clone, Debug)]
pub struct ProviderArchiveCompleted {
    pub kind: ProviderKind,
    pub id: ProviderSessionId,
    pub archived: bool,
    pub revision: u64,
    pub success: bool,
}

#[derive(Message, Clone, Debug)]
pub struct ProviderSessionRestoreRequest {
    pub keys: Vec<ProviderSessionKey>,
}

#[derive(Message, Clone, Debug)]
pub struct ProviderStageChangeRequest {
    pub kind: ProviderKind,
    pub id: ProviderSessionId,
    pub stage: StageId,
    pub source_updated_at: i64,
}

#[derive(Message, Clone, Debug)]
pub struct CleanupRequest {
    pub targets: Vec<CleanupTarget>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum CleanupTarget {
    Session(SessionId),
    Provider(ProviderSessionKey),
}

#[cfg(host)]
#[derive(Resource, Default)]
pub(crate) struct ProviderArchiveOperations {
    next_revision: u64,
    pending: HashMap<ProviderSessionKey, ProviderArchiveOperation>,
}

#[cfg(host)]
struct ProviderArchiveOperation {
    archived: bool,
    source_updated_at: i64,
    revision: u64,
    in_flight: Option<u64>,
}

#[cfg(host)]
impl ProviderArchiveOperations {
    fn request(&mut self, request: &ProviderArchiveRequest) -> Option<ProviderArchiveWriteRequest> {
        self.next_revision = self.next_revision.wrapping_add(1).max(1);
        let revision = self.next_revision;
        let key = ProviderSessionKey::from_components(&request.kind, &request.id);
        let operation = self.pending.entry(key).or_insert(ProviderArchiveOperation {
            archived: request.archived,
            source_updated_at: request.source_updated_at,
            revision,
            in_flight: None,
        });
        operation.archived = request.archived;
        operation.source_updated_at = request.source_updated_at;
        operation.revision = revision;
        if operation.in_flight.is_some() {
            return None;
        }
        operation.in_flight = Some(revision);
        Some(ProviderArchiveWriteRequest {
            kind: request.kind.clone(),
            id: request.id.clone(),
            archived: request.archived,
            revision,
        })
    }

    fn complete(
        &mut self,
        result: &ProviderArchiveCompleted,
    ) -> Option<ProviderArchiveWriteRequest> {
        let key = ProviderSessionKey::from_components(&result.kind, &result.id);
        let operation = self.pending.get_mut(&key)?;
        if operation.in_flight != Some(result.revision) {
            return None;
        }
        if operation.revision == result.revision {
            self.pending.remove(&key);
            return None;
        }
        operation.in_flight = Some(operation.revision);
        Some(ProviderArchiveWriteRequest {
            kind: result.kind.clone(),
            id: result.id.clone(),
            archived: operation.archived,
            revision: operation.revision,
        })
    }

    pub(crate) fn archived_at(&self, key: &ProviderSessionKey) -> Option<i64> {
        self.pending.get(key).map(|operation| {
            if operation.archived {
                operation.source_updated_at.max(1)
            } else {
                0
            }
        })
    }
}

#[cfg(host)]
#[derive(Resource, Default)]
pub struct ProviderSessionTombstones {
    entries: HashMap<ProviderKind, HashMap<ProviderSessionId, Option<Entity>>>,
}

#[cfg(host)]
impl ProviderSessionTombstones {
    pub fn new(keys: impl IntoIterator<Item = ProviderSessionKey>) -> Self {
        let mut tombstones = Self::default();
        for key in keys {
            tombstones
                .entries
                .entry(key.kind)
                .or_default()
                .insert(key.id, None);
        }
        tombstones
    }

    pub fn contains(&self, kind: &str, id: &str) -> bool {
        self.entries
            .get(kind)
            .is_some_and(|entries| entries.contains_key(id))
    }

    fn insert(&mut self, key: ProviderSessionKey, entity: Entity) {
        self.entries
            .entry(key.kind)
            .or_default()
            .insert(key.id, Some(entity));
    }

    fn remove(&mut self, key: &ProviderSessionKey) -> Option<Entity> {
        let entries = self.entries.get_mut(key.kind.0.as_str())?;
        let entity = entries.remove(key.id.0.as_str()).flatten();
        let remove_kind = entries.is_empty();
        if remove_kind {
            self.entries.remove(key.kind.0.as_str());
        }
        entity
    }

    fn replace(
        &mut self,
        entries: HashMap<ProviderKind, HashMap<ProviderSessionId, Option<Entity>>>,
    ) {
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

    fn dismiss(&mut self, keys: impl IntoIterator<Item = ProviderSessionKey>) {
        for key in keys {
            if self.index.contains(&key.kind.0, &key.id.0) {
                continue;
            }
            let entity = self
                .commands
                .spawn((ProviderSessionTombstone, key.kind.clone(), key.id.clone()))
                .id();
            self.index.insert(key, entity);
        }
    }

    fn restore(&mut self, keys: impl IntoIterator<Item = ProviderSessionKey>) {
        for key in keys {
            if let Some(entity) = self.index.remove(&key) {
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
    default_stage: StageId,
    stages: Vec<StageSeed>,
}

#[cfg(host)]
impl Configuration {
    fn load() -> Self {
        let file: FeatureFile = ron::from_str(include_str!("feature.ron"))
            .expect("embedded Session configuration must be valid RON");
        file.session.validate();
        Self {
            default_stage: StageId(file.session.default_stage),
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
fn ensure_stages(
    configuration: Res<Configuration>,
    definitions: Query<
        (Entity, &StageId, &Name, &Order, Has<vmux_ecs::Terminal>),
        With<StageDefinition>,
    >,
    mut commands: Commands,
) {
    for stage in &configuration.stages {
        if definitions.iter().any(|(_, id, _, _, _)| id.0 == stage.id) {
            continue;
        }
        let mut entity = commands.spawn((
            StageDefinition,
            StageId(stage.id.clone()),
            Name::new(stage.name.clone()),
            Order(stage.order),
        ));
        if stage.terminal {
            entity.insert(vmux_ecs::Terminal);
        }
    }
}

#[cfg(host)]
fn create(
    mut requests: MessageReader<CreateRequest>,
    configuration: Res<Configuration>,
    existing: Query<&SessionId, With<Session>>,
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
        let now = vmux_ecs::UnixMillis::now().0;
        let created_at = request.created_at.unwrap_or(now);
        let activated_at = request.activated_at.unwrap_or(now);
        let entity = {
            let mut session = commands.spawn((
                Session,
                request.id.clone(),
                Name::new(request.name.clone()),
                Description(request.description.clone()),
                Cwd(request.cwd.clone()),
                CreatedAt(created_at),
                LastActivatedAt(activated_at),
                Stage(configuration.default_stage.clone()),
                StageChangedAt(activated_at),
            ));
            if request.local {
                session.insert(LocalTask);
            }
            if let Some(agent) = request.agent.clone() {
                session.insert(agent);
            }
            session.id()
        };
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
    sessions: Query<(&SessionId, Ref<Name>), With<Session>>,
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
            &SessionId,
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
    mut sessions: Query<(&SessionId, &mut Description), With<Session>>,
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
fn change_stage(
    mut requests: MessageReader<StageChangeRequest>,
    definitions: Query<&StageId, With<StageDefinition>>,
    mut sessions: Query<(&SessionId, &mut Stage, &mut StageChangedAt), With<Session>>,
) {
    for request in requests.read() {
        if !definitions.iter().any(|id| id == &request.stage) {
            continue;
        }
        for (id, mut stage, mut changed_at) in &mut sessions {
            if id == &request.id && stage.0 != request.stage {
                stage.0.clone_from(&request.stage);
                changed_at.record_transition(vmux_ecs::UnixMillis::now().0);
            }
        }
    }
}

#[cfg(host)]
fn archive(
    mut requests: MessageReader<ArchiveRequest>,
    sessions: Query<(Entity, &SessionId, Option<&ArchivedAt>), With<Session>>,
    mut commands: Commands,
) {
    for request in requests.read() {
        for (entity, id, current) in &sessions {
            if id != &request.id {
                continue;
            }
            if let Some(archived_at) = request.archived_at.map(|value| value.max(1)) {
                if current.is_none_or(|current| current.0 != archived_at) {
                    commands.entity(entity).insert(ArchivedAt(archived_at));
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
    mut operations: ResMut<ProviderArchiveOperations>,
    mut overrides: Query<
        (
            Entity,
            &ProviderKind,
            &ProviderSessionId,
            &mut SourceUpdatedAt,
            Option<&ArchivedAt>,
        ),
        With<ProviderArchive>,
    >,
    mut writes: MessageWriter<ProviderArchiveWriteRequest>,
    mut commands: Commands,
) {
    let mut latest = HashMap::<ProviderSessionKey, ProviderArchiveRequest>::new();
    for request in requests.read().cloned() {
        latest.insert(
            ProviderSessionKey::from_components(&request.kind, &request.id),
            request,
        );
    }
    for request in latest.into_values() {
        let next_archived_at = request.source_updated_at.max(1);
        let mut found = false;
        for (entity, kind, id, mut source_updated_at, current_archived_at) in &mut overrides {
            if kind != &request.kind || id != &request.id {
                continue;
            }
            source_updated_at.0 = request.source_updated_at;
            if request.archived && current_archived_at.is_none() {
                commands.entity(entity).insert(ArchivedAt(next_archived_at));
            } else if !request.archived && current_archived_at.is_some() {
                commands.entity(entity).remove::<ArchivedAt>();
            }
            found = true;
            break;
        }
        if found {
            if request.native
                && let Some(write) = operations.request(&request)
            {
                writes.write(write);
            }
            continue;
        }
        let mut entity = commands.spawn((
            ProviderArchive,
            request.kind.clone(),
            request.id.clone(),
            SourceUpdatedAt(request.source_updated_at),
        ));
        if request.archived {
            entity.insert(ArchivedAt(next_archived_at));
        }
        if request.native
            && let Some(write) = operations.request(&request)
        {
            writes.write(write);
        }
    }
}

#[cfg(host)]
fn finish_provider_archives(
    mut completed: MessageReader<ProviderArchiveCompleted>,
    mut operations: ResMut<ProviderArchiveOperations>,
    mut writes: MessageWriter<ProviderArchiveWriteRequest>,
) {
    for result in completed.read() {
        if let Some(write) = operations.complete(result) {
            writes.write(write);
        }
    }
}

#[cfg(host)]
fn sync_tombstones(
    tombstones: Query<(Entity, &ProviderKind, &ProviderSessionId), With<ProviderSessionTombstone>>,
    changed: Query<
        (),
        (
            With<ProviderSessionTombstone>,
            Or<(
                Added<ProviderSessionTombstone>,
                Changed<ProviderKind>,
                Changed<ProviderSessionId>,
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
    let mut entries = HashMap::<ProviderKind, HashMap<ProviderSessionId, Option<Entity>>>::new();
    for (entity, kind, id) in &tombstones {
        entries
            .entry(kind.clone())
            .or_default()
            .insert(id.clone(), Some(entity));
    }
    index.replace(entries);
}

#[cfg(host)]
fn restore_tombstones(
    mut requests: MessageReader<ProviderSessionRestoreRequest>,
    mut tombstones: TombstoneMutations,
) {
    for request in requests.read() {
        tombstones.restore(request.keys.iter().cloned());
    }
}

#[cfg(host)]
fn change_provider_stage(
    mut requests: MessageReader<ProviderStageChangeRequest>,
    definitions: Query<&StageId, With<StageDefinition>>,
    mut stages: Query<
        (
            &ProviderKind,
            &ProviderSessionId,
            &mut Stage,
            &mut StageChangedAt,
            &mut SourceUpdatedAt,
        ),
        With<ProviderStage>,
    >,
    mut commands: Commands,
) {
    for request in requests.read() {
        if !definitions.iter().any(|id| id == &request.stage) {
            continue;
        }
        let now = vmux_ecs::UnixMillis::now().0;
        let mut found = false;
        for (kind, id, mut stage, mut changed_at, mut source_updated_at) in &mut stages {
            if kind != &request.kind || id != &request.id {
                continue;
            }
            found = true;
            if stage.0 != request.stage {
                stage.0.clone_from(&request.stage);
            }
            changed_at.record_transition(now);
            source_updated_at.0 = request.source_updated_at;
            break;
        }
        if !found {
            commands.spawn((
                ProviderStage,
                request.kind.clone(),
                request.id.clone(),
                Stage(request.stage.clone()),
                StageChangedAt(now),
                SourceUpdatedAt(request.source_updated_at),
            ));
        }
    }
}

#[cfg(host)]
fn cleanup(
    mut requests: MessageReader<CleanupRequest>,
    sessions: Query<
        (
            Entity,
            &SessionId,
            Option<&ProviderKind>,
            Option<&ProviderSessionId>,
        ),
        With<Session>,
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
    let sessions = sessions
        .iter()
        .map(|(entity, id, kind, provider_id)| {
            (
                entity,
                id.clone(),
                kind.zip(provider_id)
                    .map(|(kind, id)| ProviderSessionKey::from_components(kind, id)),
            )
        })
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
                Changed<ProviderKind>,
                Changed<ProviderSessionId>,
                Changed<LastActivatedAt>,
                Changed<StageChangedAt>,
                Changed<ArchivedAt>,
                Changed<crate::RunState>,
            )>,
        ),
    >,
    targets: Query<'w, 's, (), Or<(Added<EntityTarget<Session>>, Changed<EntityTarget<Session>>)>>,
    removed_sessions: RemovedComponents<'w, 's, Session>,
    removed_provider_kinds: RemovedComponents<'w, 's, ProviderKind>,
    removed_provider_ids: RemovedComponents<'w, 's, ProviderSessionId>,
    removed_activated_at: RemovedComponents<'w, 's, LastActivatedAt>,
    removed_stage_changed_at: RemovedComponents<'w, 's, StageChangedAt>,
    removed_archived_at: RemovedComponents<'w, 's, ArchivedAt>,
    removed_run_state: RemovedComponents<'w, 's, crate::RunState>,
    removed_targets: RemovedComponents<'w, 's, EntityTarget<Session>>,
}

#[cfg(host)]
impl CanonicalizationChanges<'_, '_> {
    fn any(&mut self) -> bool {
        !self.sessions.is_empty()
            || !self.targets.is_empty()
            || self.removed_sessions.read().count() > 0
            || self.removed_provider_kinds.read().count() > 0
            || self.removed_provider_ids.read().count() > 0
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
            Entity,
            &ProviderKind,
            &ProviderSessionId,
            Option<&LastActivatedAt>,
            Option<&StageChangedAt>,
            Has<ArchivedAt>,
            Option<&crate::RunState>,
            Option<&SessionDuplicate>,
        ),
        With<Session>,
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
    let mut groups = HashMap::<ProviderSessionKey, Vec<ProviderSessionCandidate>>::new();
    for (entity, kind, id, activated_at, stage_changed_at, archived, runtime, duplicate) in
        &sessions
    {
        if kind.0.is_empty() || id.0.is_empty() {
            continue;
        }
        groups
            .entry(ProviderSessionKey::from_components(kind, id))
            .or_default()
            .push(ProviderSessionCandidate {
                entity,
                duplicate: duplicate.map(|duplicate| duplicate.0),
                archived,
                active: matches!(
                    runtime,
                    Some(crate::RunState::Streaming | crate::RunState::AwaitingApproval { .. })
                ),
                targeted: targeted.contains(&entity),
                activated_at: activated_at.map(|value| value.0).unwrap_or_default(),
                stage_changed_at: stage_changed_at.map(|value| value.0).unwrap_or_default(),
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

    #[derive(Resource, Default)]
    struct Cleaned(Vec<Entity>);

    impl Cleaned {
        fn record(trigger: On<Cleanup>, mut cleaned: ResMut<Cleaned>) {
            cleaned.0.push(trigger.event_target());
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
    fn creation_uses_configured_default_stage_and_common_components() {
        let mut app = App::new();
        app.add_plugins((EntityPlugin, MetadataPlugin, StagePlugin));
        let request = CreateRequest::new(
            "Task",
            "Description",
            PathBuf::from("/tmp/project"),
            Some(AgentId("codex".into())),
        );
        let expected_id = request.id.clone();
        app.world_mut()
            .resource_mut::<Messages<CreateRequest>>()
            .write(request);

        app.update();

        let mut sessions = app.world_mut().query_filtered::<(
            &SessionId,
            &Name,
            &Description,
            &Cwd,
            &Stage,
            &AgentId,
        ), With<Session>>();
        let (id, name, description, cwd, stage, agent) = sessions.single(app.world()).unwrap();
        assert_eq!(id, &expected_id);
        assert_eq!(name.as_str(), "Task");
        assert_eq!(description.0, "Description");
        assert_eq!(cwd.0, PathBuf::from("/tmp/project"));
        assert_eq!(stage.0.0, "in_progress");
        assert_eq!(agent.0, "codex");
    }

    #[test]
    fn provider_import_preserves_time_without_marking_a_local_task() {
        let mut app = App::new();
        app.add_plugins((EntityPlugin, StagePlugin));
        let request = CreateRequest::imported(
            "Imported",
            "Provider session",
            PathBuf::from("/tmp/project"),
            AgentId("codex".into()),
            100,
            200,
        );
        app.world_mut()
            .resource_mut::<Messages<CreateRequest>>()
            .write(request);

        app.update();

        let mut sessions = app
            .world_mut()
            .query_filtered::<(&CreatedAt, &LastActivatedAt, Has<LocalTask>), With<Session>>();
        let (created, activated, local) = sessions.single(app.world()).unwrap();
        assert_eq!(created.0, 100);
        assert_eq!(activated.0, 200);
        assert!(!local);
    }

    #[test]
    fn stage_definitions_are_available_after_update() {
        let mut app = App::new();
        app.add_plugins(StagePlugin);

        app.update();
        app.update();

        let mut definitions = app
            .world_mut()
            .query_filtered::<(&StageId, &Order), With<StageDefinition>>()
            .iter(app.world())
            .map(|(id, order)| (id.0.clone(), order.0))
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
    fn archive_is_independent_from_stage() {
        let mut app = App::new();
        app.add_plugins((EntityPlugin, StagePlugin));
        let session = app
            .world_mut()
            .spawn((Session, SessionId("session-1".into())))
            .id();

        app.world_mut()
            .resource_mut::<Messages<ArchiveRequest>>()
            .write(ArchiveRequest {
                id: SessionId("session-1".into()),
                archived_at: Some(10),
            });
        app.update();
        assert_eq!(app.world().get::<ArchivedAt>(session).unwrap().0, 10);

        app.world_mut()
            .resource_mut::<Messages<ArchiveRequest>>()
            .write(ArchiveRequest {
                id: SessionId("session-1".into()),
                archived_at: None,
            });
        app.update();
        assert!(app.world().get::<ArchivedAt>(session).is_none());
    }

    #[test]
    fn stage_change_rejects_unknown_stage_and_records_transition_time() {
        let mut app = App::new();
        app.add_plugins(StagePlugin);
        let session = app
            .world_mut()
            .spawn((
                Session,
                SessionId("session-1".into()),
                Stage(StageId("in_progress".into())),
                StageChangedAt(1),
            ))
            .id();
        app.update();

        app.world_mut()
            .resource_mut::<Messages<StageChangeRequest>>()
            .write(StageChangeRequest {
                id: SessionId("session-1".into()),
                stage: StageId("missing".into()),
            });
        app.update();
        assert_eq!(
            app.world().get::<Stage>(session).unwrap().0.0,
            "in_progress"
        );
        assert_eq!(app.world().get::<StageChangedAt>(session).unwrap().0, 1);

        app.world_mut()
            .resource_mut::<Messages<StageChangeRequest>>()
            .write(StageChangeRequest {
                id: SessionId("session-1".into()),
                stage: StageId("done".into()),
            });
        app.update();
        assert_eq!(app.world().get::<Stage>(session).unwrap().0.0, "done");
        assert!(app.world().get::<StageChangedAt>(session).unwrap().0 > 1);
    }

    #[test]
    fn provider_stage_change_creates_and_updates_one_durable_override() {
        let mut app = App::new();
        app.add_plugins((EntityPlugin, StagePlugin));
        app.update();

        app.world_mut()
            .resource_mut::<Messages<ProviderStageChangeRequest>>()
            .write(ProviderStageChangeRequest {
                kind: ProviderKind("codex".into()),
                id: ProviderSessionId("provider-session".into()),
                stage: StageId("done".into()),
                source_updated_at: 10,
            });
        app.update();

        let mut overrides = app.world_mut().query_filtered::<(
            Entity,
            &ProviderKind,
            &ProviderSessionId,
            &Stage,
            &StageChangedAt,
        ), With<ProviderStage>>();
        let (entity, kind, id, stage, changed_at) = overrides.single(app.world()).unwrap();
        assert_eq!(kind.0, "codex");
        assert_eq!(id.0, "provider-session");
        assert_eq!(stage.0.0, "done");
        assert!(changed_at.0 > 0);

        app.world_mut()
            .resource_mut::<Messages<ProviderStageChangeRequest>>()
            .write(ProviderStageChangeRequest {
                kind: ProviderKind("codex".into()),
                id: ProviderSessionId("provider-session".into()),
                stage: StageId("in_review".into()),
                source_updated_at: 20,
            });
        app.update();

        let mut overrides = app
            .world_mut()
            .query_filtered::<(Entity, &Stage), With<ProviderStage>>();
        let (updated, stage) = overrides.single(app.world()).unwrap();
        assert_eq!(updated, entity);
        assert_eq!(stage.0.0, "in_review");

        app.world_mut().get_mut::<StageChangedAt>(entity).unwrap().0 = 1;
        app.world_mut()
            .resource_mut::<Messages<ProviderStageChangeRequest>>()
            .write(ProviderStageChangeRequest {
                kind: ProviderKind("codex".into()),
                id: ProviderSessionId("provider-session".into()),
                stage: StageId("in_review".into()),
                source_updated_at: 30,
            });
        app.update();

        assert!(app.world().get::<StageChangedAt>(entity).unwrap().0 > 1);
    }

    #[test]
    fn provider_archive_change_creates_and_updates_one_durable_override() {
        let mut app = App::new();
        app.add_plugins((EntityPlugin, StagePlugin));
        let request = ProviderArchiveRequest {
            kind: ProviderKind("acp:codex".into()),
            id: ProviderSessionId("provider-session".into()),
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
            &SourceUpdatedAt,
            Option<&ArchivedAt>,
        ), With<ProviderArchive>>();
        let (entity, source_updated_at, archived_at) = overrides.single(app.world()).unwrap();
        assert_eq!(source_updated_at.0, 10);
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
            .query_filtered::<(Entity, &SourceUpdatedAt, Has<ArchivedAt>), With<ProviderArchive>>();
        let (updated, source_updated_at, archived) = overrides.single(app.world()).unwrap();
        assert_eq!(updated, entity);
        assert_eq!(source_updated_at.0, 20);
        assert!(!archived);
    }

    #[test]
    fn native_provider_archive_persists_before_the_provider_write_finishes() {
        let mut app = App::new();
        app.add_plugins((EntityPlugin, StagePlugin));
        app.world_mut()
            .resource_mut::<Messages<ProviderArchiveRequest>>()
            .write(ProviderArchiveRequest {
                kind: ProviderKind("acp:codex".into()),
                id: ProviderSessionId("provider-session".into()),
                archived: true,
                source_updated_at: 40,
                native: true,
            });

        app.update();

        let mut overrides = app
            .world_mut()
            .query_filtered::<(&SourceUpdatedAt, &ArchivedAt), With<ProviderArchive>>();
        let (source_updated_at, archived_at) = overrides.single(app.world()).unwrap();
        assert_eq!(source_updated_at.0, 40);
        assert_eq!(archived_at.0, 40);
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
        app.add_plugins((EntityPlugin, StagePlugin));
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
        app.add_plugins((EntityPlugin, StagePlugin))
            .init_resource::<Cleaned>()
            .add_observer(Cleaned::record);
        let session = app
            .world_mut()
            .spawn((Session, SessionId("session-1".into())))
            .id();
        app.world_mut()
            .resource_mut::<Messages<CleanupRequest>>()
            .write(CleanupRequest {
                targets: vec![CleanupTarget::Session(SessionId("session-1".into()))],
            });

        app.update();

        assert_eq!(app.world().resource::<Cleaned>().0, vec![session]);
        assert!(app.world().get_entity(session).is_err());
    }

    #[test]
    fn cleanup_removes_linked_provider_copies_and_can_restore_visibility() {
        let mut app = App::new();
        app.add_plugins((EntityPlugin, StagePlugin));
        for id in ["canonical", "duplicate"] {
            app.world_mut().spawn((
                Session,
                SessionId(id.into()),
                ProviderKind("acp:codex".into()),
                ProviderSessionId("provider-session".into()),
            ));
        }
        let key = ProviderSessionKey::new(
            ProviderKind("acp:codex".into()),
            ProviderSessionId("provider-session".into()),
        );
        app.world_mut()
            .resource_mut::<Messages<CleanupRequest>>()
            .write(CleanupRequest {
                targets: vec![CleanupTarget::Session(SessionId("canonical".into()))],
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
                .contains(&key.kind.0, &key.id.0)
        );

        app.world_mut()
            .resource_mut::<Messages<ProviderSessionRestoreRequest>>()
            .write(ProviderSessionRestoreRequest {
                keys: vec![key.clone()],
            });
        app.update();

        assert!(
            !app.world()
                .resource::<ProviderSessionTombstones>()
                .contains(&key.kind.0, &key.id.0)
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
        assert!(filter.is_allowed::<SessionId>());
        assert!(filter.is_allowed::<AgentId>());
        assert!(filter.is_allowed::<ProviderStage>());
        assert!(filter.is_allowed::<ProviderArchive>());
        assert!(filter.is_allowed::<ProviderSessionTombstone>());
        assert!(filter.is_allowed::<SourceUpdatedAt>());
        assert!(filter.is_allowed::<ProviderKind>());
        assert!(filter.is_allowed::<ProviderSessionId>());
        assert!(filter.is_allowed::<AgentConversationTitle>());
        assert!(filter.is_allowed::<TitlePending>());
        assert!(filter.is_allowed::<LocalTask>());
        assert!(filter.is_allowed::<Stage>());
        assert!(filter.is_allowed::<StageChangedAt>());
    }

    #[test]
    fn activating_a_session_view_updates_the_session_timestamp() {
        let mut app = App::new();
        app.add_plugins(EntityPlugin);
        let session = app.world_mut().spawn((Session, LastActivatedAt(1))).id();
        let stack = app
            .world_mut()
            .spawn(EntityTarget::<Session>::new(session))
            .id();

        app.world_mut().trigger(ActivateRequest { entity: stack });

        assert!(app.world().get::<LastActivatedAt>(session).unwrap().0 > 1);
    }

    #[test]
    fn renamed_session_updates_view_metadata() {
        let mut app = App::new();
        app.add_plugins((EntityPlugin, MetadataPlugin, StagePlugin));
        let session = app
            .world_mut()
            .spawn((Session, SessionId("session-1".into()), Name::new("Before")))
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
                id: SessionId("session-1".into()),
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
