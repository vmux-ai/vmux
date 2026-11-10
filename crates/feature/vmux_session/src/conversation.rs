use std::collections::{HashMap, HashSet};

use bevy_app::{App, Plugin, PostUpdate, Update};
use bevy_ecs::prelude::*;
use bevy_ecs::system::SystemParam;
use bevy_reflect::{Reflect, ReflectDeserialize, ReflectSerialize};
use moonshine_save::prelude::Save;
use serde::{Deserialize, Serialize};
use vmux_api::service::ServiceMessageSet;
use vmux_api::session::{
    ClientOpId, ConversationEvent as SerializedEvent, EventId, MemberId, MemberKind, MemberRole,
    Message,
};
use vmux_ecs::persistence::PersistenceAppExt;
use vmux_ecs::{CreatedAt, IdentityIndex, IdentityIndexPlugin, IdentityIndexSet};

use crate::{AgentId, Session, SessionId};

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
        .add_message::<SnapshotReceived>()
        .add_message::<ConversationAppendReceived>()
        .add_message::<OperationReceived>()
        .add_message::<OperationCommitted>()
        .register_persisted::<Document>()
        .register_persisted::<DocumentKind>()
        .register_persisted::<ConversationEvent>()
        .register_persisted::<EventIdentity>()
        .register_persisted::<MessageContent>()
        .register_persisted::<MaterializedEvent>()
        .register_persisted::<SnapshotEvent>()
        .add_systems(
            Update,
            (ensure, sync_agent).after(index).before(TranscriptReadySet),
        )
        .add_systems(
            Update,
            (
                materialize,
                bevy_ecs::schedule::ApplyDeferred,
                cache_transcripts,
            )
                .chain()
                .after(ServiceMessageSet)
                .after(index)
                .in_set(TranscriptReadySet),
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
struct SnapshotEvent;

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
struct TranscriptCache {
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
        With<ConversationEvent>,
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
        With<ConversationEvent>,
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
        if children.is_changed()
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

fn materialize(
    mut snapshots: MessageReader<SnapshotReceived>,
    mut appended: MessageReader<ConversationAppendReceived>,
    mut received: MessageReader<OperationReceived>,
    mut committed: MessageReader<OperationCommitted>,
    history: SessionHistory,
    mut materializer: EventMaterializer,
) {
    if snapshots.is_empty() && appended.is_empty() && received.is_empty() && committed.is_empty() {
        return;
    }
    let snapshots = snapshots.read().collect::<Vec<_>>();
    let appended = appended.read().collect::<Vec<_>>();
    let received = received.read().collect::<Vec<_>>();
    let committed = committed.read().collect::<Vec<_>>();
    let requested = snapshots
        .iter()
        .map(|snapshot| &snapshot.session)
        .chain(appended.iter().map(|append| &append.session))
        .chain(received.iter().map(|operation| &operation.0.session_id))
        .chain(committed.iter().map(|operation| &operation.0.session_id))
        .cloned()
        .collect::<HashSet<_>>();
    let mut index = history.index(&requested);
    for request in snapshots {
        let Some(session) = index.session(&request.session) else {
            continue;
        };
        let mut stale = index.snapshot_events.remove(&session).unwrap_or_default();
        let now = vmux_ecs::UnixMillis::now().0.max(0) as u64;
        for event in SerializedEvent::from_messages(&request.session, now, &request.messages) {
            stale.remove(&event.event_id);
            materializer.apply(
                &mut index,
                event,
                MessageDelivery::Committed,
                MaterializationSource::Snapshot,
            );
        }
        for (event_id, (entity, client_op_id)) in stale {
            index.events.remove(&(session, event_id));
            if let Some(client_op_id) = client_op_id {
                index.operations.remove(&(session, client_op_id));
            }
            index.entity_keys.remove(&entity);
            materializer.commands.entity(entity).despawn();
        }
    }
    for request in appended {
        materializer.append(&mut index, &request.session, &request.messages);
    }
    for operation in received {
        materializer.apply(
            &mut index,
            operation.0.clone(),
            MessageDelivery::Committed,
            MaterializationSource::Operation,
        );
    }
    for operation in committed {
        materializer.apply(
            &mut index,
            operation.0.clone(),
            MessageDelivery::Committed,
            MaterializationSource::Operation,
        );
    }
}

#[derive(SystemParam)]
struct SessionHistory<'w, 's> {
    sessions: Query<'w, 's, (Entity, &'static SessionId, Option<&'static Children>), With<Session>>,
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
        ),
        (With<ConversationEvent>, With<MaterializedEvent>),
    >,
}

impl SessionHistory<'_, '_> {
    fn index(&self, requested: &HashSet<SessionId>) -> EventIndex<'_> {
        let mut index = EventIndex {
            sessions: &self.identities,
            events: HashMap::new(),
            operations: HashMap::new(),
            entity_keys: HashMap::new(),
            timestamps: HashMap::new(),
            snapshot_events: HashMap::new(),
            latest_sequences: HashMap::new(),
            latest_users: HashMap::new(),
        };
        for (session, id, children) in &self.sessions {
            if !requested.contains(id) || index.session(id) != Some(session) {
                continue;
            }
            for child in children.into_iter().flat_map(|children| children.iter()) {
                let Ok((entity, identity, message, timestamp, from_snapshot)) =
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
                index.record_sequence(session, identity, &message.0);
                if from_snapshot {
                    index.snapshot_events.entry(session).or_default().insert(
                        identity.event_id.clone(),
                        (entity, identity.client_op_id.clone()),
                    );
                }
            }
        }
        index
    }
}

