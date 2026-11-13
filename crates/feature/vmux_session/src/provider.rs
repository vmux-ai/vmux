use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

use bevy_app::{App, Plugin as BevyPlugin, Update};
use bevy_ecs::prelude::*;
use bevy_ecs::system::{SystemParam, SystemState};
use moonshine_save::prelude::Save;
use vmux_api::open_target::{OpenTarget, PaneOpenMode};
use vmux_ecs::{
    ArchivedAt, CreatedAt, Cwd, Description, EntityTarget, OperationError, OperationPlugin,
    OperationState,
};

use crate::{
    AgentConversationTitle, AgentFamily, AgentId, ArchiveState, CreateRequest, Created,
    ProviderKind, ProviderSession, ProviderSessionId, ProviderSessionTombstones, Session,
    SessionId, SourceUpdatedAt,
};

pub struct Plugin;

impl BevyPlugin for Plugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<OperationPlugin>() {
            app.add_plugins(OperationPlugin);
        }
        app.add_message::<Received>()
            .add_message::<ArchivesReceived>()
            .add_message::<ArchiveWritten>()
            .add_message::<ImportRequested>()
            .add_message::<CreateRequest>()
            .add_message::<crate::mutation::Rejected>()
            .init_resource::<ProviderSessionTombstones>()
            .add_observer(finish_import)
            .add_systems(Update, reject_imports.after(crate::mutation::CommitSet))
            .add_systems(
                Update,
                (receive, receive_archives, receive_writes)
                    .chain()
                    .in_set(Prepared),
            )
            .add_systems(
                Update,
                (
                    receive,
                    receive_archives,
                    receive_writes,
                    receive_imports,
                    maintain_imports,
                )
                    .chain()
                    .in_set(Ready)
                    .after(Prepared)
                    .before(crate::mutation::ReceiveSet),
            );
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Prepared;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Ready;

#[derive(Component)]
pub struct Source;

#[derive(Component, Default)]
pub struct Discovery {
    pub complete: bool,
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    Acp,
    Native,
}

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Capability {
    #[default]
    Unprobed,
    Pending,
    Supported,
    Unsupported,
    Failed,
}

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Generation(pub u64);

#[derive(Component)]
pub struct Entry;

#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct ConversationTitle(pub String);

#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct LastPrompt(pub String);

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArchiveObservedAt(pub i64);

#[derive(Component, Clone, Copy, Debug)]
pub struct Read {
    pub generation: u64,
    pub wire_id: u64,
}

#[derive(Component, Clone, Copy, Debug)]
pub struct Started(pub Instant);

#[derive(Component, Clone, Copy, Debug)]
pub struct ArchiveWrittenAt(pub Instant);

#[derive(Component, Clone, Copy, Debug)]
pub struct ArchiveRead {
    pub generation: u64,
}

#[derive(Component, Clone, Copy, Debug)]
pub struct ArchiveWrite {
    pub revision: u64,
    pub archived: bool,
}

#[derive(Message)]
pub struct ArchivesReceived {
    pub operation: Entity,
    pub result: Result<Option<HashMap<String, ArchiveState>>, String>,
}

#[derive(Message)]
pub struct ArchiveWritten {
    pub operation: Entity,
    pub state: ArchiveState,
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Destination {
    pub origin: Entity,
    pub stack: Entity,
    pub pane: Option<Entity>,
    pub space: Option<Entity>,
    pub window: Option<Entity>,
    pub open: OpenTarget,
}

impl Destination {
    pub fn opens_in_place(self) -> bool {
        matches!(
            self.open,
            OpenTarget::InPlace
                | OpenTarget::InPane {
                    mode: PaneOpenMode::InPlace,
                    ..
                }
        )
    }
}

#[derive(Component, Clone, Copy, Debug)]
pub struct Import {
    generation: u64,
}

#[derive(SystemParam)]
pub(crate) struct ImportLeases<'w, 's> {
    operations: Query<
        'w,
        's,
        (
            &'static Import,
            &'static ChildOf,
            &'static EntityTarget<Entry>,
            &'static Destination,
            Option<&'static Opened>,
            &'static OperationState,
            &'static ProviderKind,
            &'static ProviderSessionId,
        ),
    >,
    sources: Query<'w, 's, &'static Generation, With<Source>>,
    entries: Query<
        'w,
        's,
        (
            &'static ChildOf,
            &'static ProviderKind,
            &'static ProviderSessionId,
            &'static AgentId,
        ),
        With<Entry>,
    >,
    parents: Query<'w, 's, &'static ChildOf>,
    entities: Query<'w, 's, Entity>,
    tombstones: Res<'w, ProviderSessionTombstones>,
}

impl ImportLeases<'_, '_> {
    fn destination(&self, destination: Destination, opened: bool) -> bool {
        if !self.entities.contains(destination.stack) {
            return false;
        }
        if !opened
            && !self
                .parents
                .get(destination.origin)
                .is_ok_and(|parent| parent.parent() == destination.stack)
        {
            return false;
        }
        for entity in [destination.pane, destination.space, destination.window]
            .into_iter()
            .flatten()
        {
            if !self.entities.contains(entity) {
                return false;
            }
        }
        true
    }

