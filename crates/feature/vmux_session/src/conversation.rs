use std::collections::{HashMap, HashSet};

use bevy_app::{App, Plugin, PostUpdate, Update};
use bevy_ecs::prelude::*;
use bevy_ecs::system::{SystemParam, SystemState};
use bevy_reflect::{Reflect, ReflectDeserialize, ReflectSerialize};
use moonshine_save::prelude::Save;
use serde::{Deserialize, Serialize};
use vmux_api::service::ServiceMessageSet;
use vmux_api::session::{
    AssistantBlock, ClientOpId, ConversationEvent as SerializedEvent, EventId, MemberId,
    MemberKind, MemberRole, Message,
};
use vmux_ecs::store::StoreAppExt;
use vmux_ecs::{CreatedAt, EntityTarget, IdentityIndex, IdentityIndexPlugin, IdentityIndexSet};

use crate::history::{Applied, Committed, FileFacts, Read, Received, Versions};
use crate::{
    AgentId, ProviderKind, ProviderSessionId, ProviderSessionKey, RunState, Session, SessionId,
};

pub(crate) struct ConversationPlugin;

impl Plugin for ConversationPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<IdentityIndexPlugin<SessionId, Session>>() {
            app.add_plugins(IdentityIndexPlugin::<SessionId, Session>::default());
        }
        let index = IdentityIndexSet::of::<SessionId, Session>();
        app.configure_sets(
            Update,
            index
                .after(crate::model::MutationSet)
                .after(ServiceMessageSet),
        )
        .configure_sets(PostUpdate, index.after(crate::model::CanonicalizeSet))
        .add_message::<Ingress>()
        .add_message::<SnapshotReceived>()
        .add_message::<ConversationAppendReceived>()
        .add_message::<OperationReceived>()
        .add_message::<OperationCommitted>()
        .register_stored::<Document>()
        .register_stored::<DocumentKind>()
        .register_stored::<ConversationEvent>()
        .register_stored::<EventIdentity>()
        .register_stored::<MessageContent>()
        .register_stored::<MaterializedEvent>()
        .register_stored::<SnapshotEvent>()
        .register_stored::<NativeAlias>()
        .register_stored::<FileHistory>()
        .register_stored::<ReplayAlias>()
        .register_stored::<SourceBlock>()
        .register_stored::<TargetBlock>()
        .register_stored::<ImportedEvent>()
        .register_stored::<OperationEvent>()
        .register_stored::<Retired>()
        .register_stored::<ProviderKind>()
        .register_stored::<ProviderSessionId>()
        .add_systems(
            Update,
            (ensure, sync_agent)
                .after(index)
                .before(crate::history::Prepare),
        )
        .add_systems(
            Update,
            (collect, execute)
                .chain()
                .after(ServiceMessageSet)
                .after(index)
                .in_set(crate::history::Prepare),
        )
        .add_systems(
            Update,
            (
                accept,
                bevy_ecs::schedule::ApplyDeferred,
                execute,
                cache_transcripts,
            )
                .chain()
                .in_set(crate::history::Ready),
        );
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TranscriptReadySet;

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Member;

#[derive(Component, Clone, Debug, Default, PartialEq, Eq, Reflect)]
#[reflect(Component)]
#[require(Save)]
#[type_path = "vmux_session"]
pub struct ConversationEvent;

#[derive(Component, Clone, Debug, PartialEq, Eq, Reflect, Serialize, Deserialize)]
#[reflect(opaque)]
#[reflect(Component, Serialize, Deserialize)]
#[type_path = "vmux_session"]
pub struct EventIdentity {
    pub event_id: EventId,
    pub actor_id: MemberId,
    pub client_op_id: Option<ClientOpId>,
    pub sequence: u64,
    pub reply_to: Option<EventId>,
}

#[derive(Component, Clone, Debug, PartialEq, Reflect, Serialize, Deserialize)]
#[reflect(opaque)]
#[reflect(Component, Serialize, Deserialize)]
#[type_path = "vmux_session"]
pub struct MessageContent(pub Message);

impl MessageContent {
    fn slots(message: &Message) -> Vec<Option<usize>> {
        match message {
            Message::Assistant { blocks } => (0..blocks.len()).map(Some).collect(),
            _ => vec![None],
        }
    }

    fn same_unit(&self, target: Option<usize>, message: &Message, source: Option<usize>) -> bool {
        match (&self.0, target, message, source) {
            (
                Message::Assistant { blocks: current },
                Some(target),
                Message::Assistant { blocks: incoming },
                Some(source),
            ) => {
                let (Some(current), Some(incoming)) = (current.get(target), incoming.get(source))
                else {
                    return false;
                };
                if current == incoming {
                    return true;
                }
                let (
                    AssistantBlock::ToolUse {
                        call_id: left,
                        args: left_args,
                        ..
                    },
                    AssistantBlock::ToolUse {
                        call_id: right,
                        args: right_args,
                        ..
                    },
                ) = (current, incoming)
                else {
                    return false;
                };
                if left != right {
                    return false;
                }
                if left_args == right_args {
                    return true;
                }
                let Ok(left) = serde_json::from_str::<serde_json::Value>(left_args) else {
                    return false;
                };
                let Ok(right) = serde_json::from_str::<serde_json::Value>(right_args) else {
                    return false;
                };
                left == right
            }
            (_, None, _, None) => Versions::same(&self.0, message),
            _ => false,
        }
    }

    fn replace_unit(&mut self, target: Option<usize>, message: &Message, source: Option<usize>) {
        if self.same_unit(target, message, source) {
            return;
        }
        match (&mut self.0, target, message, source) {
            (
                Message::Assistant { blocks: current },
                Some(target),
                Message::Assistant { blocks: incoming },
                Some(source),
            ) => {
                if let (Some(current), Some(incoming)) =
                    (current.get_mut(target), incoming.get(source))
                {
                    current.clone_from(incoming);
                }
            }
            (current, None, incoming, None) => current.clone_from(incoming),
            _ => {}
        }
    }
}

#[derive(Component, Clone, Debug, Eq, PartialEq)]
pub enum MessageDelivery {
    Pending(ClientOpId),
    Committed,
    Failed(String),
}

#[derive(Component, Clone, Copy, Debug, Default, Eq, PartialEq, Reflect)]
#[reflect(Component)]
#[type_path = "vmux_session"]
pub struct MaterializedEvent;

#[derive(Component, Clone, Copy, Debug, Default, Eq, PartialEq, Reflect)]
#[reflect(Component)]
#[type_path = "vmux_session"]
pub(super) struct SnapshotEvent;

#[derive(Component, Clone, Copy, Debug, Eq, PartialEq, Hash, Reflect, Serialize, Deserialize)]
#[reflect(Component, Serialize, Deserialize)]
#[require(Save)]
#[type_path = "vmux_session"]
pub(super) enum NativeAlias {
    External,
    File {
        generation: u64,
        offset: u64,
        subrecord: usize,
    },
}

#[derive(Component, Clone, Debug, Eq, PartialEq, Reflect, Serialize, Deserialize)]
#[reflect(Component, Serialize, Deserialize)]
#[require(Save)]
#[type_path = "vmux_session"]
pub(super) struct FileHistory {
    identity: String,
    prefix: Vec<u8>,
    length: u64,
    cursor: u64,
    generation: u64,
}

impl FileHistory {
    pub(super) fn facts(&self) -> FileFacts {
        FileFacts {
            identity: self.identity.clone(),
            prefix: self.prefix.clone(),
            length: self.length,
            cursor: self.cursor,
            positions: Vec::new(),
        }
    }

    fn next(&self, facts: &FileFacts) -> Option<Self> {
        let retained = self.identity == facts.identity
            && facts.length >= self.length
            && facts.prefix.starts_with(&self.prefix);
        let generation = if retained {
            self.generation
        } else {
            self.generation.checked_add(1)?
        };
        Some(Self {
            identity: facts.identity.clone(),
            prefix: facts.prefix.clone(),
            length: facts.length,
            cursor: if retained {
                self.cursor.max(facts.cursor)
            } else {
                facts.cursor
            },
            generation,
        })
    }
}

#[derive(SystemParam)]
struct FileSources<'w, 's> {
    files: Query<
        'w,
        's,
        (
            Entity,
            &'static ChildOf,
            &'static ProviderKind,
            &'static ProviderSessionId,
            &'static FileHistory,
        ),
    >,
}

impl FileSources<'_, '_> {
    fn prepare(
        &self,
        session: Entity,
        read: &Read,
        facts: &FileFacts,
    ) -> Option<(Option<Entity>, FileHistory)> {
        for (entity, parent, kind, provider, history) in &self.files {
            if parent.parent() == session
                && kind == &read.source.kind
                && provider == &read.source.id
            {
                return Some((Some(entity), history.next(facts)?));
            }
        }
        Some((
            None,
            FileHistory {
                identity: facts.identity.clone(),
                prefix: facts.prefix.clone(),
                length: facts.length,
                cursor: facts.cursor,
                generation: 1,
            },
        ))
    }
}

#[derive(Component, Clone, Debug, Eq, PartialEq, Reflect, Serialize, Deserialize)]
#[reflect(opaque)]
#[reflect(Component, Serialize, Deserialize)]
#[require(Save)]
#[type_path = "vmux_session"]
struct ReplayAlias(EventId);

#[derive(Component, Clone, Copy, Debug, Eq, PartialEq, Reflect, Serialize, Deserialize)]
#[reflect(Component, Serialize, Deserialize)]
#[type_path = "vmux_session"]
struct SourceBlock(usize);

#[derive(Component, Clone, Copy, Debug, Eq, PartialEq, Reflect, Serialize, Deserialize)]
#[reflect(Component, Serialize, Deserialize)]
#[type_path = "vmux_session"]
struct TargetBlock(usize);

#[derive(Component, Clone, Copy, Debug, Default, Eq, PartialEq, Reflect)]
#[reflect(Component)]
#[type_path = "vmux_session"]
struct ImportedEvent;

