use std::collections::{HashMap, HashSet};

use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use bevy_ecs::system::SystemParam;
use bevy_reflect::Reflect;
use moonshine_save::prelude::Save;
use serde::{Deserialize, Serialize};
use vmux_api::session::{
    ClientOperation, ConversationEvent as SerializedEvent, Member as SerializedMember, MemberKind,
    MemberRole, Message,
};
use vmux_ecs::CreatedAt;
use vmux_ecs::persistence::PersistenceAppExt;

use crate::{AgentRef, Id, Session};

pub(crate) struct ConversationPlugin;

impl Plugin for ConversationPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<SnapshotReceived>()
            .add_message::<ConversationAppendReceived>()
            .add_message::<OperationReceived>()
            .add_message::<OperationCommitted>()
            .register_persisted::<Document>()
            .register_persisted::<DocumentKind>()
            .add_systems(Update, (ensure, sync_agent, materialize));
    }
}

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Member;

#[derive(Component, Clone, Debug, Default, PartialEq, Eq, Reflect)]
#[reflect(Component)]
#[type_path = "vmux_session"]
pub struct ConversationEvent;

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
#[component(immutable)]
pub struct Sequence(pub u64);

#[derive(Component, Clone, Debug, PartialEq, Eq)]
#[component(immutable)]
pub struct ReplyToId(pub Id<SerializedEvent>);

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
#[relationship(relationship_target = Replies)]
pub struct ReplyTo(Entity);

#[derive(Component, Debug)]
#[relationship_target(relationship = ReplyTo)]
pub struct Replies(Vec<Entity>);

#[derive(Component, Clone, Debug, PartialEq)]
pub struct MessageContent(pub Message);

#[derive(Component, Clone, Debug, Eq, PartialEq)]
pub enum MessageDelivery {
    Pending(Id<ClientOperation>),
    Committed,
    Failed(String),
}

#[derive(Component, Clone, Copy, Debug, Default, Eq, PartialEq, Reflect)]
#[reflect(Component)]
#[type_path = "vmux_session"]
pub struct MaterializedEvent;

#[derive(Component, Clone, Copy, Debug, Default, Eq, PartialEq)]
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
    pub session: Id<Session>,
    pub messages: Vec<Message>,
}