    pub(crate) fn active(&self, operation: Entity) -> bool {
        let Ok((import, parent, target, destination, opened, state, kind, id)) =
            self.operations.get(operation)
        else {
            return false;
        };
        if !matches!(state, OperationState::Pending | OperationState::Running) {
            return false;
        }
        let source = parent.parent();
        if !self
            .sources
            .get(source)
            .is_ok_and(|generation| generation.0 == import.generation)
        {
            return false;
        }
        let Ok((parent, entry_kind, entry_id, agent)) = self.entries.get(target.entity()) else {
            return false;
        };
        if parent.parent() != source
            || kind != entry_kind
            || id != entry_id
            || self
                .tombstones
                .contains_family(&AgentFamily::of(&agent.0), &id.0)
        {
            return false;
        }
        self.destination(*destination, opened.is_some())
    }
}

#[derive(Component)]
pub struct Opened;

#[derive(Message)]
pub struct ImportRequested {
    pub entry: Entity,
    pub agent: AgentId,
    pub destination: Destination,
}

#[derive(EntityEvent)]
pub struct Opening {
    pub entity: Entity,
}

#[derive(EntityEvent)]
pub struct Imported {
    pub entity: Entity,
    pub operation: Entity,
}

#[derive(Message)]
pub struct Received {
    pub operation: Entity,
    pub supported: bool,
    pub result: Result<Vec<ProviderSession>, String>,
}

#[derive(SystemParam)]
pub struct Entries<'w, 's> {
    discovery: Query<'w, 's, &'static Discovery>,
    sources: Query<
        'w,
        's,
        (
            &'static AgentId,
            &'static Origin,
            &'static Capability,
            Option<&'static Name>,
        ),
        With<Source>,
    >,
    changes: Query<
        'w,
        's,
        (),
        (
            Or<(With<Source>, With<Entry>, With<Discovery>)>,
            Or<(
                Changed<Capability>,
                Changed<Discovery>,
                Changed<Generation>,
                Changed<Origin>,
                Changed<Name>,
                Changed<Description>,
                Changed<Cwd>,
                Changed<SourceUpdatedAt>,
                Changed<ConversationTitle>,
                Changed<LastPrompt>,
                Changed<ArchiveObservedAt>,
                Changed<ArchivedAt>,
                Added<Entry>,
            )>,
        ),
    >,
    removed_sources: RemovedComponents<'w, 's, Source>,
    removed_entries: RemovedComponents<'w, 's, Entry>,
    removed_archives: RemovedComponents<'w, 's, ArchivedAt>,
    records: Query<
        'w,
        's,
        (
            Entity,
            &'static ChildOf,
            &'static ProviderKind,
            &'static ProviderSessionId,
            &'static AgentId,
            &'static Name,
            &'static Description,
            &'static Cwd,
            &'static CreatedAt,
            &'static SourceUpdatedAt,
            &'static ConversationTitle,
            &'static LastPrompt,
            Option<&'static ArchivedAt>,
            Option<&'static ArchiveObservedAt>,
        ),
        With<Entry>,
    >,
}

pub struct Record<'a> {
    pub entity: Entity,
    pub source: Entity,
    pub kind: &'a ProviderKind,
    pub id: &'a ProviderSessionId,
    pub agent: &'a AgentId,
    pub name: &'a Name,
    pub description: &'a Description,
    pub cwd: &'a Cwd,
    pub created_at: &'a CreatedAt,
    pub updated_at: &'a SourceUpdatedAt,
    pub conversation_title: &'a ConversationTitle,
    pub last_prompt: &'a LastPrompt,
    pub archived_at: Option<&'a ArchivedAt>,
    pub archive_observed_at: Option<&'a ArchiveObservedAt>,
    pub agent_name: Option<&'a Name>,
}

impl Record<'_> {
    pub fn snapshot(&self) -> ProviderSession {
        ProviderSession {
            kind: self.kind.0.clone(),
            sid: self.id.0.clone(),
            agent: self.agent.0.clone(),
            agent_name: self
                .agent_name
                .map_or_else(String::new, |name| name.as_str().to_string()),
            name: self.name.as_str().to_string(),
            description: self.description.0.clone(),
            cwd: self.cwd.0.to_string_lossy().into_owned(),
            created_at: self.created_at.0,
            updated_at: self.updated_at.0,
            conversation_title: self.conversation_title.0.clone(),
            last_prompt: self.last_prompt.0.clone(),
            archive: self.archive_observed_at.map(|at| match self.archived_at {
                Some(_) => ArchiveState::Archived(at.0),
                None => ArchiveState::Active(at.0),
            }),
        }
    }

    pub fn source_updated_at(&self) -> i64 {
        self.updated_at
            .0
            .max(self.archive_observed_at.map_or(0, |at| at.0))
    }
}

impl Entries<'_, '_> {
    pub fn is_changed(&mut self) -> bool {
        let source_removed = self.removed_sources.read().count() > 0;
        let entry_removed = self.removed_entries.read().count() > 0;
        let archive_removed = self.removed_archives.read().count() > 0;
        !self.changes.is_empty() || source_removed || entry_removed || archive_removed
    }