#[derive(Component, Clone, Copy, Debug, Default, Eq, PartialEq, Reflect)]
#[reflect(Component)]
#[type_path = "vmux_session"]
struct OperationEvent;

#[derive(Component, Clone, Copy, Debug, Default, Eq, PartialEq, Reflect)]
#[reflect(Component)]
#[type_path = "vmux_session"]
pub(super) struct Retired;

#[derive(Component, Clone, Debug, Default, Eq, PartialEq, Reflect)]
#[reflect(Component)]
#[require(Save)]
#[type_path = "vmux_session"]
pub struct Document;

#[derive(
    Component, Clone, Copy, Debug, Default, Eq, PartialEq, Reflect, Serialize, Deserialize,
)]
#[reflect(Component)]
#[require(Save)]
#[type_path = "vmux_session"]
pub enum DocumentKind {
    #[default]
    Draft,
    Notes,
    Plan,
}

#[derive(Message, Clone, Debug)]
pub struct SnapshotReceived {
    pub session: SessionId,
    pub messages: Vec<Message>,
}

#[derive(Message, Clone, Debug)]
pub struct ConversationAppendReceived {
    pub session: SessionId,
    pub messages: Vec<Message>,
}

#[derive(Message, Clone, Debug)]
pub struct OperationReceived(pub SerializedEvent);

#[derive(Message, Clone, Debug)]
pub struct OperationCommitted(pub SerializedEvent);

#[derive(Message, Clone)]
enum Ingress {
    Snapshot(SnapshotReceived),
    Append(ConversationAppendReceived),
    Operation(SerializedEvent),
    Native {
        operation: Entity,
        read: Read,
        entries: Vec<(Option<String>, Message)>,
        updated_at: i64,
        file: Option<FileFacts>,
    },
}

#[derive(SystemParam)]
struct AcceptedReads<'w, 's> {
    reads: Query<'w, 's, (&'static ChildOf, &'static Read, Has<Applied>)>,
    sessions: Query<
        'w,
        's,
        (
            &'static SessionId,
            &'static ProviderKind,
            &'static ProviderSessionId,
            &'static EntityTarget<Read>,
            Option<&'static RunState>,
        ),
        With<Session>,
    >,
    identities: Res<'w, IdentityIndex<SessionId, Session>>,
}

impl AcceptedReads<'_, '_> {
    fn read(&self, operation: Entity) -> Option<(Entity, Read)> {
        let (parent, read, applied) = self.reads.get(operation).ok()?;
        if applied {
            return None;
        }
        let entity = parent.parent();
        let (id, kind, provider, pending, state) = self.sessions.get(entity).ok()?;
        if self.identities.entity(id) != Ok(Some(entity))
            || id != &read.session
            || kind != &read.source.kind
            || provider != &read.source.id
            || pending.entity() != operation
        {
            return None;
        }
        if read.failed && !matches!(state, Some(RunState::Errored(_))) {
            return None;
        }
        Some((entity, read.clone()))
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Transcript {
    pub messages: Vec<Message>,
    pub created_at: Vec<u64>,
}

impl Transcript {
    fn from_items(mut items: Vec<(u64, Message, u64)>) -> Self {
        items.sort_by_key(|(sequence, _, _)| *sequence);
        let mut messages = Vec::with_capacity(items.len());
        let mut created_at = Vec::with_capacity(items.len());
        for (_, message, timestamp) in items {
            messages.push(message);
            created_at.push(timestamp);
        }
        Self {
            messages,
            created_at,
        }
    }
}

#[derive(Component, Default)]
pub(super) struct TranscriptCache {
    transcript: Transcript,
    order: Vec<(Entity, u64)>,
    indices: bevy_ecs::entity::EntityHashMap<usize>,
}

#[derive(SystemParam)]
pub struct Transcripts<'w, 's> {
    children: Query<'w, 's, &'static Children>,
    events: Query<
        'w,
        's,
        (
            &'static EventIdentity,
            &'static MessageContent,
            &'static CreatedAt,
        ),
        (With<ConversationEvent>, Without<Retired>),
    >,
    cache: Query<'w, 's, &'static TranscriptCache>,
}

impl Transcripts<'_, '_> {
    pub fn get(&self, session: Entity) -> Transcript {
        self.read(session, Clone::clone)
    }

    pub fn read<R>(&self, session: Entity, read: impl FnOnce(&Transcript) -> R) -> R {
        if let Ok(cache) = self.cache.get(session) {
            return read(&cache.transcript);
        }
        let transcript = self.materialize(session);
        read(&transcript)
    }

    fn materialize(&self, session: Entity) -> Transcript {
        let items = self
            .children
            .get(session)
            .ok()
            .into_iter()
            .flat_map(|children| children.iter())
            .filter_map(|child| {
                let (identity, content, created_at) = self.events.get(child).ok()?;
                Some((
                    identity.sequence,
                    content.0.clone(),
                    created_at.0.max(0) as u64,
                ))
            })
            .collect();
        Transcript::from_items(items)
    }
}

type ChangedTranscriptEvents<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static ChildOf),
    (
        With<ConversationEvent>,
        Or<(
            Added<ConversationEvent>,
            Changed<EventIdentity>,
            Changed<MessageContent>,
            Changed<CreatedAt>,
        )>,
    ),
>;

#[derive(SystemParam)]
struct TranscriptEvents<'w, 's> {
    events: Query<
        'w,
        's,
        (
            &'static EventIdentity,
            &'static MessageContent,
            &'static CreatedAt,
        ),
        (With<ConversationEvent>, Without<Retired>),
    >,
}

impl TranscriptEvents<'_, '_> {
    fn rebuild(&self, cache: &mut TranscriptCache, children: &Children) {
        let mut items = children
            .iter()
            .filter_map(|entity| {
                let (identity, content, created_at) = self.events.get(entity).ok()?;
                Some((
                    entity,
                    identity.sequence,
                    content.0.clone(),
                    created_at.0.max(0) as u64,
                ))
            })
            .collect::<Vec<_>>();
        items.sort_by_key(|(_, sequence, _, _)| *sequence);
        cache.order.clear();
        cache.indices.clear();
        cache.transcript.messages.clear();
        cache.transcript.created_at.clear();
        cache.order.reserve(items.len());
        cache.indices.reserve(items.len());
        cache.transcript.messages.reserve(items.len());
        cache.transcript.created_at.reserve(items.len());
        for (entity, sequence, message, created_at) in items {
            cache.indices.insert(entity, cache.order.len());
            cache.order.push((entity, sequence));
            cache.transcript.messages.push(message);
            cache.transcript.created_at.push(created_at);
        }
    }

    fn membership(&self, cache: &TranscriptCache, children: &Children) -> bool {
        let mut count = 0;
        for entity in children.iter() {
            let Ok((identity, _, _)) = self.events.get(entity) else {
                continue;
            };
            let Some(position) = cache.indices.get(&entity).copied() else {
                return false;
            };
            if cache.order[position].1 != identity.sequence {
                return false;
            }
            count += 1;
        }
        count == cache.order.len()
    }

