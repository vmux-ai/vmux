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
use vmux_ecs::host::persistence::PersistenceAppExt;

use crate::{AgentId, Session, SessionId};

pub(crate) struct ConversationPlugin;

impl Plugin for ConversationPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ConversationSnapshotReceived>()
            .add_message::<ConversationOperationReceived>()
            .add_message::<ConversationOperationCommitted>()
            .register_persisted::<Document>()
            .register_persisted::<DocumentKind>()
            .add_systems(Update, (ensure, sync_agent, materialize));
    }
}

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct Member {
    pub id: MemberId,
    pub display_name: String,
    pub role: MemberRole,
    pub kind: MemberKind,
}

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
pub struct ConversationSnapshotReceived {
    pub session: SessionId,
    pub messages: Vec<Message>,
}

#[derive(Message, Clone, Debug)]
pub struct ConversationOperationReceived(pub SerializedEvent);

#[derive(Message, Clone, Debug)]
pub struct ConversationOperationCommitted(pub SerializedEvent);

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Transcript {
    pub messages: Vec<Message>,
    pub created_at: Vec<u64>,
}

impl Transcript {
    fn from_items(mut items: Vec<(u64, Message, u64)>) -> Self {
        items.sort_by_key(|(sequence, _, _)| *sequence);
        Self {
            messages: items
                .iter()
                .map(|(_, message, _)| message.clone())
                .collect(),
            created_at: items
                .into_iter()
                .map(|(_, _, created_at)| created_at)
                .collect(),
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
    members: Query<&Member>,
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
                    .is_ok_and(|member| member.kind == MemberKind::Human)
            });
        if !has_local {
            commands.spawn((
                Member {
                    id: MemberId::local(id),
                    display_name: "You".to_string(),
                    role: MemberRole::Owner,
                    kind: MemberKind::Human,
                },
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
    mut members: Query<&mut Member>,
    mut commands: Commands,
) {
    for (session, session_id, agent_id, children) in &sessions {
        let mut found = false;
        for child in children.into_iter().flat_map(|children| children.iter()) {
            let Ok(mut member) = members.get_mut(child) else {
                continue;
            };
            if member.kind != MemberKind::Agent {
                continue;
            }
            member.id = MemberId::agent(session_id);
            member.display_name.clone_from(&agent_id.0);
            member.role = MemberRole::Participant;
            found = true;
        }
        if !found {
            commands.spawn((
                Member {
                    id: MemberId::agent(session_id),
                    display_name: agent_id.0.clone(),
                    role: MemberRole::Participant,
                    kind: MemberKind::Agent,
                },
                ChildOf(session),
            ));
        }
    }
}

fn materialize(
    mut snapshots: MessageReader<ConversationSnapshotReceived>,
    mut received: MessageReader<ConversationOperationReceived>,
    mut committed: MessageReader<ConversationOperationCommitted>,
    sessions: Query<(Entity, &SessionId), With<Session>>,
    existing: Query<
        (Entity, &EventIdentity, &ChildOf, Option<&MessageDelivery>),
        (With<ConversationEvent>, With<MaterializedEvent>),
    >,
    mut commands: Commands,
) {
    let session_entities = sessions
        .iter()
        .map(|(entity, id)| (id.clone(), entity))
        .collect::<HashMap<_, _>>();
    let mut event_entities = existing
        .iter()
        .map(|(entity, identity, parent, _)| ((parent.parent(), identity.event_id.clone()), entity))
        .collect::<HashMap<_, _>>();
    let mut operation_entities = existing
        .iter()
        .filter_map(|(entity, identity, parent, _)| {
            identity
                .client_op_id
                .clone()
                .map(|id| ((parent.parent(), id), entity))
        })
        .collect::<HashMap<_, _>>();
    for request in snapshots.read() {
        let Some(&session) = session_entities.get(&request.session) else {
            continue;
        };
        let mut stale = existing
            .iter()
            .filter(|(_, _, parent, delivery)| {
                parent.parent() == session && !matches!(delivery, Some(MessageDelivery::Pending(_)))
            })
            .map(|(entity, identity, _, _)| (identity.event_id.clone(), entity))
            .collect::<HashMap<_, _>>();
        let now = vmux_ecs::UnixMillis::now().0.max(0) as u64;
        for event in SerializedEvent::from_messages(&request.session, now, &request.messages) {
            stale.remove(&event.event_id);
            materialize_event(
                event,
                MessageDelivery::Committed,
                &session_entities,
                &mut event_entities,
                &mut operation_entities,
                &mut commands,
            );
        }
        for entity in stale.into_values().collect::<HashSet<_>>() {
            commands.entity(entity).despawn();
        }
    }
    for operation in received.read() {
        materialize_event(
            operation.0.clone(),
            MessageDelivery::Committed,
            &session_entities,
            &mut event_entities,
            &mut operation_entities,
            &mut commands,
        );
    }
    for operation in committed.read() {
        materialize_event(
            operation.0.clone(),
            MessageDelivery::Committed,
            &session_entities,
            &mut event_entities,
            &mut operation_entities,
            &mut commands,
        );
    }
}

fn materialize_event(
    event: SerializedEvent,
    delivery: MessageDelivery,
    sessions: &HashMap<SessionId, Entity>,
    event_entities: &mut HashMap<(Entity, EventId), Entity>,
    operation_entities: &mut HashMap<(Entity, ClientOpId), Entity>,
    commands: &mut Commands,
) {
    let Some(&session) = sessions.get(&event.session_id) else {
        return;
    };
    let existing = event_entities
        .get(&(session, event.event_id.clone()))
        .copied()
        .or_else(|| {
            event
                .client_op_id
                .as_ref()
                .and_then(|id| operation_entities.get(&(session, id.clone())).copied())
        });
    let identity = EventIdentity::from_event(&event);
    let created_at = CreatedAt(i64::try_from(event.created_at_ms).unwrap_or(i64::MAX));
    let entity = if let Some(entity) = existing {
        commands.entity(entity).insert((
            identity,
            created_at,
            MessageContent(event.message.clone()),
            delivery,
        ));
        entity
    } else {
        commands
            .spawn((
                ConversationEvent,
                MaterializedEvent,
                identity,
                created_at,
                MessageContent(event.message.clone()),
                delivery,
                ChildOf(session),
            ))
            .id()
    };
    event_entities.insert((session, event.event_id), entity);
    if let Some(client_op_id) = event.client_op_id {
        operation_entities.insert((session, client_op_id), entity);
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
            .resource_mut::<Messages<ConversationSnapshotReceived>>()
            .write(ConversationSnapshotReceived {
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
            .filter_map(|child| app.world().get::<Member>(child))
            .collect::<Vec<_>>();
        assert_eq!(members.len(), 2);
        assert!(
            members
                .iter()
                .any(|member| member.kind == MemberKind::Human)
        );
        assert!(
            members.iter().any(|member| {
                member.kind == MemberKind::Agent && member.display_name == "codex"
            })
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
            .resource_mut::<Messages<ConversationOperationCommitted>>()
            .write(ConversationOperationCommitted(SerializedEvent {
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
}