    pub fn from_origin(&self, origin: Origin) -> Vec<Record<'_>> {
        let mut records = Vec::new();
        for (entity, parent, _, _, _, _, _, _, _, _, _, _, _, _) in &self.records {
            if self
                .sources
                .get(parent.parent())
                .is_ok_and(|(_, found, _, _)| *found == origin)
                && let Some(record) = self.record(entity)
            {
                records.push(record);
            }
        }
        records
    }

    pub fn allows_fallback(&self, family: &AgentFamily) -> bool {
        for (agent, origin, capability, _) in &self.sources {
            if *origin == Origin::Acp
                && AgentFamily::of(&agent.0) == *family
                && *capability != Capability::Unsupported
            {
                return false;
            }
        }
        true
    }

    pub fn loading(&self) -> bool {
        self.discovery.iter().any(|discovery| !discovery.complete)
            || self.sources.iter().any(|(_, origin, capability, _)| {
                *capability == Capability::Pending
                    || *origin == Origin::Acp && *capability == Capability::Unprobed
            })
    }

    pub fn record(&self, entity: Entity) -> Option<Record<'_>> {
        let (
            entity,
            parent,
            kind,
            id,
            agent,
            name,
            description,
            cwd,
            created_at,
            updated_at,
            conversation_title,
            last_prompt,
            archived_at,
            archive_observed_at,
        ) = self.records.get(entity).ok()?;
        let (_, _, _, agent_name) = self.sources.get(parent.parent()).ok()?;
        Some(Record {
            entity,
            source: parent.parent(),
            kind,
            id,
            agent,
            name,
            description,
            cwd,
            created_at,
            updated_at,
            conversation_title,
            last_prompt,
            archived_at,
            archive_observed_at,
            agent_name,
        })
    }

    pub fn get(&self, kind: &str, sid: &str) -> Option<Record<'_>> {
        let mut selected = None;
        let mut priority = None;
        for (entity, parent, entry_kind, id, _, _, _, _, _, updated, _, _, _, _) in &self.records {
            if entry_kind.0 != kind || id.0 != sid {
                continue;
            }
            let Ok((_, origin, _, _)) = self.sources.get(parent.parent()) else {
                continue;
            };
            let key = (*origin == Origin::Acp, updated.0, entity);
            if priority.is_none_or(|previous| key > previous) {
                priority = Some(key);
                selected = Some(entity);
            }
        }
        self.record(selected?)
    }

    pub fn from_catalog(&self, catalog_id: &str) -> Option<Record<'_>> {
        for (_, _, kind, id, _, _, _, _, _, _, _, _, _, _) in &self.records {
            if catalog_id
                .strip_prefix(kind.0.as_str())
                .and_then(|tail| tail.strip_prefix(':'))
                == Some(id.0.as_str())
            {
                return self.get(&kind.0, &id.0);
            }
        }
        None
    }

    pub fn selected(&self) -> Vec<Entity> {
        let mut selected = HashMap::<(AgentFamily, String), (i64, Entity)>::new();
        for (entity, parent, _, id, agent, _, _, _, _, updated, _, _, _, _) in &self.records {
            let Ok((_, origin, _, _)) = self.sources.get(parent.parent()) else {
                continue;
            };
            let family = AgentFamily::of(&agent.0);
            if *origin == Origin::Native && !self.allows_fallback(&family) {
                continue;
            }
            let key = (family, id.0.clone());
            if selected
                .get(&key)
                .is_none_or(|previous| (updated.0, entity) >= *previous)
            {
                selected.insert(key, (updated.0, entity));
            }
        }
        let mut entries = Vec::with_capacity(selected.len());
        for (_, entry) in selected {
            entries.push(entry);
        }
        entries.sort_by_key(|entry| std::cmp::Reverse(*entry));
        entries.into_iter().map(|(_, entity)| entity).collect()
    }

    pub fn association(
        &self,
        agent: &str,
        resume: &str,
        name: &str,
        cwd: &str,
        created_at: i64,
    ) -> Option<Record<'_>> {
        let family = AgentFamily::of(agent);
        let mut direct = None;
        let mut priority = None;
        for (entity, parent, _, id, entry_agent, _, _, _, _, updated, _, _, _, _) in &self.records {
            if id.0 != resume || AgentFamily::of(&entry_agent.0) != family {
                continue;
            }
            let Ok((_, origin, _, _)) = self.sources.get(parent.parent()) else {
                continue;
            };
            let candidate = (*origin == Origin::Acp, updated.0, entity);
            if priority.is_none_or(|previous| candidate > previous) {
                priority = Some(candidate);
                direct = Some(entity);
            }
        }
        if let Some(entity) = direct {
            return self.record(entity);
        }
        let mut fallback = None;
        let mut matches = 0;
        for (
            entity,
            parent,
            _,
            id,
            entry_agent,
            entry_name,
            _,
            entry_cwd,
            entry_created,
            _,
            title,
            _,
            _,
            _,
        ) in &self.records
        {
            let Ok((_, origin, _, _)) = self.sources.get(parent.parent()) else {
                continue;
            };
            if *origin != Origin::Native || AgentFamily::of(&entry_agent.0) != family {
                continue;
            }
            if id.0 == resume {
                return self.record(entity);
            }
            if entry_cwd.0.to_string_lossy() != cwd || entry_created.0 != created_at {
                continue;
            }
            if entry_name.as_str() == name || title.0 == name {
                return self.record(entity);
            }
            matches += 1;
            fallback = Some(entity);
        }
        if matches == 1 {
            self.record(fallback?)
        } else {
            None
        }
    }
}

fn receive_imports(world: &mut World) {
    let requests: Vec<_> = world
        .resource_mut::<Messages<ImportRequested>>()
        .drain()
        .collect();
    for request in requests {
        let mut leases = SystemState::<ImportLeases>::new(world);
        if !leases
            .get(world)
            .expect("import lease parameters")
            .destination(request.destination, false)
        {
            continue;
        }
        let Some(source) = world.get::<ChildOf>(request.entry).map(ChildOf::parent) else {
            continue;
        };
        if world.get::<Entry>(request.entry).is_none() || world.get::<Source>(source).is_none() {
            continue;
        }
        let Some(agent) = world.get::<AgentId>(request.entry) else {
            continue;
        };
        let family = AgentFamily::of(&agent.0);
        if world.get::<Origin>(source) == Some(&Origin::Native) {
            let mut sources =
                world.query_filtered::<(&AgentId, &Origin, &Capability), With<Source>>();
            if sources.iter(world).any(|(agent, origin, capability)| {
                *origin == Origin::Acp
                    && AgentFamily::of(&agent.0) == family
                    && *capability != Capability::Unsupported
            }) {
                continue;
            }
        }
        if AgentFamily::of(&request.agent.0) != family {
            continue;
        }
        let Some(kind) = world.get::<ProviderKind>(request.entry).cloned() else {
            continue;
        };
        let Some(id) = world.get::<ProviderSessionId>(request.entry).cloned() else {
            continue;
        };
        if world
            .resource::<ProviderSessionTombstones>()
            .contains_family(&family, &id.0)
        {
            continue;
        }
        let Some(generation) = world.get::<Generation>(source).copied() else {
            continue;
        };
        let mut imports = world.query_filtered::<(Entity, &ProviderSessionId, &AgentId, &OperationState), With<Import>>();
        let mut existing = None;
        for (entity, found, agent, state) in imports.iter(world) {
            if found == &id
                && AgentFamily::of(&agent.0) == family
                && matches!(state, OperationState::Pending | OperationState::Running)
            {
                existing = Some(entity);
                break;
            }
        }
        if let Some(operation) = existing {
            let changed = world.get::<Destination>(operation) != Some(&request.destination);
            world.entity_mut(operation).insert((
                Import {
                    generation: generation.0,
                },
                ChildOf(source),
                EntityTarget::<Entry>::new(request.entry),
                kind,
                id,
                request.destination,
            ));
            if request.destination.opens_in_place() {
                if changed || world.get::<Opened>(operation).is_none() {
                    world.entity_mut(operation).insert(Opened);
                    world.trigger(Opening { entity: operation });
                }
            } else {
                world.entity_mut(operation).remove::<Opened>();
            }
            continue;
        }
        let operation = world
            .spawn((
                Import {
                    generation: generation.0,
                },
                ChildOf(source),
                EntityTarget::<Entry>::new(request.entry),
                kind,
                id,
                request.agent.clone(),
                request.destination,
                vmux_ecs::Operation::bundle("provider import")
                    .expect("import operation kind")
                    .running(),
            ))
            .id();
        let Some(cwd) = world.get::<Cwd>(request.entry).cloned() else {
            world.entity_mut(operation).insert(OperationState::Failed);
            continue;
        };
        let cwd = match cwd.0.canonicalize() {
            Ok(cwd) if cwd.is_dir() => cwd,
            _ => {
                world.entity_mut(operation).insert((
                    OperationState::Failed,
                    OperationError::new("provider working directory is unavailable")
                        .expect("bounded import error"),
                ));
                continue;
            }
        };
        let id = world
            .get::<ProviderSessionId>(request.entry)
            .expect("provider entry identity");
        let name = world.get::<Name>(request.entry).map_or_else(
            || id.0.clone(),
            |name| {
                if name.as_str().trim().is_empty() {
                    id.0.clone()
                } else {
                    name.as_str().to_string()
                }
            },
        );
        let description = world
            .get::<Description>(request.entry)
            .map_or_else(String::new, |description| description.0.clone());
        let created_at = world.get::<CreatedAt>(request.entry).map_or(0, |at| at.0);
        let updated_at = world
            .get::<SourceUpdatedAt>(request.entry)
            .map_or(0, |at| at.0);
        let create = CreateRequest::imported(
            name,
            description,
            cwd,
            request.agent,
            created_at,
            updated_at,
        )
        .leased(operation);
        world.entity_mut(operation).insert(create.id().clone());
        if request.destination.opens_in_place() {
            world.entity_mut(operation).insert(Opened);
            world.trigger(Opening { entity: operation });
        }
        world.write_message(create);
    }
}