struct EventIndex<'a> {
    sessions: &'a IdentityIndex<SessionId, Session>,
    events: HashMap<(Entity, EventId), Entity>,
    operations: HashMap<(Entity, ClientOpId), Entity>,
    entity_keys: HashMap<Entity, (EventId, Option<ClientOpId>)>,
    timestamps: HashMap<Entity, CreatedAt>,
    snapshot_events: HashMap<Entity, HashMap<EventId, (Entity, Option<ClientOpId>)>>,
    latest_sequences: HashMap<Entity, u64>,
    latest_users: HashMap<Entity, (u64, EventId)>,
}

impl EventIndex<'_> {
    fn session(&self, id: &SessionId) -> Option<Entity> {
        self.sessions.entity(id).unwrap_or_default()
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
}

#[derive(SystemParam)]
struct EventMaterializer<'w, 's> {
    commands: Commands<'w, 's>,
}

impl EventMaterializer<'_, '_> {
    fn append(&mut self, index: &mut EventIndex<'_>, session_id: &SessionId, messages: &[Message]) {
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
            self.apply(
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
        index: &mut EventIndex<'_>,
        event: SerializedEvent,
        delivery: MessageDelivery,
        source: MaterializationSource,
    ) {
        let Some(session) = index.session(&event.session_id) else {
            return;
        };
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
        let timestamp = CreatedAt(i64::try_from(event.created_at_ms).unwrap_or(i64::MAX));
        let entity = if let Some(entity) = existing {
            if let Some((event_id, client_op_id)) = index.entity_keys.remove(&entity) {
                index.events.remove(&(session, event_id));
                if let Some(client_op_id) = client_op_id {
                    index.operations.remove(&(session, client_op_id));
                }
            }
            let timestamp = if source == MaterializationSource::Snapshot {
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
            if source == MaterializationSource::Snapshot {
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
        if source == MaterializationSource::Snapshot {
            index
                .snapshot_events
                .entry(session)
                .or_default()
                .insert(event.event_id.clone(), (entity, event.client_op_id.clone()));
        }
        index
            .entity_keys
            .insert(entity, (event.event_id, event.client_op_id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "app")]
    use bevy::prelude::{AssetPlugin, MinimalPlugins};
    use bevy_ecs::system::RunSystemOnce;
    #[cfg(feature = "app")]
    use moonshine_save::prelude::{LoadWorld, SaveWorld, TriggerLoad, TriggerSave};
    use vmux_api::session::AssistantBlock;

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
        app.add_plugins(ConversationPlugin);
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

    #[cfg(feature = "app")]
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
                MinimalPlugins,
                AssetPlugin::default(),
                vmux_ecs::PrimitivesPlugin,
                ConversationPlugin,
            ))
            .register_persisted::<Session>()
            .register_persisted::<SessionId>()
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
        let filter = vmux_ecs::persistence::WorkspacePersisted::filter(&registry)
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
                MinimalPlugins,
                AssetPlugin::default(),
                vmux_ecs::PrimitivesPlugin,
                ConversationPlugin,
            ))
            .register_persisted::<Session>()
            .register_persisted::<SessionId>()
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
        app.add_plugins(ConversationPlugin);
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
        app.add_plugins(ConversationPlugin);
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
        app.add_plugins(ConversationPlugin);
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
        app.add_plugins(ConversationPlugin);
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
        app.add_plugins(ConversationPlugin);
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
        app.add_plugins(ConversationPlugin);
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
        app.add_plugins(ConversationPlugin);
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
        app.add_plugins(ConversationPlugin);
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