#[derive(Message, Clone, Debug)]
pub struct ConversationAppendReceived {
    pub session: Id<Session>,
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

#[derive(SystemParam)]
pub struct Transcripts<'w, 's> {
    children: Query<'w, 's, &'static Children>,
    events: Query<
        'w,
        's,
        (
            &'static Sequence,
            &'static MessageContent,
            &'static CreatedAt,
        ),
        With<ConversationEvent>,
    >,
}

impl Transcripts<'_, '_> {
    pub fn get(&self, session: Entity) -> Transcript {
        let items = self
            .children
            .get(session)
            .ok()
            .into_iter()
            .flat_map(|children| children.iter())
            .filter_map(|child| {
                let (sequence, content, created_at) = self.events.get(child).ok()?;
                Some((
                    sequence.0,
                    content.0.clone(),
                    created_at.milliseconds().max(0) as u64,
                ))
            })
            .collect();
        Transcript::from_items(items)
    }
}

fn ensure(
    sessions: Query<(Entity, &Id<Session>, Option<&Children>), Added<Session>>,
    members: Query<&MemberKind, With<Member>>,
    documents: Query<(), With<Document>>,
    mut commands: Commands,
) {
    for (session, id, children) in &sessions {
        let prefix = format!("session:{}", id.as_str());
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
                SerializedMember::local_id(id),
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
    sessions: Query<(Entity, &Id<Session>, &AgentRef, Option<&Children>), Changed<AgentRef>>,
    mut members: Query<
        (
            Entity,
            Option<&Id<SerializedMember>>,
            &mut Name,
            &mut MemberRole,
            &MemberKind,
        ),
        With<Member>,
    >,
    mut commands: Commands,
) {
    for (session, session_id, agent_id, children) in &sessions {
        let mut found = false;
        for child in children.into_iter().flat_map(|children| children.iter()) {
            let Ok((entity, id, mut name, mut role, kind)) = members.get_mut(child) else {
                continue;
            };
            if *kind != MemberKind::Agent {
                continue;
            }
            let expected = SerializedMember::agent_id(session_id);
            if id != Some(&expected) {
                commands.entity(entity).insert(expected);
            }
            *name = Name::new(agent_id.as_str().to_string());
            *role = MemberRole::Participant;
            found = true;
        }
        if !found {
            commands.spawn((
                Member,
                SerializedMember::agent_id(session_id),
                Name::new(agent_id.as_str().to_string()),
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
    sessions: Query<(Entity, &Id<Session>, Option<&Children>), With<Session>>,
    existing: Query<
        (
            Entity,
            &Id<SerializedEvent>,
            &Id<SerializedMember>,
            Option<&Id<ClientOperation>>,
            &Sequence,
            Option<&ReplyToId>,
            &MessageContent,
            &CreatedAt,
            Has<SnapshotEvent>,
        ),
        (With<ConversationEvent>, With<MaterializedEvent>),
    >,
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
    let mut index = EventIndex {
        sessions: HashMap::new(),
        events: HashMap::new(),
        operations: HashMap::new(),
        entity_keys: HashMap::new(),
        timestamps: HashMap::new(),
        snapshot_events: HashMap::new(),
        latest_sequences: HashMap::new(),
        latest_users: HashMap::new(),
        reply_ids: HashMap::new(),
    };
    for (session, id, children) in &sessions {
        if !requested.contains(id) {
            continue;
        }
        index.sessions.insert(id.clone(), session);
        for child in children.into_iter().flat_map(|children| children.iter()) {
            let Ok((
                entity,
                event_id,
                _actor_id,
                client_op_id,
                sequence,
                reply_to,
                message,
                timestamp,
                from_snapshot,
            )) = existing.get(child)
            else {
                continue;
            };
            index.events.insert((session, event_id.clone()), entity);
            if let Some(client_op_id) = client_op_id.cloned() {
                index.operations.insert((session, client_op_id), entity);
            }
            index
                .entity_keys
                .insert(entity, (event_id.clone(), client_op_id.cloned()));
            index.timestamps.insert(entity, *timestamp);
            index.record_sequence(session, event_id, sequence.0, &message.0);
            if let Some(reply_to) = reply_to {
                index
                    .reply_ids
                    .insert(entity, (session, reply_to.0.clone()));
            }
            if from_snapshot {
                index
                    .snapshot_events
                    .entry(session)
                    .or_default()
                    .insert(event_id.clone(), (entity, client_op_id.cloned()));
            }
        }
    }
    for request in snapshots {
        let Some(&session) = index.sessions.get(&request.session) else {
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
            index.reply_ids.remove(&entity);
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
    index.resolve_replies(&mut materializer.commands);
}

struct EventIndex {
    sessions: HashMap<Id<Session>, Entity>,
    events: HashMap<(Entity, Id<SerializedEvent>), Entity>,
    operations: HashMap<(Entity, Id<ClientOperation>), Entity>,
    entity_keys: HashMap<Entity, (Id<SerializedEvent>, Option<Id<ClientOperation>>)>,
    timestamps: HashMap<Entity, CreatedAt>,
    snapshot_events:
        HashMap<Entity, HashMap<Id<SerializedEvent>, (Entity, Option<Id<ClientOperation>>)>>,
    latest_sequences: HashMap<Entity, u64>,
    latest_users: HashMap<Entity, (u64, Id<SerializedEvent>)>,
    reply_ids: HashMap<Entity, (Entity, Id<SerializedEvent>)>,
}

impl EventIndex {
    fn record_sequence(
        &mut self,
        session: Entity,
        event_id: &Id<SerializedEvent>,
        event_sequence: u64,
        message: &Message,
    ) {
        let sequence = self.latest_sequences.entry(session).or_default();
        *sequence = (*sequence).max(event_sequence);
        if matches!(message, Message::User { .. })
            && self
                .latest_users
                .get(&session)
                .is_none_or(|(latest, _)| *latest <= event_sequence)
        {
            self.latest_users
                .insert(session, (event_sequence, event_id.clone()));
        }
    }

    fn resolve_replies(&self, commands: &mut Commands) {
        for (source, (session, reply_id)) in &self.reply_ids {
            let target = self.events.get(&(*session, reply_id.clone())).copied();
            let mut entity = commands.entity(*source);
            match target {
                Some(target) if target != *source => {
                    entity.insert(ReplyTo(target));
                }
                _ => {
                    entity.remove::<ReplyTo>();
                }
            }
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
    fn append(&mut self, index: &mut EventIndex, session_id: &Id<Session>, messages: &[Message]) {
        let Some(&session) = index.sessions.get(session_id) else {
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
            let event_id = Id::<SerializedEvent>::new(format!(
                "session:{}:event:{sequence}",
                session_id.as_str()
            ));
            let is_user = matches!(message, Message::User { .. });
            let event = SerializedEvent {
                event_id: event_id.clone(),
                session_id: session_id.clone(),
                actor_id: if is_user {
                    SerializedMember::local_id(session_id)
                } else {
                    SerializedMember::agent_id(session_id)
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
        index: &mut EventIndex,
        event: SerializedEvent,
        delivery: MessageDelivery,
        source: MaterializationSource,
    ) {
        let Some(&session) = index.sessions.get(&event.session_id) else {
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
        let timestamp = CreatedAt::new(i64::try_from(event.created_at_ms).unwrap_or(i64::MAX));
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
                event.event_id.clone(),
                event.actor_id.clone(),
                Sequence(event.server_seq),
                timestamp,
                MessageContent(event.message.clone()),
                delivery,
            ));
            match &event.client_op_id {
                Some(client_op_id) => {
                    entity_commands.insert(client_op_id.clone());
                }
                None => {
                    entity_commands.remove::<Id<ClientOperation>>();
                }
            }
            match &event.reply_to {
                Some(reply_to) => {
                    entity_commands.insert(ReplyToId(reply_to.clone()));
                }
                None => {
                    entity_commands.remove::<ReplyToId>();
                    entity_commands.remove::<ReplyTo>();
                }
            }
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
                event.event_id.clone(),
                event.actor_id.clone(),
                Sequence(event.server_seq),
                timestamp,
                MessageContent(event.message.clone()),
                delivery,
                ChildOf(session),
            ));
            if let Some(client_op_id) = &event.client_op_id {
                entity_commands.insert(client_op_id.clone());
            }
            if let Some(reply_to) = &event.reply_to {
                entity_commands.insert(ReplyToId(reply_to.clone()));
            }
            if source == MaterializationSource::Snapshot {
                entity_commands.insert(SnapshotEvent);
            }
            entity_commands.id()
        };
        index
            .events
            .insert((session, event.event_id.clone()), entity);
        index.record_sequence(session, &event.event_id, event.server_seq, &event.message);
        if let Some(client_op_id) = event.client_op_id.clone() {
            index.operations.insert((session, client_op_id), entity);
        }
        match &event.reply_to {
            Some(reply_to) => {
                index.reply_ids.insert(entity, (session, reply_to.clone()));
            }
            None => {
                index.reply_ids.remove(&entity);
            }
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
    use bevy_ecs::system::RunSystemOnce;
    use vmux_api::session::AssistantBlock;

    #[test]
    fn snapshot_materializes_ordered_events_under_session() {
        let mut app = App::new();
        app.add_plugins(ConversationPlugin);
        let session = app
            .world_mut()
            .spawn((Session, Id::<Session>::new("session-1")))
            .id();
        app.update();
        app.world_mut()
            .resource_mut::<Messages<SnapshotReceived>>()
            .write(SnapshotReceived {
                session: Id::<Session>::new("session-1"),
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
        let mut events = app
            .world_mut()
            .query_filtered::<(Entity, &Id<SerializedEvent>, &Sequence), With<ConversationEvent>>()
            .iter(app.world())
            .map(|(entity, id, sequence)| (entity, id.clone(), sequence.0))
            .collect::<Vec<_>>();
        events.sort_by_key(|(_, _, sequence)| *sequence);
        assert_eq!(
            app.world().get::<ReplyTo>(events[1].0).map(|reply| reply.0),
            Some(events[0].0)
        );
        assert_eq!(
            app.world()
                .get::<ReplyToId>(events[1].0)
                .map(|reply| reply.0.clone()),
            Some(events[0].1.clone())
        );
    }

    #[test]
    fn agent_added_after_session_gets_a_member() {
        let mut app = App::new();
        app.add_plugins(ConversationPlugin);
        let session = app
            .world_mut()
            .spawn((Session, Id::<Session>::new("session-1")))
            .id();
        app.update();

        app.world_mut()
            .entity_mut(session)
            .insert(AgentRef::new("codex"));
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
        let session_id = Id::<Session>::new("session-1");
        let session = app.world_mut().spawn((Session, session_id.clone())).id();
        let client_op_id = Id::<ClientOperation>::new("op-1");
        let pending = app
            .world_mut()
            .spawn((
                ConversationEvent,
                MaterializedEvent,
                Id::<SerializedEvent>::new("pending-1"),
                SerializedMember::local_id(&session_id),
                client_op_id.clone(),
                Sequence(0),
                CreatedAt::new(1),
                MessageContent(Message::user("draft")),
                MessageDelivery::Pending(client_op_id.clone()),
                ChildOf(session),
            ))
            .id();
        app.update();
        app.world_mut()
            .resource_mut::<Messages<OperationCommitted>>()
            .write(OperationCommitted(SerializedEvent {
                event_id: Id::new("event-1"),
                session_id,
                actor_id: Id::new("member-1"),
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
        let session_id = Id::<Session>::new("session-1");
        let session = app.world_mut().spawn((Session, session_id.clone())).id();
        app.world_mut()
            .resource_mut::<Messages<OperationCommitted>>()
            .write(OperationCommitted(SerializedEvent {
                event_id: Id::new("operation-1"),
                session_id: session_id.clone(),
                actor_id: SerializedMember::local_id(&session_id),
                client_op_id: Some(Id::new("op-1")),
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
        let session_id = Id::<Session>::new("session-1");
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
        let session_id = Id::<Session>::new("session-1");
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
                event_id: Id::new("session:session-1:event:1"),
                session_id: session_id.clone(),
                actor_id: SerializedMember::local_id(&session_id),
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
        let session_id = Id::<Session>::new("session-1");
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
                event_id: Id::new("session:session-1:event:1"),
                session_id: session_id.clone(),
                actor_id: SerializedMember::local_id(&session_id),
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
        let session_id = Id::<Session>::new("session-1");
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
        app.world_mut().entity_mut(event).insert(CreatedAt::new(42));
        app.world_mut()
            .resource_mut::<Messages<SnapshotReceived>>()
            .write(SnapshotReceived {
                session: session_id,
                messages: vec![Message::user("hello")],
            });

        app.update();

        assert_eq!(
            app.world()
                .get::<CreatedAt>(event)
                .map(|value| value.milliseconds()),
            Some(42)
        );
    }
}