fn maintain_imports(world: &mut World) {
    let mut imports = world.query_filtered::<Entity, With<Import>>();
    let operations: Vec<_> = imports.iter(world).collect();
    let mut leases = SystemState::<ImportLeases>::new(world);
    let mut canceled = Vec::new();
    {
        let leases = leases.get(world).expect("import lease parameters");
        for operation in operations {
            if world.get::<OperationState>(operation).is_some_and(|state| {
                matches!(state, OperationState::Pending | OperationState::Running)
            }) && !leases.active(operation)
            {
                canceled.push(operation);
            }
        }
    }
    for operation in canceled {
        world.entity_mut(operation).insert(OperationState::Canceled);
    }
}

fn finish_import(
    event: On<Created>,
    imports: Query<(Entity, &SessionId, &vmux_ecs::Uuid), With<Import>>,
    leases: ImportLeases,
    world: &World,
    mut commands: Commands,
) {
    if world.get::<Session>(event.entity).is_none()
        || world.get::<SessionId>(event.entity) != Some(&event.id)
    {
        return;
    }
    for (operation, id, uuid) in &imports {
        if id != &event.id || !leases.active(operation) {
            continue;
        }
        let committed = world.get::<Children>(event.entity).is_some_and(|children| {
            children.iter().any(|record| {
                world.get::<vmux_ecs::revision::Record>(record).is_some()
                    && world.get::<vmux_ecs::Uuid>(record) == Some(uuid)
            })
        });
        if !committed {
            continue;
        }
        let target = world
            .get::<EntityTarget<Entry>>(operation)
            .expect("active import target")
            .entity();
        let kind = world
            .get::<ProviderKind>(target)
            .expect("active provider kind")
            .clone();
        let sid = world
            .get::<ProviderSessionId>(target)
            .expect("active provider identity")
            .clone();
        let title = match world.get::<ConversationTitle>(target) {
            Some(title) if !title.0.trim().is_empty() => title.0.clone(),
            _ => world
                .get::<Name>(target)
                .map_or_else(String::new, |name| name.as_str().to_string()),
        };
        commands
            .entity(event.entity)
            .insert((kind, sid, AgentConversationTitle(title)));
        commands.entity(operation).insert(OperationState::Succeeded);
        commands.trigger(Imported {
            entity: event.entity,
            operation,
        });
        break;
    }
}

fn reject_imports(
    mut rejected: MessageReader<crate::mutation::Rejected>,
    imports: Query<(Entity, &SessionId, &vmux_ecs::Uuid, &OperationState), With<Import>>,
    mut commands: Commands,
) {
    for rejected in rejected.read() {
        for (operation, id, uuid, state) in &imports {
            if id != &rejected.session
                || uuid.0 != rejected.operation
                || !matches!(state, OperationState::Pending | OperationState::Running)
            {
                continue;
            }
            commands.entity(operation).insert(OperationState::Failed);
            if let Ok(error) = OperationError::new(format!("{:?}", rejected.reason)) {
                commands.entity(operation).insert(error);
            }
        }
    }
}

