use std::collections::{HashMap, HashSet};

use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use bevy_ecs::system::SystemParam;
use bevy_reflect::Reflect;
use moonshine_save::prelude::Save;
use serde::{Deserialize, Serialize};
use vmux_api::conversation::{
    ClientOpId, ConversationEvent as SerializedEvent, EventId, MemberId, MemberKind, MemberRole,
    Message,
};
use vmux_ecs::CreatedAt;
use vmux_ecs::persistence::PersistenceAppExt;

use crate::{AgentId, Session, SessionId};

pub(crate) struct ConversationPlugin;

impl Plugin for ConversationPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<SnapshotReceived>()
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

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct EventIdentity {
    pub event_id: EventId,
    pub actor_id: MemberId,
    pub client_op_id: Option<ClientOpId>,
    pub sequence: u64,
    pub reply_to: Option<EventId>,
}

#[derive(Component, Clone, Debug, PartialEq)]
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
    mut received: MessageReader<OperationReceived>,
    mut committed: MessageReader<OperationCommitted>,
    sessions: Query<(Entity, &SessionId, Option<&Children>), With<Session>>,
    existing: Query<
        (Entity, &EventIdentity, &CreatedAt, Has<SnapshotEvent>),
        (With<ConversationEvent>, With<MaterializedEvent>),
    >,
    mut materializer: EventMaterializer,
) {
    if snapshots.is_empty() && received.is_empty() && committed.is_empty() {
        return;
    }
    let snapshots = snapshots.read().collect::<Vec<_>>();
    let received = received.read().collect::<Vec<_>>();
    let committed = committed.read().collect::<Vec<_>>();
    let requested = snapshots
        .iter()
        .map(|snapshot| &snapshot.session)
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
    };
    for (session, id, children) in &sessions {
        if !requested.contains(id) {
            continue;
        }
        index.sessions.insert(id.clone(), session);
        for child in children.into_iter().flat_map(|children| children.iter()) {
            let Ok((entity, identity, timestamp, from_snapshot)) = existing.get(child) else {
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
            if from_snapshot {
                index.snapshot_events.entry(session).or_default().insert(
                    identity.event_id.clone(),
                    (entity, identity.client_op_id.clone()),
                );
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
            materializer.commands.entity(entity).despawn();
        }
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

struct EventIndex {
    sessions: HashMap<SessionId, Entity>,
    events: HashMap<(Entity, EventId), Entity>,
    operations: HashMap<(Entity, ClientOpId), Entity>,
    entity_keys: HashMap<Entity, (EventId, Option<ClientOpId>)>,
    timestamps: HashMap<Entity, CreatedAt>,
    snapshot_events: HashMap<Entity, HashMap<EventId, (Entity, Option<ClientOpId>)>>,
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
    use bevy_ecs::system::RunSystemOnce;
    use vmux_api::conversation::AssistantBlock;

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
