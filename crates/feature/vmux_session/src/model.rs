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
#[cfg(test)]
use vmux_ecs::CreatedAt;
#[cfg(host)]
use vmux_ecs::store::StoreAppExt;
#[cfg(host)]
use vmux_ecs::{
    ActivateRequest, EntityTarget, ExternalIdentityIndex, ExternalIdentityIndexPlugin,
    ExternalIdentityIndexSet, LastActivatedAt, Order, PageMetadata,
};
use vmux_ecs::{ArchivedAt, Cwd, Description};

#[cfg(host)]
use crate::{AgentConversationTitle, TitlePending};

#[cfg(host)]
pub(crate) struct EntityPlugin;

#[cfg(host)]
impl Plugin for EntityPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<crate::mutation::MutationPlugin>() {
            app.add_plugins(crate::mutation::MutationPlugin);
        }
        app.add_plugins(ExternalIdentityIndexPlugin::<
            ProviderKind,
            ProviderSessionId,
            ProviderArchive,
        >::default())
            .configure_sets(
                Update,
                ExternalIdentityIndexSet::of::<ProviderKind, ProviderSessionId, ProviderArchive>()
                    .before(crate::mutation::ReceiveSet),
            )
            .register_stored::<Session>()
            .register_stored::<SessionId>()
            .register_stored::<AgentId>()
            .register_stored::<ProviderStage>()
            .register_stored::<ProviderArchive>()
            .register_stored::<ProviderSessionTombstone>()
            .register_stored::<SourceUpdatedAt>()
            .register_stored::<ProviderKind>()
            .register_stored::<ProviderSessionId>()
            .register_stored::<AgentConversationTitle>()
            .register_stored::<TitlePending>()
            .register_stored::<LocalTask>()
            .init_resource::<ArchiveSequence>()
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
                    .in_set(crate::mutation::ReceiveSet),
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
        if !app.is_plugin_added::<crate::mutation::MutationPlugin>() {
            app.add_plugins(crate::mutation::MutationPlugin);
        }
        app.add_message::<RenameRequest>()
            .add_message::<DescriptionUpdateRequest>()
            .add_systems(
                Update,
                (sync_conversation_title, rename, update_description)
                    .chain()
                    .in_set(crate::mutation::ReceiveSet),
            )
            .add_systems(Update, project_views.after(MutationSet));
    }
}

#[cfg(host)]
pub(crate) struct StagePlugin;