    fn update(&self, cache: &mut TranscriptCache, entity: Entity) -> bool {
        let Some(index) = cache.indices.get(&entity).copied() else {
            return false;
        };
        let Ok((identity, content, created_at)) = self.events.get(entity) else {
            return false;
        };
        if cache.order[index].1 != identity.sequence {
            return false;
        }
        cache.transcript.messages[index].clone_from(&content.0);
        cache.transcript.created_at[index] = created_at.0.max(0) as u64;
        true
    }
}

fn cache_transcripts(
    changed_events: ChangedTranscriptEvents,
    projection: TranscriptEvents,
    changed_children: Query<Entity, (With<Session>, Changed<Children>)>,
    uncached: Query<Entity, (With<Session>, With<Children>, Without<TranscriptCache>)>,
    mut sessions: Query<(Ref<Children>, Option<&mut TranscriptCache>), With<Session>>,
    mut commands: Commands,
) {
    let mut changed = HashMap::<Entity, Vec<Entity>>::new();
    for (entity, parent) in &changed_events {
        changed.entry(parent.parent()).or_default().push(entity);
    }
    let mut targets = changed
        .keys()
        .copied()
        .collect::<bevy_ecs::entity::EntityHashSet>();
    targets.extend(changed_children.iter());
    targets.extend(uncached.iter());
    for session in targets {
        let Ok((children, cache)) = sessions.get_mut(session) else {
            continue;
        };
        let changes = changed.remove(&session).unwrap_or_default();
        let Some(mut cache) = cache else {
            let mut cache = TranscriptCache::default();
            projection.rebuild(&mut cache, &children);
            commands.entity(session).try_insert(cache);
            continue;
        };
        if (children.is_changed() && !projection.membership(&cache, &children))
            || changes
                .iter()
                .any(|entity| !projection.update(&mut cache, *entity))
        {
            projection.rebuild(&mut cache, &children);
        }
    }
}

impl EventIdentity {
    fn from_event(event: &SerializedEvent) -> Self {
        Self {
            event_id: event.event_id.clone(),
            actor_id: event.actor_id.clone(),
            client_op_id: event.client_op_id.clone(),
            sequence: event.server_seq,
            reply_to: event.reply_to.clone(),
        }
    }
}

fn ensure(
    sessions: Query<(Entity, &SessionId, Option<&Children>), Added<Session>>,
    members: Query<&MemberKind, With<Member>>,
    documents: Query<(), With<Document>>,
    mut commands: Commands,
) {
    for (session, id, children) in &sessions {
        let prefix = format!("session:{}", id.0);
        let has_local = children
            .into_iter()
            .flat_map(|children| children.iter())
            .any(|child| {
                members
                    .get(child)
                    .is_ok_and(|kind| *kind == MemberKind::Human)
            });
        if !has_local {
            commands.spawn((
                Member,
                MemberId::local(id),
                Name::new("You"),
                MemberRole::Owner,
                MemberKind::Human,
                ChildOf(session),
            ));
        }
        let has_document = children
            .into_iter()
            .flat_map(|children| children.iter())
            .any(|child| documents.contains(child));
        if !has_document {
            commands.spawn((
                Document,
                DocumentKind::Draft,
                Name::new(format!("{prefix}:draft")),
                ChildOf(session),
            ));
        }
    }
}

fn sync_agent(
    sessions: Query<(Entity, &SessionId, &AgentId, Option<&Children>), Changed<AgentId>>,
    mut members: Query<(&mut MemberId, &mut Name, &mut MemberRole, &MemberKind), With<Member>>,
    mut commands: Commands,
) {
    for (session, session_id, agent_id, children) in &sessions {
        let mut found = false;
        for child in children.into_iter().flat_map(|children| children.iter()) {
            let Ok((mut id, mut name, mut role, kind)) = members.get_mut(child) else {
                continue;
            };
            if *kind != MemberKind::Agent {
                continue;
            }
            *id = MemberId::agent(session_id);
            *name = Name::new(agent_id.0.clone());
            *role = MemberRole::Participant;
            found = true;
        }
        if !found {
            commands.spawn((
                Member,
                MemberId::agent(session_id),
                Name::new(agent_id.0.clone()),
                MemberRole::Participant,
                MemberKind::Agent,
                ChildOf(session),
            ));
        }
    }
}

fn collect(
    mut snapshots: MessageReader<SnapshotReceived>,
    mut appended: MessageReader<ConversationAppendReceived>,
    mut received: MessageReader<OperationReceived>,
    mut committed: MessageReader<OperationCommitted>,
    mut ingress: MessageWriter<Ingress>,
) {
    for request in snapshots.read() {
        ingress.write(Ingress::Snapshot(request.clone()));
    }
    for request in appended.read() {
        ingress.write(Ingress::Append(request.clone()));
    }
    for request in received.read() {
        ingress.write(Ingress::Operation(request.0.clone()));
    }
    for request in committed.read() {
        ingress.write(Ingress::Operation(request.0.clone()));
    }
}

fn execute(world: &mut World) {
    let requests: Vec<_> = world.resource_mut::<Messages<Ingress>>().drain().collect();
    for request in requests {
        let id = match &request {
            Ingress::Snapshot(request) => request.session.clone(),
            Ingress::Append(request) => request.session.clone(),
            Ingress::Operation(event) => event.session_id.clone(),
            Ingress::Native { read, .. } => read.session.clone(),
        };
        let reconcile = matches!(&request, Ingress::Snapshot(_) | Ingress::Native { .. });
        let mut history = SystemState::<SessionHistory>::new(world);
        let mut index = match history.get(world) {
            Ok(history) => history.index(&HashSet::from([id.clone()]), reconcile),
            Err(_) => continue,
        };
        let Some(session) = index.session(&id) else {
            continue;
        };
        let mut writer = SystemState::<EventMaterializer>::new(world);
        match request {
            Ingress::Snapshot(request) => {
                let mut stale = index.snapshot_events.remove(&session).unwrap_or_default();
                let now = vmux_ecs::UnixMillis::now().0.max(0) as u64;
                for event in SerializedEvent::from_messages(&id, now, &request.messages) {
                    let ids = writer
                        .get_mut(world)
                        .expect("canonical materializer")
                        .replay(&mut index, event);
                    for id in ids {
                        stale.remove(&id);
                    }
                    writer.apply(world);
                }
                for (_, (entity, _)) in stale {
                    world.despawn(entity);
                }
            }
            Ingress::Append(request) => {
                for message in request.messages {
                    writer
                        .get_mut(world)
                        .expect("canonical materializer")
                        .append(&mut index, &id, &[message]);
                    writer.apply(world);
                }
            }
            Ingress::Operation(event) => {
                let _ = writer
                    .get_mut(world)
                    .expect("canonical materializer")
                    .apply(
                        &mut index,
                        event,
                        MessageDelivery::Committed,
                        MaterializationSource::Operation,
                    );
                writer.apply(world);
            }
            Ingress::Native {
                operation,
                read,
                entries,
                updated_at,
                file,
            } => {
                let mut file_sources = SystemState::<FileSources>::new(world);
                let file_state = if let Some(facts) = &file {
                    let prepared = file_sources
                        .get(world)
                        .expect("canonical file source")
                        .prepare(session, &read, facts);
                    let Some(prepared) = prepared else {
                        world.entity_mut(operation).insert(Applied);
                        world.write_message(Committed {
                            operation,
                            result: Err("history file generation exhausted".into()),
                        });
                        continue;
                    };
                    Some(prepared)
                } else {
                    None
                };
                let mut identified = Vec::with_capacity(entries.len());
                for (position, (external, message)) in entries.into_iter().enumerate() {
                    let identity = if let Some(external) = external {
                        Some((NativeAlias::External, external))
                    } else if let (Some(facts), Some((_, state))) = (&file, &file_state) {
                        let (offset, subrecord) = facts.positions[position];
                        Some((
                            NativeAlias::File {
                                generation: state.generation,
                                offset,
                                subrecord,
                            },
                            "file".into(),
                        ))
                    } else {
                        None
                    };
                    identified.push((identity, message));
                }
                let entries = identified;
                let mut required = 0u64;
                let mut added = HashSet::new();
                for (id, message) in &entries {
                    if let Some(id) = id {
                        let key = (session, read.source.clone(), id.0, id.1.clone());
                        if let Some(bindings) = index.native_aliases.get(&key) {
                            let covered = bindings
                                .iter()
                                .filter_map(|(_, source, _)| *source)
                                .max()
                                .map_or(0, |last| last + 1);
                            let spread = bindings.first().is_some_and(|(first, _, _)| {
                                bindings.iter().any(|(entity, _, _)| entity != first)
                            });
                            if spread && MessageContent::slots(message).len() > covered {
                                required = required.saturating_add(1);
                            }
                            continue;
                        }
                        if !added.insert(id) {
                            continue;
                        }
                    }
                    required = required.saturating_add(1);
                }
                if index
                    .latest_sequences
                    .get(&session)
                    .copied()
                    .unwrap_or_default()
                    .checked_add(required)
                    .is_none()
                {
                    world.entity_mut(operation).insert(Applied);
                    world.write_message(Committed {
                        operation,
                        result: Err("history sequence exhausted".into()),
                    });
                    continue;
                }
                let timestamp = if entries.iter().any(|(id, _)| id.is_some()) {
                    updated_at.max(0) as u64
                } else {
                    vmux_ecs::UnixMillis::now().0.max(0) as u64
                };
                for (external, message) in entries {
                    writer
                        .get_mut(world)
                        .expect("canonical materializer")
                        .native(&mut index, &read, external.as_ref(), &message, timestamp);
                    writer.apply(world);
                }
                if let Some((entity, state)) = file_state {
                    if let Some(entity) = entity {
                        world.entity_mut(entity).insert(state);
                    } else {
                        world.spawn((state, read.source.kind, read.source.id, ChildOf(session)));
                    }
                }
                if let Ok(mut operation_entity) = world.get_entity_mut(operation) {
                    operation_entity.insert(Applied);
                }
                world.write_message(Committed {
                    operation,
                    result: Ok(true),
                });
            }
        }
    }
}

fn accept(
    mut received: MessageReader<Received>,
    reads: AcceptedReads,
    versions: Versions,
    mut ingress: MessageWriter<Ingress>,
    mut committed: MessageWriter<Committed>,
    mut commands: Commands,
) {
    for request in received.read() {
        if reads
            .reads
            .get(request.operation)
            .is_ok_and(|(_, _, applied)| applied)
        {
            continue;
        }
        let Some((session, read)) = reads.read(request.operation) else {
            committed.write(Committed {
                operation: request.operation,
                result: Ok(false),
            });
            continue;
        };
        if versions.fingerprint(session) != read.fingerprint {
            committed.write(Committed {
                operation: request.operation,
                result: Ok(false),
            });
            continue;
        }
        if let Some(error) = &request.error {
            commands.entity(request.operation).insert(Applied);
            committed.write(Committed {
                operation: request.operation,
                result: Err(error.clone()),
            });
            continue;
        }
        let mut entries = request.entries.clone();
        if let Some(file) = &request.file {
            if file.prefix.len() > 4096
                || file.cursor > file.length
                || file.positions.len() != entries.len()
                || file.positions.windows(2).any(|pair| pair[0] >= pair[1])
                || file
                    .positions
                    .iter()
                    .any(|(offset, _)| *offset >= file.cursor)
            {
                commands.entity(request.operation).insert(Applied);
                committed.write(Committed {
                    operation: request.operation,
                    result: Err("invalid native file positions".into()),
                });
                continue;
            }
        } else if request.tail && entries.iter().all(|(id, _)| id.is_none()) {
            let mut messages = Vec::with_capacity(entries.len());
            for (_, message) in entries {
                messages.push(message);
            }
            entries = Vec::new();
            for message in versions.unseen(session, messages) {
                entries.push((None, message));
            }
        } else if request.tail && entries.iter().any(|(id, _)| id.is_none()) {
            commands.entity(request.operation).insert(Applied);
            committed.write(Committed {
                operation: request.operation,
                result: Err("native tail requires an identity for every entry".into()),
            });
            continue;
        }
        ingress.write(Ingress::Native {
            operation: request.operation,
            read,
            entries,
            updated_at: request.updated_at,
            file: request.file.clone(),
        });
    }
}

#[derive(SystemParam)]
struct SessionHistory<'w, 's> {
    sessions: Query<
        'w,
        's,
        (
            Entity,
            &'static SessionId,
            Option<&'static Children>,
            Option<&'static ProviderKind>,
            Option<&'static ProviderSessionId>,
        ),
        With<Session>,
    >,
    identities: Res<'w, IdentityIndex<SessionId, Session>>,
    existing: Query<
        'w,
        's,
        (
            Entity,
            &'static EventIdentity,
            &'static MessageContent,
            &'static CreatedAt,
            Has<SnapshotEvent>,
            Option<&'static ProviderKind>,
            Option<&'static ProviderSessionId>,
        ),
        (With<ConversationEvent>, With<MaterializedEvent>),
    >,
    aliases: Query<
        'w,
        's,
        (
            &'static ChildOf,
            &'static Name,
            &'static NativeAlias,
            &'static ProviderKind,
            &'static ProviderSessionId,
            Option<&'static SourceBlock>,
            Option<&'static TargetBlock>,
        ),
        With<NativeAlias>,
    >,
    replays: Query<
        'w,
        's,
        (
            &'static ChildOf,
            &'static ReplayAlias,
            Option<&'static ProviderKind>,
            Option<&'static ProviderSessionId>,
            Option<&'static SourceBlock>,
            Option<&'static TargetBlock>,
        ),
    >,
}

impl SessionHistory<'_, '_> {
    fn index(&self, requested: &HashSet<SessionId>, reconcile: bool) -> EventIndex {
        let mut index = EventIndex {
            sessions: HashMap::new(),
            events: HashMap::new(),
            operations: HashMap::new(),
            entity_keys: HashMap::new(),
            timestamps: HashMap::new(),
            snapshot_events: HashMap::new(),
            latest_sequences: HashMap::new(),
            latest_users: HashMap::new(),
            native_aliases: HashMap::new(),
            native_bound: HashSet::new(),
            identities: HashMap::new(),
            native_positions: HashMap::new(),
            event_owners: HashMap::new(),
            units: HashMap::new(),
            claimed_units: HashSet::new(),
            replay_aliases: HashMap::new(),
            replay_positions: HashMap::new(),
            sources: HashMap::new(),
            origins: HashMap::new(),
            unit_positions: HashMap::new(),
        };
        for (session, id, children, kind, provider) in &self.sessions {
            if !requested.contains(id) || self.identities.entity(id) != Ok(Some(session)) {
                continue;
            }
            index.sessions.insert(id.clone(), session);
            if let (Some(kind), Some(provider)) = (kind, provider) {
                index
                    .sources
                    .insert(session, ProviderSessionKey::from_components(kind, provider));
            }
            for child in children.into_iter().flat_map(|children| children.iter()) {
                let Ok((entity, identity, message, timestamp, from_snapshot, kind, provider)) =
                    self.existing.get(child)
                else {
                    continue;
                };
                index
                    .events
                    .insert((session, identity.event_id.clone()), entity);
                if let Some(client_op_id) = identity.client_op_id.clone() {
                    index.operations.insert((session, client_op_id), entity);
                }
                index.entity_keys.insert(
                    entity,
                    (identity.event_id.clone(), identity.client_op_id.clone()),
                );
                index.timestamps.insert(entity, *timestamp);
                index.identities.insert(entity, identity.clone());
                index.event_owners.insert(entity, session);
                if let (Some(kind), Some(provider)) = (kind, provider) {
                    index
                        .origins
                        .insert(entity, ProviderSessionKey::from_components(kind, provider));
                }
                if reconcile {
                    let slots = index.units.entry(session).or_default();
                    match &message.0 {
                        Message::Assistant { blocks } => {
                            for slot in 0..blocks.len() {
                                slots.push((entity, Some(slot)));
                            }
                        }
                        _ => slots.push((entity, None)),
                    }
                }
                index.record_sequence(session, identity, &message.0);
                if from_snapshot {
                    index.snapshot_events.entry(session).or_default().insert(
                        identity.event_id.clone(),
                        (entity, identity.client_op_id.clone()),
                    );
                }
            }
        }
        if !reconcile {
            return index;
        }
        for slots in index.units.values_mut() {
            slots.sort_by_key(|(entity, slot)| (index.identities[entity].sequence, *slot));
        }
        for slots in index.units.values() {
            for (position, slot) in slots.iter().enumerate() {
                index.unit_positions.insert(*slot, position);
            }
        }
        for (parent, name, alias, kind, provider, source_slot, target_slot) in &self.aliases {
            let entity = parent.parent();
            let Some(session) = index.event_owners.get(&entity).copied() else {
                continue;
            };
            let source = ProviderSessionKey::from_components(kind, provider);
            let source_slot = source_slot.map(|slot| slot.0);
            let target_slot = target_slot.map(|slot| slot.0);
            index
                .native_aliases
                .entry((session, source, *alias, name.as_str().to_string()))
                .or_default()
                .push((entity, source_slot, target_slot));
            index.claimed_units.insert((entity, target_slot));
            index.native_bound.insert(entity);
        }
        for (parent, alias, kind, provider, source_slot, target_slot) in &self.replays {
            let entity = parent.parent();
            let Some(session) = index.event_owners.get(&entity).copied() else {
                continue;
            };
            let source = match (kind, provider) {
                (Some(kind), Some(provider)) => {
                    Some(ProviderSessionKey::from_components(kind, provider))
                }
                _ => None,
            };
            index
                .replay_aliases
                .entry((session, source, alias.0.clone()))
                .or_default()
                .push((
                    entity,
                    source_slot.map(|slot| slot.0),
                    target_slot.map(|slot| slot.0),
                ));
        }
        for aliases in index.native_aliases.values_mut() {
            aliases.sort_by_key(|(_, source, _)| *source);
        }
        for aliases in index.replay_aliases.values_mut() {
            aliases.sort_by_key(|(_, source, _)| *source);
        }
        index
    }
}

struct EventIndex {
    sessions: HashMap<SessionId, Entity>,
    events: HashMap<(Entity, EventId), Entity>,
    operations: HashMap<(Entity, ClientOpId), Entity>,
    entity_keys: HashMap<Entity, (EventId, Option<ClientOpId>)>,
    timestamps: HashMap<Entity, CreatedAt>,
    snapshot_events: HashMap<Entity, HashMap<EventId, (Entity, Option<ClientOpId>)>>,
    latest_sequences: HashMap<Entity, u64>,
    latest_users: HashMap<Entity, (u64, EventId)>,
    native_aliases: HashMap<
        (Entity, ProviderSessionKey, NativeAlias, String),
        Vec<(Entity, Option<usize>, Option<usize>)>,
    >,
    native_bound: HashSet<Entity>,
    identities: HashMap<Entity, EventIdentity>,
    native_positions: HashMap<Entity, usize>,
    event_owners: HashMap<Entity, Entity>,
    units: HashMap<Entity, Vec<(Entity, Option<usize>)>>,
    claimed_units: HashSet<(Entity, Option<usize>)>,
    replay_aliases: HashMap<
        (Entity, Option<ProviderSessionKey>, EventId),
        Vec<(Entity, Option<usize>, Option<usize>)>,
    >,
    replay_positions: HashMap<Entity, usize>,
    sources: HashMap<Entity, ProviderSessionKey>,
    origins: HashMap<Entity, ProviderSessionKey>,
    unit_positions: HashMap<(Entity, Option<usize>), usize>,
}

impl EventIndex {
    fn session(&self, id: &SessionId) -> Option<Entity> {
        self.sessions.get(id).copied()
    }