fn receive(world: &mut World) {
    let received: Vec<_> = world.resource_mut::<Messages<Received>>().drain().collect();
    for received in received {
        let Some(read) = world.get::<Read>(received.operation).copied() else {
            continue;
        };
        let Some(source) = world
            .get::<ChildOf>(received.operation)
            .map(ChildOf::parent)
        else {
            continue;
        };
        if world
            .get::<OperationState>(received.operation)
            .is_none_or(|state| !matches!(state, OperationState::Pending | OperationState::Running))
        {
            continue;
        }
        if world.get::<Source>(source).is_none()
            || world
                .get::<Generation>(source)
                .is_none_or(|generation| generation.0 != read.generation)
            || world
                .get::<EntityTarget<Read>>(source)
                .is_none_or(|target| target.entity() != received.operation)
        {
            world
                .entity_mut(received.operation)
                .insert(OperationState::Canceled);
            if world.get::<Source>(source).is_some()
                && world
                    .get::<EntityTarget<Read>>(source)
                    .is_some_and(|target| target.entity() == received.operation)
            {
                world
                    .entity_mut(source)
                    .insert(Capability::Failed)
                    .remove::<EntityTarget<Read>>();
            }
            continue;
        }
        let entries = match received.result {
            Ok(entries) => entries,
            Err(error) => {
                if let Ok(error) = OperationError::new(error) {
                    world.entity_mut(received.operation).insert(error.clone());
                    world.entity_mut(source).insert(error);
                }
                world
                    .entity_mut(source)
                    .insert(Capability::Failed)
                    .remove::<EntityTarget<Read>>();
                world
                    .entity_mut(received.operation)
                    .insert(OperationState::Failed);
                continue;
            }
        };
        let native = world.get::<Origin>(source) == Some(&Origin::Native);
        let Some(agent) = world.get::<AgentId>(source).cloned() else {
            continue;
        };
        let archives_current = match (
            world.get::<Started>(received.operation),
            world.get::<ArchiveWrittenAt>(source),
        ) {
            (Some(read), Some(written)) => read.0 >= written.0,
            _ => true,
        };
        let mut current = HashMap::new();
        if let Some(children) = world.get::<Children>(source) {
            for child in children.iter() {
                if world.get::<Entry>(child).is_none() {
                    continue;
                }
                if let (Some(kind), Some(id)) = (
                    world.get::<ProviderKind>(child),
                    world.get::<ProviderSessionId>(child),
                ) {
                    current.insert((kind.0.clone(), id.0.clone()), child);
                }
            }
        }
        let mut incoming = HashMap::<(String, String), usize>::new();
        if received.supported {
            for (position, entry) in entries.iter().enumerate() {
                if entry.sid.trim().is_empty() || entry.kind.trim().is_empty() {
                    continue;
                }
                let key = (entry.kind.clone(), entry.sid.clone());
                if incoming
                    .get(&key)
                    .is_some_and(|previous| entries[*previous].updated_at > entry.updated_at)
                {
                    continue;
                }
                incoming.insert(key, position);
            }
        }
        for (key, position) in incoming {
            let entry = &entries[position];
            let entity = match current.remove(&key) {
                Some(entity) => entity,
                None => world.spawn((Entry, ChildOf(source))).id(),
            };
            let cwd = if native && entry.cwd.is_empty() {
                world.get::<Cwd>(entity).cloned().unwrap_or_default()
            } else {
                Cwd(PathBuf::from(&entry.cwd))
            };
            let created_at = if native && entry.created_at == 0 {
                world.get::<CreatedAt>(entity).copied().unwrap_or_default()
            } else {
                CreatedAt(entry.created_at)
            };
            let updated_at = if native {
                world
                    .get::<SourceUpdatedAt>(entity)
                    .map_or(entry.updated_at, |old| old.0.max(entry.updated_at))
            } else {
                entry.updated_at
            };
            world.entity_mut(entity).insert((
                ProviderKind(entry.kind.clone()),
                ProviderSessionId(entry.sid.clone()),
                agent.clone(),
                Name::new(entry.name.clone()),
                Description(entry.description.clone()),
                cwd,
                created_at,
                SourceUpdatedAt(updated_at),
                ConversationTitle(entry.conversation_title.clone()),
                LastPrompt(entry.last_prompt.clone()),
            ));
            if archives_current && let Some(archive) = entry.archive {
                let observed = match archive {
                    ArchiveState::Active(at) | ArchiveState::Archived(at) => at,
                };
                if world
                    .get::<ArchiveObservedAt>(entity)
                    .is_none_or(|old| old.0 <= observed)
                {
                    world.entity_mut(entity).insert(ArchiveObservedAt(observed));
                    match archive {
                        ArchiveState::Active(_) => {
                            world.entity_mut(entity).remove::<ArchivedAt>();
                        }
                        ArchiveState::Archived(at) => {
                            world.entity_mut(entity).insert(ArchivedAt(at));
                        }
                    }
                }
            }
            world.entity_mut(entity).remove::<Save>();
        }
        for (_, entity) in current {
            world.despawn(entity);
        }
        world
            .entity_mut(source)
            .insert(if received.supported {
                Capability::Supported
            } else {
                Capability::Unsupported
            })
            .remove::<EntityTarget<Read>>()
            .remove::<OperationError>();
        world
            .entity_mut(received.operation)
            .insert(OperationState::Succeeded)
            .remove::<OperationError>();
    }
}

fn receive_archives(world: &mut World) {
    let received: Vec<_> = world
        .resource_mut::<Messages<ArchivesReceived>>()
        .drain()
        .collect();
    for received in received {
        let Some(read) = world.get::<ArchiveRead>(received.operation).copied() else {
            continue;
        };
        let Some(source) = world
            .get::<ChildOf>(received.operation)
            .map(ChildOf::parent)
        else {
            continue;
        };
        if world.get::<OperationState>(received.operation) != Some(&OperationState::Running) {
            continue;
        }
        if world.get::<Source>(source).is_none()
            || world
                .get::<Generation>(source)
                .is_none_or(|generation| generation.0 != read.generation)
            || world
                .get::<EntityTarget<ArchiveRead>>(source)
                .is_none_or(|target| target.entity() != received.operation)
        {
            world
                .entity_mut(received.operation)
                .insert(OperationState::Canceled);
            if world
                .get::<EntityTarget<ArchiveRead>>(source)
                .is_some_and(|target| target.entity() == received.operation)
            {
                world
                    .entity_mut(source)
                    .remove::<EntityTarget<ArchiveRead>>();
            }
            continue;
        }
        let archives = match received.result {
            Ok(archives) => archives,
            Err(error) => {
                if let Ok(error) = OperationError::new(error) {
                    world.entity_mut(received.operation).insert(error);
                }
                world
                    .entity_mut(received.operation)
                    .insert(OperationState::Failed);
                world
                    .entity_mut(source)
                    .remove::<EntityTarget<ArchiveRead>>();
                continue;
            }
        };
        let current = match (
            world.get::<Started>(received.operation),
            world.get::<ArchiveWrittenAt>(source),
        ) {
            (Some(read), Some(written)) => read.0 >= written.0,
            _ => true,
        };
        if current && let Some(archives) = archives {
            let family = world
                .get::<AgentId>(source)
                .map(|agent| AgentFamily::of(&agent.0));
            let mut entries =
                world.query_filtered::<(Entity, &AgentId, &ProviderSessionId), With<Entry>>();
            let mut observations = Vec::new();
            for (entity, agent, id) in entries.iter(world) {
                if family.as_ref() == Some(&AgentFamily::of(&agent.0))
                    && let Some(state) = archives.get(&id.0)
                {
                    observations.push((entity, *state));
                }
            }
            for (entity, state) in observations {
                if world
                    .get::<ArchiveObservedAt>(entity)
                    .is_some_and(|old| old.0 > state.updated_at())
                {
                    continue;
                }
                world
                    .entity_mut(entity)
                    .insert(ArchiveObservedAt(state.updated_at()));
                match state {
                    ArchiveState::Active(_) => {
                        world.entity_mut(entity).remove::<ArchivedAt>();
                    }
                    ArchiveState::Archived(at) => {
                        world.entity_mut(entity).insert(ArchivedAt(at));
                    }
                }
            }
        }
        world
            .entity_mut(source)
            .remove::<EntityTarget<ArchiveRead>>();
        world
            .entity_mut(received.operation)
            .insert(OperationState::Succeeded);
    }
}