#[cfg(host)]
impl Plugin for StagePlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<crate::mutation::MutationPlugin>() {
            app.add_plugins(crate::mutation::MutationPlugin);
        }
        app.add_plugins(ExternalIdentityIndexPlugin::<
            ProviderKind,
            ProviderSessionId,
            ProviderStage,
        >::default())
            .configure_sets(
                Update,
                ExternalIdentityIndexSet::of::<ProviderKind, ProviderSessionId, ProviderStage>()
                    .before(crate::mutation::ReceiveSet),
            )
            .insert_resource(Configuration::load())
            .register_stored::<Stage>()
            .register_stored::<StageChangedAt>()
            .register_type::<StageId>()
            .register_type::<StageDefinition>()
            .add_message::<StageChangeRequest>()
            .add_message::<ProviderStageChangeRequest>()
            .add_systems(
                Update,
                (ensure_stages, change_stage, change_provider_stage)
                    .in_set(crate::mutation::ReceiveSet),
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
    operation: Option<Entity>,
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
            operation: None,
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
            operation: None,
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

    pub(crate) fn leased(mut self, operation: Entity) -> Self {
        self.operation = Some(operation);
        self
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
struct ArchiveSequence(u64);

#[cfg(host)]
impl ArchiveSequence {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(1).max(1);
        self.0
    }
}

#[cfg(host)]
#[derive(Component)]
struct ArchiveWritePending {
    revision: u64,
    in_flight: u64,
}

#[cfg(host)]
#[derive(SystemParam)]
pub(crate) struct ArchiveWrites<'w, 's> {
    pending: Query<
        'w,
        's,
        (
            &'static ProviderKind,
            &'static ProviderSessionId,
            &'static SourceUpdatedAt,
            Has<ArchivedAt>,
        ),
        (With<ProviderArchive>, With<ArchiveWritePending>),
    >,
    changed: Query<
        'w,
        's,
        (),
        (
            With<ProviderArchive>,
            With<ArchiveWritePending>,
            Or<(
                Changed<ArchiveWritePending>,
                Changed<SourceUpdatedAt>,
                Changed<ArchivedAt>,
            )>,
        ),
    >,
    removed: RemovedComponents<'w, 's, ArchiveWritePending>,
}

#[cfg(host)]
impl ArchiveWrites<'_, '_> {
    pub(crate) fn is_changed(&mut self) -> bool {
        let removed = self.removed.read().count() > 0;
        !self.changed.is_empty() || removed
    }

    pub(crate) fn archived_at(&self, key: &ProviderSessionKey) -> Option<i64> {
        let mut selected = None;
        for (kind, id, updated, archived) in &self.pending {
            if kind != &key.kind || id != &key.id {
                continue;
            }
            if selected.is_some() {
                return None;
            }
            selected = Some(if archived { updated.0.max(1) } else { 0 });
        }
        selected
    }
}

#[cfg(host)]
#[derive(Resource, Default)]
pub struct ProviderSessionTombstones {
    entries: HashMap<ProviderKind, HashMap<ProviderSessionId, Option<Entity>>>,
}

#[cfg(host)]
impl ProviderSessionTombstones {
    pub fn contains_family(&self, family: &crate::AgentFamily, id: &str) -> bool {
        for (kind, entries) in &self.entries {
            if crate::AgentFamily::of(kind.0.strip_prefix("acp:").unwrap_or(&kind.0)) == *family
                && entries.contains_key(id)
            {
                return true;
            }
        }
        false
    }

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
    imports: Query<
        (&SessionId, &vmux_ecs::Uuid, &vmux_ecs::OperationState),
        With<crate::provider::Import>,
    >,
    mut mutations: MessageWriter<crate::mutation::Request>,
) {
    for request in requests.read() {
        let lease = if let Some(operation) = request.operation {
            let Ok((id, uuid, state)) = imports.get(operation) else {
                continue;
            };
            if id != request.id()
                || !matches!(
                    state,
                    vmux_ecs::OperationState::Pending | vmux_ecs::OperationState::Running
                )
            {
                continue;
            }
            Some((operation, uuid.0.clone()))
        } else {
            None
        };
        let now = vmux_ecs::UnixMillis::now().0;
        let mut mutation = crate::mutation::Request::local(
            request.id.clone(),
            crate::mutation::Mutation::Create(crate::mutation::Creation {
                name: request.name.clone(),
                description: request.description.clone(),
                cwd: request.cwd.clone(),
                agent: request.agent.clone(),
                stage: configuration.default_stage.clone(),
                created_at: request.created_at.unwrap_or(now),
                activated_at: request.activated_at.unwrap_or(now),
                local: request.local,
            }),
        );
        if let Some((operation, uuid)) = lease {
            mutation = mutation.leased(operation, uuid);
        }
        mutations.write(mutation);
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
    sessions: Query<
        (&SessionId, &AgentConversationTitle, &Name),
        (With<Session>, Changed<AgentConversationTitle>),
    >,
    mut mutations: MessageWriter<crate::mutation::Request>,
) {
    for (id, title, name) in &sessions {
        let title = title.0.trim();
        if !title.is_empty() && name.as_str() != title {
            mutations.write(crate::mutation::Request::local(
                id.clone(),
                crate::mutation::Mutation::Rename(title.to_string()),
            ));
        }
    }
}

#[cfg(host)]
fn rename(
    mut requests: MessageReader<RenameRequest>,
    mut mutations: MessageWriter<crate::mutation::Request>,
) {
    for request in requests.read() {
        mutations.write(crate::mutation::Request::local(
            request.id.clone(),
            crate::mutation::Mutation::Rename(request.name.clone()),
        ));
    }
}

#[cfg(host)]
fn update_description(
    mut requests: MessageReader<DescriptionUpdateRequest>,
    mut mutations: MessageWriter<crate::mutation::Request>,
) {
    for request in requests.read() {
        mutations.write(crate::mutation::Request::local(
            request.id.clone(),
            crate::mutation::Mutation::Describe(request.description.clone()),
        ));
    }
}

#[cfg(host)]
fn change_stage(
    mut requests: MessageReader<StageChangeRequest>,
    mut mutations: MessageWriter<crate::mutation::Request>,
) {
    for request in requests.read() {
        mutations.write(crate::mutation::Request::local(
            request.id.clone(),
            crate::mutation::Mutation::ChangeStage(request.stage.clone()),
        ));
    }
}

#[cfg(host)]
fn archive(
    mut requests: MessageReader<ArchiveRequest>,
    mut mutations: MessageWriter<crate::mutation::Request>,
) {
    for request in requests.read() {
        mutations.write(crate::mutation::Request::local(
            request.id.clone(),
            crate::mutation::Mutation::Archive(request.archived_at),
        ));
    }
}

#[cfg(host)]
fn archive_provider(
    mut requests: MessageReader<ProviderArchiveRequest>,
    mut sequence: ResMut<ArchiveSequence>,
    index: Res<ExternalIdentityIndex<ProviderKind, ProviderSessionId, ProviderArchive>>,
    mut overrides: Query<
        (
            Entity,
            &mut SourceUpdatedAt,
            Option<&ArchivedAt>,
            Option<&mut ArchiveWritePending>,
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
        let entity = match index.entity(&request.kind, &request.id) {
            Ok(entity) => entity,
            Err(error) => {
                tracing::warn!("provider archive refused: {error}");
                continue;
            }
        };
        if let Some(entity) = entity {
            let Ok((_, mut source_updated_at, current_archived_at, pending)) =
                overrides.get_mut(entity)
            else {
                continue;
            };
            source_updated_at.0 = request.source_updated_at;
            if request.archived && current_archived_at.is_none() {
                commands.entity(entity).insert(ArchivedAt(next_archived_at));
            } else if !request.archived && current_archived_at.is_some() {
                commands.entity(entity).remove::<ArchivedAt>();
            }
            if request.native {
                let revision = sequence.next();
                if let Some(mut pending) = pending {
                    pending.revision = revision;
                } else {
                    commands.entity(entity).insert(ArchiveWritePending {
                        revision,
                        in_flight: revision,
                    });
                    writes.write(ProviderArchiveWriteRequest {
                        kind: request.kind.clone(),
                        id: request.id.clone(),
                        archived: request.archived,
                        revision,
                    });
                }
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
        if request.native {
            let revision = sequence.next();
            entity.insert(ArchiveWritePending {
                revision,
                in_flight: revision,
            });
            writes.write(ProviderArchiveWriteRequest {
                kind: request.kind.clone(),
                id: request.id.clone(),
                archived: request.archived,
                revision,
            });
        }
    }
}

#[cfg(host)]
fn finish_provider_archives(
    mut completed: MessageReader<ProviderArchiveCompleted>,
    mut pending: Query<
        (
            Entity,
            &ProviderKind,
            &ProviderSessionId,
            Has<ArchivedAt>,
            &mut ArchiveWritePending,
        ),
        With<ProviderArchive>,
    >,
    mut writes: MessageWriter<ProviderArchiveWriteRequest>,
    mut commands: Commands,
) {
    for result in completed.read() {
        let mut matching = None;
        let mut duplicate = false;
        for (entity, kind, id, _, _) in &pending {
            if kind == &result.kind && id == &result.id && matching.replace(entity).is_some() {
                duplicate = true;
                break;
            }
        }
        if duplicate {
            continue;
        }
        let Some(entity) = matching else {
            continue;
        };
        let Ok((_, kind, id, archived, mut operation)) = pending.get_mut(entity) else {
            continue;
        };
        if operation.in_flight != result.revision {
            continue;
        }
        if operation.revision == result.revision {
            commands.entity(entity).remove::<ArchiveWritePending>();
            continue;
        }
        operation.in_flight = operation.revision;
        writes.write(ProviderArchiveWriteRequest {
            kind: kind.clone(),
            id: id.clone(),
            archived,
            revision: operation.revision,
        });
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
    let removed = removed_tombstones.read().count() > 0;
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
    index: Res<ExternalIdentityIndex<ProviderKind, ProviderSessionId, ProviderStage>>,
    mut stages: Query<(&mut Stage, &mut StageChangedAt, &mut SourceUpdatedAt), With<ProviderStage>>,
    mut commands: Commands,
) {
    let mut latest = HashMap::<ProviderSessionKey, ProviderStageChangeRequest>::new();
    for request in requests.read() {
        latest.insert(
            ProviderSessionKey::from_components(&request.kind, &request.id),
            request.clone(),
        );
    }
    for request in latest.into_values() {
        if !definitions.iter().any(|id| id == &request.stage) {
            continue;
        }
        let now = vmux_ecs::UnixMillis::now().0;
        let entity = match index.entity(&request.kind, &request.id) {
            Ok(entity) => entity,
            Err(error) => {
                tracing::warn!("provider stage refused: {error}");
                continue;
            }
        };
        if let Some(entity) = entity {
            let Ok((mut stage, mut changed_at, mut source_updated_at)) = stages.get_mut(entity)
            else {
                continue;
            };
            if stage.0 != request.stage {
                stage.0.clone_from(&request.stage);
            }
            changed_at.record_transition(now);
            source_updated_at.0 = request.source_updated_at;
        } else {
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
        let mut removed = false;
        removed |= self.removed_sessions.read().count() > 0;
        removed |= self.removed_provider_kinds.read().count() > 0;
        removed |= self.removed_provider_ids.read().count() > 0;
        removed |= self.removed_activated_at.read().count() > 0;
        removed |= self.removed_stage_changed_at.read().count() > 0;
        removed |= self.removed_archived_at.read().count() > 0;
        removed |= self.removed_run_state.read().count() > 0;
        removed |= self.removed_targets.read().count() > 0;
        !self.sessions.is_empty() || !self.targets.is_empty() || removed
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
    fn provider_stage_requests_in_one_frame_keep_one_identity_and_the_latest_selection() {
        let mut app = App::new();
        app.add_plugins((EntityPlugin, StagePlugin));
        app.update();
        for (stage, updated_at) in [("done", 10), ("in_progress", 20)] {
            app.world_mut().write_message(ProviderStageChangeRequest {
                kind: ProviderKind("codex".into()),
                id: ProviderSessionId("provider-session".into()),
                stage: StageId(stage.into()),
                source_updated_at: updated_at,
            });
        }
        app.update();
        let (stage, updated_at) = app
            .world_mut()
            .query_filtered::<(&Stage, &SourceUpdatedAt), With<ProviderStage>>()
            .single(app.world())
            .unwrap();
        assert_eq!(stage.0.0, "in_progress");
        assert_eq!(updated_at.0, 20);
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
    fn native_archive_coalesces_latest_intent_and_ignores_duplicate_completions() {
        let mut app = App::new();
        app.add_plugins(crate::DomainPlugin);
        let kind = ProviderKind("acp:codex".into());
        let id = ProviderSessionId("provider-session".into());
        app.world_mut().write_message(ProviderArchiveRequest {
            kind: kind.clone(),
            id: id.clone(),
            archived: true,
            source_updated_at: 40,
            native: true,
        });
        app.update();
        let first = app
            .world_mut()
            .resource_mut::<Messages<ProviderArchiveWriteRequest>>()
            .drain()
            .next()
            .unwrap();
        app.world_mut().write_message(ProviderArchiveRequest {
            kind: kind.clone(),
            id: id.clone(),
            archived: false,
            source_updated_at: 80,
            native: true,
        });
        app.update();
        assert!(
            app.world_mut()
                .resource_mut::<Messages<ProviderArchiveWriteRequest>>()
                .drain()
                .next()
                .is_none()
        );
        let completed = ProviderArchiveCompleted {
            kind: kind.clone(),
            id: id.clone(),
            archived: true,
            revision: first.revision,
            success: true,
        };
        app.world_mut().write_message(completed.clone());
        app.update();
        let writes = app
            .world_mut()
            .resource_mut::<Messages<ProviderArchiveWriteRequest>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(writes.len(), 1);
        assert!(!writes[0].archived);
        assert_ne!(writes[0].revision, first.revision);
        app.world_mut().write_message(completed);
        app.world_mut().write_message(ProviderArchiveCompleted {
            kind,
            id,
            archived: false,
            revision: writes[0].revision,
            success: true,
        });
        app.update();
        assert!(
            app.world_mut()
                .resource_mut::<Messages<ProviderArchiveWriteRequest>>()
                .drain()
                .next()
                .is_none()
        );
        let mut overrides = app
            .world_mut()
            .query_filtered::<(&SourceUpdatedAt, Has<ArchivedAt>), With<ProviderArchive>>();
        let (updated, archived) = overrides.single(app.world()).unwrap();
        assert_eq!(updated.0, 80);
        assert!(!archived);
    }

    #[test]
    fn removing_an_archive_owner_discards_its_late_write_completion() {
        let mut app = App::new();
        app.add_plugins(crate::DomainPlugin);
        let kind = ProviderKind("acp:codex".into());
        let id = ProviderSessionId("provider-session".into());
        app.world_mut().write_message(ProviderArchiveRequest {
            kind: kind.clone(),
            id: id.clone(),
            archived: true,
            source_updated_at: 40,
            native: true,
        });
        app.update();
        let first = app
            .world_mut()
            .resource_mut::<Messages<ProviderArchiveWriteRequest>>()
            .drain()
            .next()
            .unwrap();
        app.world_mut().write_message(ProviderArchiveRequest {
            kind: kind.clone(),
            id: id.clone(),
            archived: false,
            source_updated_at: 80,
            native: true,
        });
        app.update();
        let mut owners = app
            .world_mut()
            .query_filtered::<Entity, With<ProviderArchive>>();
        let owner = owners.single(app.world()).unwrap();
        app.world_mut().entity_mut(owner).despawn();
        app.world_mut().write_message(ProviderArchiveCompleted {
            kind,
            id,
            archived: true,
            revision: first.revision,
            success: true,
        });
        app.update();
        assert!(
            app.world_mut()
                .resource_mut::<Messages<ProviderArchiveWriteRequest>>()
                .drain()
                .next()
                .is_none()
        );
        assert_eq!(owners.iter(app.world()).count(), 0);
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
        let filter = vmux_ecs::store::WorkspaceStored::filter(&registry);

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