    fn record_sequence(&mut self, session: Entity, identity: &EventIdentity, message: &Message) {
        let sequence = self.latest_sequences.entry(session).or_default();
        *sequence = (*sequence).max(identity.sequence);
        if matches!(message, Message::User { .. })
            && self
                .latest_users
                .get(&session)
                .is_none_or(|(latest, _)| *latest <= identity.sequence)
        {
            self.latest_users
                .insert(session, (identity.sequence, identity.event_id.clone()));
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum MaterializationSource {
    Snapshot,
    Operation,
    Native,
}

#[derive(SystemParam)]
struct EventMaterializer<'w, 's> {
    commands: Commands<'w, 's>,
    existing: Query<'w, 's, &'static mut MessageContent, With<ConversationEvent>>,
    targets: Query<'w, 's, (Entity, &'static ChildOf, &'static TargetBlock)>,
    native_records: Query<
        'w,
        's,
        (
            Entity,
            &'static ChildOf,
            &'static Name,
            &'static NativeAlias,
            &'static ProviderKind,
            &'static ProviderSessionId,
        ),
        With<NativeAlias>,
    >,
    replay_records: Query<
        'w,
        's,
        (
            Entity,
            &'static ChildOf,
            &'static ReplayAlias,
            Option<&'static ProviderKind>,
            Option<&'static ProviderSessionId>,
        ),
    >,
    imported: Query<'w, 's, (), With<ImportedEvent>>,
    protected: Query<'w, 's, (), With<OperationEvent>>,
    retired: Query<'w, 's, (), With<Retired>>,
}

impl EventMaterializer<'_, '_> {
    fn native(
        &mut self,
        index: &mut EventIndex,
        read: &Read,
        identity: Option<&(NativeAlias, String)>,
        message: &Message,
        created_at: u64,
    ) {
        let Some(session) = index.session(&read.session) else {
            return;
        };
        let key = identity.map(|(alias, id)| (session, read.source.clone(), *alias, id.clone()));
        let mut bindings = key
            .as_ref()
            .and_then(|key| index.native_aliases.get(key))
            .cloned()
            .unwrap_or_default();
        let previous = bindings.clone();
        if bindings.is_empty() && identity.is_some() {
            let after = index
                .native_positions
                .get(&session)
                .copied()
                .unwrap_or_default();
            if let Some((found, next)) =
                self.match_units(index, (session, Some(&read.source), after, true), message)
            {
                bindings = found;
                index.native_positions.insert(session, next);
            }
        }
        if bindings.is_empty() {
            let sequence = index
                .latest_sequences
                .get(&session)
                .copied()
                .unwrap_or_default()
                .checked_add(1)
                .expect("preflight bounded history sequence");
            let id = EventId::new(format!(
                "session:{}:event:{sequence}",
                read.session.as_str()
            ));
            let is_user = matches!(message, Message::User { .. });
            let _ = self.apply(
                index,
                SerializedEvent {
                    event_id: id.clone(),
                    session_id: read.session.clone(),
                    actor_id: if is_user {
                        MemberId::local(&read.session)
                    } else {
                        MemberId::agent(&read.session)
                    },
                    client_op_id: None,
                    server_seq: sequence,
                    created_at_ms: created_at,
                    reply_to: if is_user {
                        None
                    } else {
                        index.latest_users.get(&session).map(|(_, id)| id.clone())
                    },
                    message: message.clone(),
                },
                MessageDelivery::Committed,
                MaterializationSource::Native,
            );
            let entity = index.events[&(session, id)];
            for slot in MessageContent::slots(message) {
                bindings.push((entity, slot, slot));
            }
        } else {
            self.resize(index, &mut bindings, message, true);
            self.patch(index, &mut bindings, message);
        }
        let cursor = index.native_positions.entry(session).or_default();
        for (entity, _, target) in &bindings {
            if let Some(position) = index.unit_positions.get(&(*entity, *target)) {
                *cursor = (*cursor).max(position + 1);
            }
        }
        for (entity, _, target) in &bindings {
            if let Some(identity) = index.identities.get(entity)
                && let Some(events) = index.snapshot_events.get_mut(&session)
            {
                events.remove(&identity.event_id);
            }
            self.commands.entity(*entity).remove::<SnapshotEvent>();
            index.native_bound.insert(*entity);
            index.claimed_units.insert((*entity, *target));
        }
        if let Some(key) = key
            && (!index.native_aliases.contains_key(&key) || previous != bindings)
        {
            for (alias, parent, name, source_alias, kind, provider) in &self.native_records {
                if name.as_str() == key.3
                    && source_alias == &key.2
                    && kind == &key.1.kind
                    && provider == &key.1.id
                    && index.event_owners.get(&parent.parent()) == Some(&session)
                {
                    self.commands.entity(alias).despawn();
                }
            }
            for (entity, source_slot, target_slot) in &bindings {
                let mut alias = self.commands.spawn((
                    key.2,
                    Name::new(key.3.clone()),
                    read.source.kind.clone(),
                    read.source.id.clone(),
                    ChildOf(*entity),
                ));
                if let Some(slot) = source_slot {
                    alias.insert(SourceBlock(*slot));
                }
                if let Some(slot) = target_slot {
                    alias.insert(TargetBlock(*slot));
                }
            }
            index.native_aliases.insert(key, bindings);
        }
    }

    fn match_units(
        &self,
        index: &EventIndex,
        selection: (Entity, Option<&ProviderSessionKey>, usize, bool),
        message: &Message,
    ) -> Option<(Vec<(Entity, Option<usize>, Option<usize>)>, usize)> {
        let (session, source, after, unclaimed) = selection;
        let candidates = index.units.get(&session)?;
        let incoming = MessageContent::slots(message);
        if incoming.is_empty() {
            return None;
        }
        for start in after..candidates.len() {
            let Some(window) = candidates.get(start..start + incoming.len()) else {
                break;
            };
            let mut bindings = Vec::with_capacity(incoming.len());
            for ((entity, target), source_slot) in window.iter().zip(&incoming) {
                if unclaimed && index.claimed_units.contains(&(*entity, *target)) {
                    break;
                }
                if index
                    .origins
                    .get(entity)
                    .is_some_and(|origin| Some(origin) != source)
                {
                    break;
                }
                let Ok(content) = self.existing.get(*entity) else {
                    break;
                };
                if !content.same_unit(*target, message, *source_slot) {
                    break;
                }
                bindings.push((*entity, *source_slot, *target));
            }
            if bindings.len() == incoming.len() {
                return Some((bindings, start + incoming.len()));
            }
        }
        None
    }

    fn visibility(&mut self, entity: Entity, retired: bool) {
        if self.retired.get(entity).is_ok() == retired {
            return;
        }
        let Ok(content) = self.existing.get(entity) else {
            return;
        };
        let content = content.clone();
        if retired {
            self.commands.entity(entity).insert((Retired, content));
        } else {
            self.commands
                .entity(entity)
                .remove::<Retired>()
                .insert(content);
        }
    }

    fn resize(
        &mut self,
        index: &mut EventIndex,
        bindings: &mut Vec<(Entity, Option<usize>, Option<usize>)>,
        message: &Message,
        native: bool,
    ) {
        let Message::Assistant { blocks: incoming } = message else {
            return;
        };
        let Some((entity, _, _)) = bindings.first().copied() else {
            return;
        };
        if self.protected.get(entity).is_ok()
            && bindings.iter().all(|(current, _, _)| *current == entity)
        {
            return;
        }
        if incoming.len() == bindings.len()
            && bindings.iter().all(|(entity, source, target)| {
                self.existing
                    .get(*entity)
                    .is_ok_and(|content| content.same_unit(*target, message, *source))
            })
        {
            return;
        }

        if bindings.iter().all(|(current, _, _)| *current == entity)
            && bindings.iter().any(|(_, _, target)| target.is_none())
        {
            for source in 0..incoming.len() {
                if !bindings
                    .iter()
                    .any(|(_, current, _)| *current == Some(source))
                {
                    bindings.push((entity, Some(source), None));
                }
            }
            return;
        }
        if native && bindings.iter().any(|(current, _, _)| *current != entity) {
            self.trim(index, bindings, incoming.len());
        }
        if bindings
            .iter()
            .any(|(current, _, target)| *current != entity || target.is_none())
        {
            let covered = bindings
                .iter()
                .filter_map(|(_, source, _)| *source)
                .max()
                .map_or(0, |last| last + 1);
            if covered >= incoming.len() {
                return;
            }
            let Some(session) = index.event_owners.get(&entity).copied() else {
                return;
            };
            let Some((session_id, _)) = index
                .sessions
                .iter()
                .find(|(_, current)| **current == session)
            else {
                return;
            };
            let session_id = session_id.clone();
            let Some(sequence) = index
                .latest_sequences
                .get(&session)
                .copied()
                .unwrap_or_default()
                .checked_add(1)
            else {
                return;
            };
            let event_id =
                EventId::new(format!("session:{}:event:{sequence}", session_id.as_str()));
            let mut appended = Vec::with_capacity(incoming.len() - covered);
            for block in &incoming[covered..] {
                appended.push(block.clone());
            }
            let event = SerializedEvent {
                event_id,
                session_id: session_id.clone(),
                actor_id: MemberId::agent(&session_id),
                client_op_id: None,
                server_seq: sequence,
                created_at_ms: index
                    .timestamps
                    .get(&entity)
                    .map_or(0, |time| time.0.max(0) as u64),
                reply_to: index.latest_users.get(&session).map(|(_, id)| id.clone()),
                message: Message::Assistant { blocks: appended },
            };
            let source = if native {
                MaterializationSource::Native
            } else {
                MaterializationSource::Snapshot
            };
            let Some(new_entity) = self.apply(index, event, MessageDelivery::Committed, source)
            else {
                return;
            };
            for source_slot in covered..incoming.len() {
                bindings.push((new_entity, Some(source_slot), Some(source_slot - covered)));
            }
            return;
        }
        let Ok(content) = self.existing.get(entity) else {
            return;
        };
        let Message::Assistant { blocks: current } = &content.0 else {
            return;
        };
        let mut targets = Vec::with_capacity(bindings.len());
        for (_, _, target) in bindings.iter() {
            targets.push(target.unwrap());
        }
        targets.sort_unstable();
        targets.dedup();
        let Some(start) = targets.first().copied() else {
            return;
        };
        let end = targets.last().copied().unwrap() + 1;
        if end > current.len() || end - start != targets.len() {
            return;
        }
        if incoming.len() == targets.len() {
            let mut reordered = false;
            for (_, source, target) in bindings.iter() {
                if content.same_unit(*target, message, *source) {
                    continue;
                }
                for (_, _, other_target) in bindings.iter() {
                    if other_target != target && content.same_unit(*other_target, message, *source)
                    {
                        reordered = true;
                        break;
                    }
                }
                if reordered {
                    break;
                }
            }
            if !reordered {
                return;
            }
        }
        if !native && index.native_bound.contains(&entity) && incoming.len() < targets.len() {
            return;
        }
        let old = content;
        let mut replaced = current.clone();
        replaced.splice(start..end, incoming.iter().cloned());
        let next = Message::Assistant { blocks: replaced };
        let mut mapping = Vec::with_capacity(current.len());
        let mut consumed = HashSet::new();
        for old_slot in 0..current.len() {
            if old_slot < start {
                mapping.push(Some(old_slot));
                continue;
            }
            if old_slot >= end {
                mapping.push(Some(old_slot - (end - start) + incoming.len()));
                continue;
            }
            let mut found = None;
            for new_slot in start..start + incoming.len() {
                if consumed.contains(&new_slot) {
                    continue;
                }
                if old.same_unit(Some(old_slot), &next, Some(new_slot)) {
                    found = Some(new_slot);
                    break;
                }
            }
            if found.is_none() && old_slot - start < incoming.len() {
                found = Some(start + old_slot - start);
            }
            if let Some(slot) = found {
                consumed.insert(slot);
            }
            mapping.push(found);
        }
        if let Ok(mut content) = self.existing.get_mut(entity)
            && content.0 != next
        {
            content.0 = next;
        }
        self.remap(index, entity, &mapping);
        bindings.clear();
        for slot in 0..incoming.len() {
            bindings.push((entity, Some(slot), Some(start + slot)));
        }
    }

    fn trim(
        &mut self,
        index: &mut EventIndex,
        bindings: &mut [(Entity, Option<usize>, Option<usize>)],
        length: usize,
    ) {
        let mut removed = HashMap::<Entity, HashSet<usize>>::new();
        for (entity, source, target) in bindings.iter() {
            if source.is_none_or(|slot| slot < length)
                || self.imported.get(*entity).is_err()
                || self.protected.get(*entity).is_ok()
            {
                continue;
            }
            let Some(slot) = *target else {
                continue;
            };
            let mut shared = false;
            for other in index.native_aliases.values() {
                if other == bindings {
                    continue;
                }
                for (owner, source, target) in other {
                    if owner == entity && (source.is_none() || *target == Some(slot)) {
                        shared = true;
                        break;
                    }
                }
                if shared {
                    break;
                }
            }
            if !shared {
                removed.entry(*entity).or_default().insert(slot);
            }
        }
        for (entity, removed) in removed {
            let Ok(content) = self.existing.get(entity) else {
                continue;
            };
            let Message::Assistant { blocks } = &content.0 else {
                continue;
            };
            let mut next = Vec::with_capacity(blocks.len().saturating_sub(removed.len()));
            let mut mapping = Vec::with_capacity(blocks.len());
            for (slot, block) in blocks.iter().enumerate() {
                if removed.contains(&slot) {
                    mapping.push(None);
                } else {
                    mapping.push(Some(next.len()));
                    next.push(block.clone());
                }
            }
            if next.is_empty() {
                self.visibility(entity, true);
                continue;
            }
            if let Ok(mut content) = self.existing.get_mut(entity) {
                content.0 = Message::Assistant { blocks: next };
            }
            self.remap(index, entity, &mapping);
            for (owner, _, target) in bindings.iter_mut() {
                if *owner == entity
                    && let Some(slot) = *target
                {
                    *target = mapping.get(slot).copied().flatten();
                }
            }
        }
    }

    fn remap(&mut self, index: &mut EventIndex, entity: Entity, mapping: &[Option<usize>]) {
        for aliases in index.native_aliases.values_mut() {
            for (target_entity, _, target) in aliases {
                if *target_entity == entity
                    && let Some(slot) = *target
                {
                    *target = mapping.get(slot).copied().flatten();
                }
            }
        }
        for aliases in index.replay_aliases.values_mut() {
            for (target_entity, _, target) in aliases {
                if *target_entity == entity
                    && let Some(slot) = *target
                {
                    *target = mapping.get(slot).copied().flatten();
                }
            }
        }
        for (alias, parent, target) in &self.targets {
            if parent.parent() != entity {
                continue;
            }
            match mapping.get(target.0).copied().flatten() {
                Some(slot) => {
                    self.commands.entity(alias).insert(TargetBlock(slot));
                }
                None => {
                    self.commands.entity(alias).remove::<TargetBlock>();
                }
            }
        }
        let Some(session) = index.event_owners.get(&entity).copied() else {
            return;
        };
        let slots = index.units.entry(session).or_default();
        slots.retain(|(current, _)| *current != entity);
        let length = match self.existing.get(entity).map(|content| &content.0) {
            Ok(Message::Assistant { blocks }) => blocks.len(),
            _ => 0,
        };
        for slot in 0..length {
            slots.push((entity, Some(slot)));
        }
        slots.sort_by_key(|(entity, slot)| (index.identities[entity].sequence, *slot));
        index
            .unit_positions
            .retain(|(entity, _), _| index.event_owners.get(entity) != Some(&session));
        for (position, slot) in slots.iter().enumerate() {
            index.unit_positions.insert(*slot, position);
        }
        index
            .claimed_units
            .retain(|(current, _)| *current != entity);
        for aliases in index.native_aliases.values() {
            for (current, source, target) in aliases {
                if *current == entity && (source.is_none() || target.is_some()) {
                    index.claimed_units.insert((*current, *target));
                }
            }
        }
    }

    fn patch(
        &mut self,
        index: &mut EventIndex,
        bindings: &mut [(Entity, Option<usize>, Option<usize>)],
        message: &Message,
    ) {
        for (entity, source, target) in bindings {
            if self.protected.get(*entity).is_ok() {
                continue;
            }
            let covered = match message {
                Message::Assistant { blocks } => source.is_some_and(|slot| slot < blocks.len()),
                _ => source.is_none(),
            };
            if !covered {
                continue;
            }
            self.visibility(*entity, false);
            let Ok(current) = self.existing.get(*entity) else {
                continue;
            };
            if source.is_some() && target.is_none() {
                let Message::Assistant { blocks: incoming } = message else {
                    continue;
                };
                let Some(block) = incoming.get(source.unwrap()) else {
                    continue;
                };
                let Message::Assistant {
                    blocks: current_blocks,
                } = &current.0
                else {
                    continue;
                };
                let slot = current_blocks.len();
                if let Ok(mut current) = self.existing.get_mut(*entity)
                    && let Message::Assistant { blocks } = &mut current.0
                {
                    blocks.push(block.clone());
                }
                *target = Some(slot);
                if let Some(session) = index.event_owners.get(entity).copied() {
                    let units = index.units.entry(session).or_default();
                    units.push((*entity, Some(slot)));
                    units.sort_by_key(|(entity, slot)| (index.identities[entity].sequence, *slot));
                    index
                        .unit_positions
                        .retain(|(entity, _), _| index.event_owners.get(entity) != Some(&session));
                    for (position, unit) in units.iter().enumerate() {
                        index.unit_positions.insert(*unit, position);
                    }
                }
                continue;
            }
            if current.same_unit(*target, message, *source) {
                continue;
            }
            if let Ok(mut current) = self.existing.get_mut(*entity) {
                current.replace_unit(*target, message, *source);
            }
        }
    }

    fn replay(&mut self, index: &mut EventIndex, event: SerializedEvent) -> Vec<EventId> {
        let Some(session) = index.session(&event.session_id) else {
            return Vec::new();
        };
        let source = index.sources.get(&session).cloned();
        let key = (session, source.clone(), event.event_id.clone());
        let mut bindings = index.replay_aliases.get(&key).cloned().unwrap_or_default();
        let previous = bindings.clone();
        if bindings.is_empty() {
            let after = index
                .replay_positions
                .get(&session)
                .copied()
                .unwrap_or_default();
            if let Some((found, next)) = self.match_units(
                index,
                (session, source.as_ref(), after, false),
                &event.message,
            ) {
                bindings = found;
                index.replay_positions.insert(session, next);
            }
        }
        if bindings.is_empty() {
            let Some(entity) = self.apply(
                index,
                event.clone(),
                MessageDelivery::Committed,
                MaterializationSource::Snapshot,
            ) else {
                return Vec::new();
            };
            for slot in MessageContent::slots(&event.message) {
                bindings.push((entity, slot, slot));
            }
        } else {
            self.resize(index, &mut bindings, &event.message, false);
            self.patch(index, &mut bindings, &event.message);
        }
        let cursor = index.replay_positions.entry(session).or_default();
        for (entity, _, target) in &bindings {
            if let Some(position) = index.unit_positions.get(&(*entity, *target)) {
                *cursor = (*cursor).max(position + 1);
            }
        }
        if previous != bindings || !index.replay_aliases.contains_key(&key) {
            for (alias, parent, record, kind, provider) in &self.replay_records {
                let matches_source = match (&source, kind, provider) {
                    (Some(source), Some(kind), Some(provider)) => {
                        kind == &source.kind && provider == &source.id
                    }
                    (None, None, None) => true,
                    _ => false,
                };
                if record.0 == key.2
                    && matches_source
                    && index.event_owners.get(&parent.parent()) == Some(&session)
                {
                    self.commands.entity(alias).despawn();
                }
            }
            for (entity, source_slot, target_slot) in &bindings {
                let mut alias = self
                    .commands
                    .spawn((ReplayAlias(event.event_id.clone()), ChildOf(*entity)));
                if let Some(source) = &source {
                    alias.insert((source.kind.clone(), source.id.clone()));
                }
                if let Some(slot) = source_slot {
                    alias.insert(SourceBlock(*slot));
                }
                if let Some(slot) = target_slot {
                    alias.insert(TargetBlock(*slot));
                }
            }
            index.replay_aliases.insert(key, bindings.clone());
        }
        let mut ids = Vec::new();
        for (entity, _, _) in bindings {
            let id = index.identities[&entity].event_id.clone();
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
        ids
    }

    fn append(&mut self, index: &mut EventIndex, session_id: &SessionId, messages: &[Message]) {
        let Some(session) = index.session(session_id) else {
            return;
        };
        let mut sequence = index
            .latest_sequences
            .get(&session)
            .copied()
            .unwrap_or_default();
        let mut reply_to = index
            .latest_users
            .get(&session)
            .map(|(_, event_id)| event_id.clone());
        let created_at = vmux_ecs::UnixMillis::now().0.max(0) as u64;
        for (offset, message) in messages.iter().enumerate() {
            sequence = sequence.saturating_add(1);
            let event_id =
                EventId::new(format!("session:{}:event:{sequence}", session_id.as_str()));
            let is_user = matches!(message, Message::User { .. });
            let event = SerializedEvent {
                event_id: event_id.clone(),
                session_id: session_id.clone(),
                actor_id: if is_user {
                    MemberId::local(session_id)
                } else {
                    MemberId::agent(session_id)
                },
                client_op_id: None,
                server_seq: sequence,
                created_at_ms: created_at.saturating_add(offset as u64),
                reply_to: if is_user { None } else { reply_to.clone() },
                message: message.clone(),
            };
            let _ = self.apply(
                index,
                event,
                MessageDelivery::Committed,
                MaterializationSource::Snapshot,
            );
            if is_user {
                reply_to = Some(event_id);
            }
        }
    }

    fn apply(
        &mut self,
        index: &mut EventIndex,
        mut event: SerializedEvent,
        delivery: MessageDelivery,
        source: MaterializationSource,
    ) -> Option<Entity> {
        let session = index.session(&event.session_id)?;
        if source == MaterializationSource::Snapshot {
            let current_source = index.sources.get(&session);
            let incompatible = index
                .events
                .get(&(session, event.event_id.clone()))
                .is_some_and(|entity| {
                    index
                        .origins
                        .get(entity)
                        .is_some_and(|origin| Some(origin) != current_source)
                });
            if incompatible {
                let mut sequence = index
                    .latest_sequences
                    .get(&session)
                    .copied()
                    .unwrap_or_default();
                loop {
                    sequence = sequence.checked_add(1)?;
                    let id = EventId::new(format!(
                        "session:{}:event:{sequence}",
                        event.session_id.as_str()
                    ));
                    if index.events.contains_key(&(session, id.clone())) {
                        continue;
                    }
                    event.event_id = id;
                    event.server_seq = sequence;
                    break;
                }
            }
            if let Some(reply) = &event.reply_to {
                let key = (session, current_source.cloned(), reply.clone());
                if let Some(bindings) = index.replay_aliases.get(&key)
                    && let Some((entity, _, _)) = bindings.first()
                    && let Some(identity) = index.identities.get(entity)
                {
                    event.reply_to = Some(identity.event_id.clone());
                }
            }
        }
        let existing = index
            .events
            .get(&(session, event.event_id.clone()))
            .copied()
            .or_else(|| {
                event
                    .client_op_id
                    .as_ref()
                    .and_then(|id| index.operations.get(&(session, id.clone())).copied())
            });
        let identity = EventIdentity::from_event(&event);
        let mut timestamp = CreatedAt(i64::try_from(event.created_at_ms).unwrap_or(i64::MAX));
        let entity = if let Some(entity) = existing {
            if let Some((event_id, client_op_id)) = index.entity_keys.remove(&entity) {
                index.events.remove(&(session, event_id));
                if let Some(client_op_id) = client_op_id {
                    index.operations.remove(&(session, client_op_id));
                }
            }
            timestamp = if source == MaterializationSource::Snapshot {
                index.timestamps.get(&entity).copied().unwrap_or(timestamp)
            } else {
                timestamp
            };
            let mut entity_commands = self.commands.entity(entity);
            entity_commands.insert((
                identity.clone(),
                timestamp,
                MessageContent(event.message.clone()),
                delivery,
            ));
            if source == MaterializationSource::Snapshot && !index.native_bound.contains(&entity) {
                entity_commands.insert(SnapshotEvent);
            } else {
                entity_commands.remove::<SnapshotEvent>();
            }
            entity
        } else {
            let mut entity_commands = self.commands.spawn((
                ConversationEvent,
                MaterializedEvent,
                identity.clone(),
                timestamp,
                MessageContent(event.message.clone()),
                delivery,
                ChildOf(session),
            ));
            if source == MaterializationSource::Snapshot {
                entity_commands.insert(SnapshotEvent);
            }
            entity_commands.id()
        };
        index
            .events
            .insert((session, event.event_id.clone()), entity);
        index.record_sequence(session, &identity, &event.message);
        if let Some(client_op_id) = event.client_op_id.clone() {
            index.operations.insert((session, client_op_id), entity);
        }
        index.timestamps.insert(entity, timestamp);
        index.identities.insert(entity, identity.clone());
        index.event_owners.insert(entity, session);
        if source == MaterializationSource::Operation {
            self.commands.entity(entity).insert(OperationEvent);
        } else {
            self.commands.entity(entity).insert(ImportedEvent);
        }
        if let Some(origin) = index.sources.get(&session) {
            self.commands
                .entity(entity)
                .insert((origin.kind.clone(), origin.id.clone()));
            index.origins.insert(entity, origin.clone());
        }
        let slots = index.units.entry(session).or_default();
        if !index.unit_positions.contains_key(&(entity, None))
            && !index.unit_positions.contains_key(&(entity, Some(0)))
        {
            for slot in MessageContent::slots(&event.message) {
                index.unit_positions.insert((entity, slot), slots.len());
                slots.push((entity, slot));
            }
        }
        if source == MaterializationSource::Snapshot && !index.native_bound.contains(&entity) {
            index
                .snapshot_events
                .entry(session)
                .or_default()
                .insert(event.event_id.clone(), (entity, event.client_op_id.clone()));
        }
        index
            .entity_keys
            .insert(entity, (event.event_id, event.client_op_id));
        Some(entity)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_app::TaskPoolPlugin;
    use bevy_asset::AssetPlugin;
    use bevy_ecs::system::RunSystemOnce;
    use moonshine_save::prelude::{LoadWorld, SaveWorld, TriggerLoad, TriggerSave};
    use vmux_api::session::AssistantBlock;

    #[test]
    fn read_lifecycle_children_do_not_rewrite_the_transcript_cache() {
        let observed = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let writes = observed.clone();
        let mut app = App::new();
        app.add_plugins(crate::history::Plugin).add_systems(
            PostUpdate,
            move |changed: Query<Entity, Changed<TranscriptCache>>| {
                writes.fetch_add(changed.iter().count(), std::sync::atomic::Ordering::SeqCst);
            },
        );
        let session = app
            .world_mut()
            .spawn((
                Session,
                SessionId::new("cache"),
                ProviderKind("native".into()),
                ProviderSessionId("source".into()),
            ))
            .id();
        app.world_mut().write_message(SnapshotReceived {
            session: SessionId::new("cache"),
            messages: vec![Message::user("first")],
        });
        app.update();
        assert_eq!(observed.swap(0, std::sync::atomic::Ordering::SeqCst), 1);
        let read = app
            .world_mut()
            .run_system_once(move |versions: Versions| versions.capture(session, false))
            .unwrap()
            .unwrap();
        let operation = app.world_mut().spawn((read, ChildOf(session))).id();
        app.update();
        assert_eq!(observed.load(std::sync::atomic::Ordering::SeqCst), 0);
        app.world_mut().despawn(operation);
        app.update();
        assert_eq!(observed.load(std::sync::atomic::Ordering::SeqCst), 0);
        app.world_mut().write_message(ConversationAppendReceived {
            session: SessionId::new("cache"),
            messages: vec![Message::user("next")],
        });
        app.update();
        assert_eq!(observed.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[test]
    fn a_session_created_with_its_first_message_is_materialized_in_one_update() {
        let mut app = App::new();
        app.add_plugins(crate::DomainPlugin);
        let request = crate::CreateRequest::new("Task", "", std::path::PathBuf::from("/tmp"), None);
        let id = request.id().clone();
        app.world_mut().write_message(request);
        let messages = vec![Message::user("first message")];
        app.world_mut().write_message(ConversationAppendReceived {
            session: id.clone(),
            messages: messages.clone(),
        });

        app.update();

        let session = app
            .world()
            .resource::<IdentityIndex<SessionId, Session>>()
            .entity(&id)
            .unwrap()
            .unwrap();
        let transcript = app
            .world_mut()
            .run_system_once(move |transcripts: Transcripts| transcripts.get(session))
            .unwrap();
        assert_eq!(transcript.messages, messages);
    }

    #[test]
    fn duplicate_session_ids_refuse_events_and_recover_after_removal() {
        let mut app = App::new();
        app.add_plugins(crate::history::Plugin);
        let id = SessionId("shared".into());
        let first = app.world_mut().spawn((Session, id.clone())).id();
        let duplicate = app.world_mut().spawn((Session, id.clone())).id();
        app.world_mut().write_message(ConversationAppendReceived {
            session: id.clone(),
            messages: vec![Message::user("ambiguous")],
        });
        app.update();
        assert!(
            app.world_mut()
                .query_filtered::<Entity, With<ConversationEvent>>()
                .iter(app.world())
                .next()
                .is_none()
        );

        app.world_mut().despawn(duplicate);
        let messages = vec![Message::user("resolved")];
        app.world_mut().write_message(ConversationAppendReceived {
            session: id,
            messages: messages.clone(),
        });
        app.update();
        let transcript = app
            .world_mut()
            .run_system_once(move |transcripts: Transcripts| transcripts.get(first))
            .unwrap();
        assert_eq!(transcript.messages, messages);
    }

    #[test]
    fn saved_history_is_available_before_replay_and_replaced_without_duplicates() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("conversation.ron");
        let id = SessionId("saved-session".into());
        let messages = vec![
            Message::user("history"),
            Message::Assistant {
                blocks: vec![AssistantBlock::Text("answer".into())],
            },
        ];
        let mut source = App::new();
        source
            .add_plugins((
                TaskPoolPlugin::default(),
                AssetPlugin::default(),
                vmux_ecs::PrimitivesPlugin,
                crate::history::Plugin,
            ))
            .register_stored::<Session>()
            .register_stored::<SessionId>()
            .register_stored::<CreatedAt>()
            .add_observer(moonshine_save::save::save_on_default_event);
        let session = source.world_mut().spawn((Session, id.clone())).id();
        source.world_mut().write_message(SnapshotReceived {
            session: id.clone(),
            messages: messages.clone(),
        });
        source.update();
        let original = source
            .world_mut()
            .run_system_once(move |transcripts: Transcripts| transcripts.get(session))
            .unwrap();
        let registry = source
            .world()
            .resource::<bevy_ecs::reflect::AppTypeRegistry>()
            .read();
        let filter = vmux_ecs::store::WorkspaceStored::filter(&registry)
            .allow::<Save>()
            .allow::<ChildOf>()
            .allow::<Children>()
            .allow::<Name>();
        drop(registry);
        let mut save = SaveWorld::default_into_file(path.clone());
        save.components = filter;
        source.world_mut().commands().trigger_save(save);
        source.update();
        assert!(path.exists());
        let mut restored = App::new();
        restored
            .add_plugins((
                TaskPoolPlugin::default(),
                AssetPlugin::default(),
                vmux_ecs::PrimitivesPlugin,
                crate::history::Plugin,
            ))
            .register_stored::<Session>()
            .register_stored::<SessionId>()
            .register_stored::<CreatedAt>()
            .add_observer(moonshine_save::load::load_on_default_event);
        restored.update();
        restored
            .world_mut()
            .commands()
            .trigger_load(LoadWorld::default_from_file(path));
        restored.update();
        let session = restored
            .world_mut()
            .query_filtered::<Entity, With<Session>>()
            .single(restored.world())
            .unwrap();
        let history = restored
            .world_mut()
            .run_system_once(move |transcripts: Transcripts| transcripts.get(session))
            .unwrap();
        assert_eq!(history.messages, messages);
        assert_eq!(history.created_at, original.created_at);
        let replacement = vec![Message::user("replayed")];
        restored.world_mut().write_message(SnapshotReceived {
            session: id,
            messages: replacement.clone(),
        });
        restored.update();
        let history = restored
            .world_mut()
            .run_system_once(move |transcripts: Transcripts| transcripts.get(session))
            .unwrap();
        assert_eq!(history.messages, replacement);
    }

    #[test]
    fn snapshot_materializes_ordered_events_under_session() {
        let mut app = App::new();
        app.add_plugins(crate::history::Plugin);
        let session = app
            .world_mut()
            .spawn((Session, SessionId("session-1".into())))
            .id();
        app.update();
        app.world_mut()
            .resource_mut::<Messages<SnapshotReceived>>()
            .write(SnapshotReceived {
                session: SessionId("session-1".into()),
                messages: vec![
                    Message::user("hello"),
                    Message::Assistant {
                        blocks: vec![AssistantBlock::Text("again".into())],
                    },
                ],
            });

        app.update();

        let transcript = app
            .world_mut()
            .run_system_once(move |transcripts: Transcripts| transcripts.get(session))
            .unwrap();
        assert_eq!(transcript.messages.len(), 2);
        assert_eq!(transcript.created_at.len(), 2);
        let mut identities = app
            .world_mut()
            .query_filtered::<&EventIdentity, With<ConversationEvent>>()
            .iter(app.world())
            .cloned()
            .collect::<Vec<_>>();
        identities.sort_by_key(|identity| identity.sequence);
        assert_eq!(identities[1].reply_to, Some(identities[0].event_id.clone()));
    }

    #[test]
    fn agent_added_after_session_gets_a_member() {
        let mut app = App::new();
        app.add_plugins(crate::history::Plugin);
        let session = app
            .world_mut()
            .spawn((Session, SessionId("session-1".into())))
            .id();
        app.update();

        app.world_mut()
            .entity_mut(session)
            .insert(AgentId("codex".into()));
        app.update();

        let members = app
            .world()
            .get::<Children>(session)
            .unwrap()
            .iter()
            .filter_map(|child| {
                Some((
                    app.world().get::<MemberKind>(child)?,
                    app.world().get::<Name>(child)?,
                ))
            })
            .collect::<Vec<_>>();
        assert_eq!(members.len(), 2);
        assert!(members.iter().any(|(kind, _)| **kind == MemberKind::Human));
        assert!(
            members
                .iter()
                .any(|member| { *member.0 == MemberKind::Agent && member.1.as_str() == "codex" })
        );
    }

    #[test]
    fn committed_operation_replaces_the_pending_client_operation() {
        let mut app = App::new();
        app.add_plugins(crate::history::Plugin);
        let session_id = SessionId("session-1".into());
        let session = app.world_mut().spawn((Session, session_id.clone())).id();
        let client_op_id = ClientOpId::new("op-1");
        let pending = app
            .world_mut()
            .spawn((
                ConversationEvent,
                MaterializedEvent,
                EventIdentity {
                    event_id: EventId::new("pending-1"),
                    actor_id: MemberId::local(&session_id),
                    client_op_id: Some(client_op_id.clone()),
                    sequence: 0,
                    reply_to: None,
                },
                CreatedAt(1),
                MessageContent(Message::user("draft")),
                MessageDelivery::Pending(client_op_id.clone()),
                ChildOf(session),
            ))
            .id();
        app.update();
        app.world_mut()
            .resource_mut::<Messages<OperationCommitted>>()
            .write(OperationCommitted(SerializedEvent {
                event_id: EventId::new("event-1"),
                session_id,
                actor_id: MemberId::new("member-1"),
                client_op_id: Some(client_op_id),
                server_seq: 1,
                created_at_ms: 2,
                reply_to: None,
                message: Message::user("committed"),
            }));

        app.update();

        assert_eq!(
            app.world().get::<MessageContent>(pending),
            Some(&MessageContent(Message::user("committed")))
        );
        assert_eq!(
            app.world().get::<MessageDelivery>(pending),
            Some(&MessageDelivery::Committed)
        );
        let count = app
            .world_mut()
            .query_filtered::<Entity, With<ConversationEvent>>()
            .iter(app.world())
            .count();
        assert_eq!(count, 1);
    }

    #[test]
    fn snapshot_does_not_remove_operation_events() {
        let mut app = App::new();
        app.add_plugins(crate::history::Plugin);
        let session_id = SessionId("session-1".into());
        let session = app.world_mut().spawn((Session, session_id.clone())).id();
        app.world_mut()
            .resource_mut::<Messages<OperationCommitted>>()
            .write(OperationCommitted(SerializedEvent {
                event_id: EventId::new("operation-1"),
                session_id: session_id.clone(),
                actor_id: MemberId::local(&session_id),
                client_op_id: Some(ClientOpId::new("op-1")),
                server_seq: 1,
                created_at_ms: 10,
                reply_to: None,
                message: Message::user("operation"),
            }));
        app.update();
        app.world_mut()
            .resource_mut::<Messages<SnapshotReceived>>()
            .write(SnapshotReceived {
                session: session_id,
                messages: Vec::new(),
            });

        app.update();

        let transcript = app
            .world_mut()
            .run_system_once(move |transcripts: Transcripts| transcripts.get(session))
            .unwrap();
        assert_eq!(transcript.messages, vec![Message::user("operation")]);
    }

    #[test]
    fn append_preserves_the_snapshot_and_orders_new_messages() {
        let mut app = App::new();
        app.add_plugins(crate::history::Plugin);
        let session_id = SessionId("session-1".into());
        let session = app.world_mut().spawn((Session, session_id.clone())).id();
        app.world_mut()
            .resource_mut::<Messages<SnapshotReceived>>()
            .write(SnapshotReceived {
                session: session_id.clone(),
                messages: vec![Message::user("first")],
            });
        app.update();
        app.world_mut()
            .resource_mut::<Messages<ConversationAppendReceived>>()
            .write(ConversationAppendReceived {
                session: session_id,
                messages: vec![
                    Message::Assistant {
                        blocks: vec![AssistantBlock::Text("second".into())],
                    },
                    Message::user("third"),
                ],
            });

        app.update();

        let transcript = app
            .world_mut()
            .run_system_once(move |transcripts: Transcripts| transcripts.get(session))
            .unwrap();
        assert_eq!(
            transcript.messages,
            vec![
                Message::user("first"),
                Message::Assistant {
                    blocks: vec![AssistantBlock::Text("second".into())],
                },
                Message::user("third"),
            ]
        );
    }

    #[test]
    fn operation_replaces_stale_snapshot_event_in_the_same_update() {
        let mut app = App::new();
        app.add_plugins(crate::history::Plugin);
        let session_id = SessionId("session-1".into());
        let session = app.world_mut().spawn((Session, session_id.clone())).id();
        app.world_mut()
            .resource_mut::<Messages<SnapshotReceived>>()
            .write(SnapshotReceived {
                session: session_id.clone(),
                messages: vec![Message::user("snapshot")],
            });
        app.update();
        app.world_mut()
            .resource_mut::<Messages<SnapshotReceived>>()
            .write(SnapshotReceived {
                session: session_id.clone(),
                messages: Vec::new(),
            });
        app.world_mut()
            .resource_mut::<Messages<OperationCommitted>>()
            .write(OperationCommitted(SerializedEvent {
                event_id: EventId::new("session:session-1:event:1"),
                session_id: session_id.clone(),
                actor_id: MemberId::local(&session_id),
                client_op_id: None,
                server_seq: 1,
                created_at_ms: 20,
                reply_to: None,
                message: Message::user("operation"),
            }));

        app.update();

        let transcript = app
            .world_mut()
            .run_system_once(move |transcripts: Transcripts| transcripts.get(session))
            .unwrap();
        assert_eq!(transcript.messages, vec![Message::user("operation")]);
    }

    #[test]
    fn operation_replacement_survives_a_later_snapshot() {
        let mut app = App::new();
        app.add_plugins(crate::history::Plugin);
        let session_id = SessionId("session-1".into());
        let session = app.world_mut().spawn((Session, session_id.clone())).id();
        app.world_mut()
            .resource_mut::<Messages<SnapshotReceived>>()
            .write(SnapshotReceived {
                session: session_id.clone(),
                messages: vec![Message::user("snapshot")],
            });
        app.update();
        app.world_mut()
            .resource_mut::<Messages<OperationCommitted>>()
            .write(OperationCommitted(SerializedEvent {
                event_id: EventId::new("session:session-1:event:1"),
                session_id: session_id.clone(),
                actor_id: MemberId::local(&session_id),
                client_op_id: None,
                server_seq: 1,
                created_at_ms: 20,
                reply_to: None,
                message: Message::user("operation"),
            }));
        app.update();
        app.world_mut()
            .resource_mut::<Messages<SnapshotReceived>>()
            .write(SnapshotReceived {
                session: session_id,
                messages: Vec::new(),
            });

        app.update();

        let transcript = app
            .world_mut()
            .run_system_once(move |transcripts: Transcripts| transcripts.get(session))
            .unwrap();
        assert_eq!(transcript.messages, vec![Message::user("operation")]);
    }

    #[test]
    fn repeated_snapshot_preserves_existing_timestamp() {
        let mut app = App::new();
        app.add_plugins(crate::history::Plugin);
        let session_id = SessionId("session-1".into());
        app.world_mut().spawn((Session, session_id.clone()));
        app.world_mut()
            .resource_mut::<Messages<SnapshotReceived>>()
            .write(SnapshotReceived {
                session: session_id.clone(),
                messages: vec![Message::user("hello")],
            });
        app.update();
        let event = app
            .world_mut()
            .query_filtered::<Entity, With<ConversationEvent>>()
            .single(app.world())
            .unwrap();
        app.world_mut().entity_mut(event).insert(CreatedAt(42));
        app.world_mut()
            .resource_mut::<Messages<SnapshotReceived>>()
            .write(SnapshotReceived {
                session: session_id,
                messages: vec![Message::user("hello")],
            });

        app.update();

        assert_eq!(
            app.world().get::<CreatedAt>(event).map(|value| value.0),
            Some(42)
        );
    }
}