fn receive_writes(world: &mut World) {
    let received: Vec<_> = world
        .resource_mut::<Messages<ArchiveWritten>>()
        .drain()
        .collect();
    for received in received {
        if world.get::<ArchiveWrite>(received.operation).is_none()
            || world.get::<OperationState>(received.operation) != Some(&OperationState::Running)
        {
            continue;
        }
        let Some(source) = world
            .get::<ChildOf>(received.operation)
            .map(ChildOf::parent)
        else {
            continue;
        };
        if world.get::<Source>(source).is_none() {
            continue;
        }
        let Some(kind) = world.get::<ProviderKind>(received.operation) else {
            continue;
        };
        let family = AgentFamily::of(kind.0.strip_prefix("acp:").unwrap_or(&kind.0));
        if world.get::<Origin>(source) != Some(&Origin::Native)
            || world
                .get::<AgentId>(source)
                .is_none_or(|agent| AgentFamily::of(&agent.0) != family)
        {
            continue;
        }
        let Some(id) = world.get::<ProviderSessionId>(received.operation).cloned() else {
            continue;
        };
        let completed_at = Instant::now();
        world
            .entity_mut(source)
            .insert(ArchiveWrittenAt(completed_at));
        let mut entries =
            world.query_filtered::<(Entity, &AgentId, &ProviderSessionId), With<Entry>>();
        let mut targets = Vec::new();
        for (entity, agent, found) in entries.iter(world) {
            if found == &id && AgentFamily::of(&agent.0) == family {
                targets.push(entity);
            }
        }
        for entity in targets {
            world.entity_mut(entity).insert((
                ArchiveWrittenAt(completed_at),
                ArchiveObservedAt(received.state.updated_at()),
            ));
            match received.state {
                ArchiveState::Active(_) => {
                    world.entity_mut(entity).remove::<ArchivedAt>();
                }
                ArchiveState::Archived(at) => {
                    world.entity_mut(entity).insert(ArchivedAt(at));
                }
            }
        }
        world
            .entity_mut(received.operation)
            .insert(OperationState::Succeeded);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_ecs::system::{RunSystemOnce, SystemState};

    struct Fixture {
        app: App,
        source: Entity,
    }

    impl Fixture {
        fn new() -> Self {
            let mut app = App::new();
            app.add_plugins(Plugin);
            let source = app
                .world_mut()
                .spawn((
                    Source,
                    Origin::Native,
                    AgentId("test".into()),
                    Generation(1),
                    Capability::Unprobed,
                ))
                .id();
            Self { app, source }
        }

        fn read(&mut self, generation: u64) -> Entity {
            let operation = self
                .app
                .world_mut()
                .spawn((
                    Read {
                        generation,
                        wire_id: 1,
                    },
                    OperationState::Running,
                    ChildOf(self.source),
                ))
                .id();
            self.app
                .world_mut()
                .entity_mut(self.source)
                .insert((Generation(generation), EntityTarget::<Read>::new(operation)));
            operation
        }

        fn receive(&mut self, operation: Entity, result: Result<Vec<ProviderSession>, String>) {
            self.app.world_mut().write_message(Received {
                operation,
                supported: true,
                result,
            });
            self.app.update();
        }

        fn names(&mut self) -> Vec<String> {
            self.app
                .world_mut()
                .query_filtered::<&Name, With<Entry>>()
                .iter(self.app.world())
                .map(|name| name.as_str().to_string())
                .collect()
        }
    }

    #[test]
    fn primary_acp_entries_associate_without_a_native_catalog() {
        let mut fixture = Fixture::new();
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.source)
            .insert(Origin::Acp);
        let read = fixture.read(1);
        fixture.receive(
            read,
            Ok(vec![ProviderSession {
                kind: "acp:test".into(),
                sid: "live-id".into(),
                name: "Primary".into(),
                cwd: "/work".into(),
                created_at: 100,
                ..Default::default()
            }]),
        );
        let associated = fixture
            .app
            .world_mut()
            .run_system_once(|entries: Entries| {
                entries
                    .association("test", "live-id", "unrelated", "/different", 0)
                    .map(|entry| entry.kind.0.clone())
            })
            .unwrap();
        assert_eq!(associated.as_deref(), Some("acp:test"));
    }

    #[test]
    fn native_import_requires_every_acp_source_to_be_unsupported() {
        for capability in [
            Capability::Unprobed,
            Capability::Pending,
            Capability::Failed,
            Capability::Supported,
            Capability::Unsupported,
        ] {
            let cwd = tempfile::tempdir().unwrap();
            let mut fixture = Fixture::new();
            fixture.app.add_plugins(crate::DomainPlugin);
            let read = fixture.read(1);
            fixture.receive(
                read,
                Ok(vec![ProviderSession {
                    kind: "acp:test".into(),
                    sid: "id".into(),
                    name: "Imported".into(),
                    cwd: cwd.path().to_string_lossy().into_owned(),
                    ..Default::default()
                }]),
            );
            let entry = fixture
                .app
                .world_mut()
                .query_filtered::<Entity, With<Entry>>()
                .single(fixture.app.world())
                .unwrap();
            fixture.app.world_mut().spawn((
                Source,
                Origin::Acp,
                AgentId("test".into()),
                capability,
                Generation(1),
            ));
            let stack = fixture.app.world_mut().spawn_empty().id();
            let origin = fixture.app.world_mut().spawn(ChildOf(stack)).id();
            fixture.app.world_mut().write_message(ImportRequested {
                entry,
                agent: AgentId("test".into()),
                destination: Destination {
                    origin,
                    stack,
                    pane: None,
                    space: None,
                    window: None,
                    open: OpenTarget::InNewStack,
                },
            });
            fixture.app.update();
            let sessions = fixture
                .app
                .world_mut()
                .query_filtered::<Entity, With<Session>>()
                .iter(fixture.app.world())
                .count();
            assert_eq!(
                sessions,
                usize::from(capability == Capability::Unsupported),
                "{capability:?}"
            );
        }
    }

    #[test]
    fn queued_import_creation_refuses_cancellation_and_invalidated_leases() {
        for interruption in ["cancel", "source", "generation", "destination"] {
            let cwd = tempfile::tempdir().unwrap();
            let mut fixture = Fixture::new();
            fixture.app.add_plugins(crate::DomainPlugin);
            let read = fixture.read(1);
            fixture.receive(
                read,
                Ok(vec![ProviderSession {
                    kind: "acp:test".into(),
                    sid: "id".into(),
                    name: "Imported".into(),
                    cwd: cwd.path().to_string_lossy().into_owned(),
                    created_at: 10,
                    updated_at: 20,
                    ..Default::default()
                }]),
            );
            let entry = fixture
                .app
                .world_mut()
                .query_filtered::<Entity, With<Entry>>()
                .single(fixture.app.world())
                .unwrap();
            let stack = fixture.app.world_mut().spawn_empty().id();
            let origin = fixture.app.world_mut().spawn(ChildOf(stack)).id();
            let source = fixture.source;
            fixture.app.add_systems(
                Update,
                (move |mut operations: Query<&mut OperationState, With<Import>>,
                       mut generations: Query<&mut Generation, With<Source>>,
                       mut commands: Commands| {
                    match interruption {
                        "cancel" => {
                            for mut state in &mut operations {
                                *state = OperationState::Canceled;
                            }
                        }
                        "source" => {
                            if generations.get(source).is_ok() {
                                commands.entity(source).despawn();
                            }
                        }
                        "generation" => {
                            if let Ok(mut generation) = generations.get_mut(source) {
                                generation.0 += 1;
                            }
                        }
                        "destination" => {
                            commands.entity(stack).despawn();
                        }
                        _ => unreachable!(),
                    }
                })
                .after(crate::mutation::ReceiveSet)
                .before(crate::mutation::CommitSet),
            );
            fixture.app.world_mut().write_message(ImportRequested {
                entry,
                agent: AgentId("test".into()),
                destination: Destination {
                    origin,
                    stack,
                    pane: None,
                    space: None,
                    window: None,
                    open: OpenTarget::InNewStack,
                },
            });
            fixture.app.update();
            assert_eq!(
                fixture
                    .app
                    .world_mut()
                    .query_filtered::<Entity, With<Session>>()
                    .iter(fixture.app.world())
                    .count(),
                0,
                "{interruption}"
            );
            let rejected = fixture
                .app
                .world_mut()
                .resource_mut::<Messages<crate::mutation::Rejected>>()
                .drain()
                .collect::<Vec<_>>();
            assert_eq!(rejected.len(), 1, "{interruption}");
            assert_eq!(
                rejected[0].reason,
                crate::mutation::Rejection::Canceled,
                "{interruption}"
            );
        }
    }

    #[test]
    fn native_lookup_results_preserve_canonical_metadata_and_keep_the_latest_identity() {
        let mut fixture = Fixture::new();
        let operation = fixture.read(1);
        fixture.receive(
            operation,
            Ok(vec![
                ProviderSession {
                    kind: "native:test".into(),
                    sid: "id".into(),
                    name: "old".into(),
                    updated_at: 1,
                    ..Default::default()
                },
                ProviderSession {
                    kind: "native:test".into(),
                    sid: "id".into(),
                    name: "current".into(),
                    cwd: "/work".into(),
                    created_at: 10,
                    updated_at: 20,
                    archive: Some(ArchiveState::Archived(20)),
                    ..Default::default()
                },
            ]),
        );
        assert_eq!(fixture.names(), ["current"]);
        let operation = fixture.read(2);
        fixture.receive(
            operation,
            Ok(vec![ProviderSession {
                kind: "native:test".into(),
                sid: "id".into(),
                name: "new title".into(),
                updated_at: 30,
                ..Default::default()
            }]),
        );
        let observed = fixture
            .app
            .world_mut()
            .run_system_once(|entries: Entries| {
                let record = entries.get("native:test", "id").unwrap();
                (
                    record.name.as_str().to_string(),
                    record.cwd.0.clone(),
                    record.created_at.0,
                    record.archived_at.map(|at| at.0),
                )
            })
            .unwrap();
        assert_eq!(
            observed,
            ("new title".into(), PathBuf::from("/work"), 10, Some(20))
        );
    }

    #[test]
    fn completed_archive_writes_win_over_older_catalog_and_archive_reads() {
        let mut fixture = Fixture::new();
        let operation = fixture.read(1);
        fixture.receive(
            operation,
            Ok(vec![ProviderSession {
                kind: "native:test".into(),
                sid: "id".into(),
                name: "old title".into(),
                archive: Some(ArchiveState::Active(100)),
                ..Default::default()
            }]),
        );
        let operation = fixture.read(2);
        let started_at = Instant::now();
        fixture
            .app
            .world_mut()
            .entity_mut(operation)
            .insert(Started(started_at));
        let archive_read = fixture
            .app
            .world_mut()
            .spawn((
                ArchiveRead { generation: 2 },
                Started(started_at),
                OperationState::Running,
                ChildOf(fixture.source),
            ))
            .id();
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.source)
            .insert(EntityTarget::<ArchiveRead>::new(archive_read));
        let write = fixture
            .app
            .world_mut()
            .spawn((
                ArchiveWrite {
                    archived: true,
                    revision: 1,
                },
                OperationState::Running,
                ProviderKind("acp:test".into()),
                ProviderSessionId("id".into()),
                ChildOf(fixture.source),
            ))
            .id();
        fixture.app.world_mut().write_message(ArchiveWritten {
            operation: write,
            state: ArchiveState::Archived(300),
        });
        fixture.app.update();
        fixture.app.world_mut().write_message(ArchivesReceived {
            operation: archive_read,
            result: Ok(Some(HashMap::from([(
                "id".into(),
                ArchiveState::Active(300),
            )]))),
        });
        fixture.receive(
            operation,
            Ok(vec![ProviderSession {
                kind: "native:test".into(),
                sid: "id".into(),
                name: "new title".into(),
                archive: Some(ArchiveState::Active(300)),
                ..Default::default()
            }]),
        );
        let archived = fixture
            .app
            .world_mut()
            .run_system_once(|entries: Entries| {
                entries
                    .get("native:test", "id")
                    .unwrap()
                    .archived_at
                    .map(|at| at.0)
            })
            .unwrap();
        assert_eq!(archived, Some(300));
        assert_eq!(fixture.names(), ["new title"]);
        let fresh = fixture
            .app
            .world_mut()
            .spawn((
                ArchiveRead { generation: 2 },
                Started(Instant::now()),
                OperationState::Running,
                ChildOf(fixture.source),
            ))
            .id();
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.source)
            .insert(EntityTarget::<ArchiveRead>::new(fresh));
        fixture.app.world_mut().write_message(ArchivesReceived {
            operation: fresh,
            result: Ok(Some(HashMap::from([(
                "id".into(),
                ArchiveState::Active(400),
            )]))),
        });
        fixture.app.update();
        let archived = fixture
            .app
            .world_mut()
            .run_system_once(|entries: Entries| {
                entries
                    .get("native:test", "id")
                    .unwrap()
                    .archived_at
                    .is_some()
            })
            .unwrap();
        assert!(!archived);
    }

    #[test]
    fn late_archive_completion_cannot_clear_the_current_request() {
        let mut fixture = Fixture::new();
        let old = fixture
            .app
            .world_mut()
            .spawn((
                ArchiveRead { generation: 1 },
                Started(Instant::now()),
                OperationState::Running,
                ChildOf(fixture.source),
            ))
            .id();
        let current = fixture
            .app
            .world_mut()
            .spawn((
                ArchiveRead { generation: 1 },
                Started(Instant::now()),
                OperationState::Running,
                ChildOf(fixture.source),
            ))
            .id();
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.source)
            .insert(EntityTarget::<ArchiveRead>::new(current));
        fixture.app.world_mut().write_message(ArchivesReceived {
            operation: old,
            result: Ok(None),
        });
        fixture.app.update();
        assert_eq!(
            fixture
                .app
                .world()
                .get::<EntityTarget<ArchiveRead>>(fixture.source)
                .unwrap()
                .entity(),
            current
        );
        assert_eq!(
            fixture.app.world().get::<OperationState>(current),
            Some(&OperationState::Running)
        );
        assert_eq!(
            fixture.app.world().get::<OperationState>(old),
            Some(&OperationState::Canceled)
        );
    }

    #[test]
    fn removing_entries_and_sources_invalidates_the_catalog_projection() {
        let mut fixture = Fixture::new();
        let mut observed = SystemState::<Entries>::new(fixture.app.world_mut());
        let operation = fixture.read(1);
        fixture.receive(
            operation,
            Ok(vec![ProviderSession {
                kind: "native:test".into(),
                sid: "id".into(),
                agent: "test".into(),
                name: "visible".into(),
                ..Default::default()
            }]),
        );
        assert!(
            observed
                .get_mut(fixture.app.world_mut())
                .unwrap()
                .is_changed()
        );
        fixture.app.update();
        assert!(
            !observed
                .get_mut(fixture.app.world_mut())
                .unwrap()
                .is_changed()
        );
        let entry = fixture
            .app
            .world_mut()
            .query_filtered::<Entity, With<Entry>>()
            .single(fixture.app.world())
            .unwrap();
        fixture.app.world_mut().despawn(entry);
        fixture.app.update();
        assert!(
            observed
                .get_mut(fixture.app.world_mut())
                .unwrap()
                .is_changed()
        );
        assert!(fixture.names().is_empty());
        fixture.app.world_mut().despawn(fixture.source);
        fixture.app.update();
        assert!(
            observed
                .get_mut(fixture.app.world_mut())
                .unwrap()
                .is_changed()
        );
        fixture.app.update();
        assert!(
            !observed
                .get_mut(fixture.app.world_mut())
                .unwrap()
                .is_changed()
        );
    }

    #[test]
    fn a_pending_provider_does_not_hide_another_completed_source() {
        let mut fixture = Fixture::new();
        let operation = fixture.read(1);
        fixture.receive(
            operation,
            Ok(vec![ProviderSession {
                kind: "native:test".into(),
                sid: "ready".into(),
                agent: "test".into(),
                name: "ready".into(),
                ..Default::default()
            }]),
        );
        fixture.app.world_mut().spawn((
            Source,
            Origin::Acp,
            AgentId("different".into()),
            Capability::Pending,
        ));
        let (loading, selected) = fixture
            .app
            .world_mut()
            .run_system_once(|entries: Entries| (entries.loading(), entries.selected().len()))
            .unwrap();
        assert!(loading);
        assert_eq!(selected, 1);
    }

    #[test]
    fn canonical_query_retains_hidden_entries_and_requires_explicit_fallback() {
        let mut fixture = Fixture::new();
        let read = fixture.read(1);
        fixture.receive(
            read,
            Ok(vec![ProviderSession {
                kind: "native:test".into(),
                sid: "id".into(),
                agent: "test".into(),
                name: "native".into(),
                ..Default::default()
            }]),
        );
        let acp = fixture
            .app
            .world_mut()
            .spawn((
                Source,
                Origin::Acp,
                AgentId("test".into()),
                Capability::Pending,
                Generation(1),
            ))
            .id();
        for (capability, visible) in [
            (Capability::Pending, false),
            (Capability::Failed, false),
            (Capability::Supported, false),
            (Capability::Unsupported, true),
        ] {
            fixture.app.world_mut().entity_mut(acp).insert(capability);
            let (selected, retained) = fixture
                .app
                .world_mut()
                .run_system_once(|entries: Entries| {
                    (
                        entries.selected().len(),
                        entries.get("native:test", "id").is_some(),
                    )
                })
                .unwrap();
            assert_eq!(selected, usize::from(visible));
            assert!(retained);
        }
        fixture.app.world_mut().despawn(acp);
        let selected = fixture
            .app
            .world_mut()
            .run_system_once(|entries: Entries| entries.selected().len())
            .unwrap();
        assert_eq!(selected, 1);
    }

    #[test]
    fn accepted_source_results_update_entities_and_failures_keep_previous_entries() {
        let mut fixture = Fixture::new();
        let first = fixture.read(1);
        fixture.receive(
            first,
            Ok(vec![ProviderSession {
                kind: "native:test".into(),
                sid: "id".into(),
                name: "first".into(),
                ..Default::default()
            }]),
        );
        let entity = fixture
            .app
            .world_mut()
            .query_filtered::<Entity, With<Entry>>()
            .single(fixture.app.world())
            .unwrap();
        let current = fixture.read(2);
        fixture.receive(
            first,
            Ok(vec![ProviderSession {
                kind: "native:test".into(),
                sid: "id".into(),
                name: "stale".into(),
                ..Default::default()
            }]),
        );
        assert_eq!(fixture.names(), ["first"]);
        fixture.receive(current, Err("offline".into()));
        assert_eq!(fixture.names(), ["first"]);
        let current = fixture.read(3);
        fixture.receive(
            current,
            Ok(vec![ProviderSession {
                kind: "native:test".into(),
                sid: "id".into(),
                name: "updated".into(),
                ..Default::default()
            }]),
        );
        assert_eq!(fixture.names(), ["updated"]);
        assert!(fixture.app.world().get::<Entry>(entity).is_some());
        assert!(fixture.app.world().get::<Save>(entity).is_none());
    }
}
