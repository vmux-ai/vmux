use std::collections::{HashMap, HashSet};

use bevy::prelude::*;
use vmux_wire::room::{
    ClientOpId, EventId, MemberId, MemberKind, Message, RoomEvent, RoomId, RoomRole,
};

use crate::client::acp::AcpSession;
use crate::components::{AgentConversationTitle, AgentMessages, AgentSession};

/// Projects each agent session into a collaborative room: members, a draft document, and one
/// materialized event per message, kept in step with the session's transcript.
pub struct RoomPlugin;

impl Plugin for RoomPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RoomIndex>()
            .init_resource::<RoomEventIndex>()
            .add_message::<RoomIntent>()
            .add_message::<RoomOpReceived>()
            .add_message::<RoomOpCommitted>()
            .add_message::<CrdtChangeReceived>()
            .add_systems(
                PostUpdate,
                (
                    ensure_implicit_rooms,
                    sync_room_messages,
                    sync_room_titles,
                    cleanup_orphaned_rooms,
                )
                    .chain(),
            );
    }
}

#[derive(Component, Clone, Debug, Eq, PartialEq)]
pub struct ChatRoom {
    pub room_id: RoomId,
}

#[derive(Component, Clone, Debug, Default, Eq, PartialEq)]
pub struct RoomMetadata {
    pub title: String,
}

#[derive(Component, Clone, Debug, Eq, PartialEq)]
pub struct RoomProjection {
    pub source_sid: String,
    pub through_seq: u64,
}

#[derive(Component, Clone, Debug, Eq, PartialEq)]
pub struct RoomMember {
    pub room_id: RoomId,
    pub member_id: MemberId,
    pub display_name: String,
    pub role: RoomRole,
    pub kind: MemberKind,
}

#[derive(Component, Clone, Debug, Default, Eq, PartialEq)]
pub struct MemberPresence {
    pub online: bool,
    pub last_seen_ms: u64,
}

#[derive(Component, Clone, Debug, Eq, PartialEq)]
pub struct RoomAgentBinding {
    pub room_id: RoomId,
    pub member_id: MemberId,
}

#[derive(Component, Clone, Debug, Eq, PartialEq)]
pub struct RoomEventIdentity {
    pub event_id: EventId,
    pub actor_id: MemberId,
    pub client_op_id: Option<ClientOpId>,
    pub server_seq: u64,
    pub created_at_ms: u64,
    pub reply_to: Option<EventId>,
}

#[derive(Component, Clone, Debug, PartialEq)]
pub struct RoomMessageContent(pub Message);

#[derive(Component, Clone, Debug, Eq, PartialEq)]
pub enum MessageDelivery {
    Pending(ClientOpId),
    Committed,
    Failed(String),
}

#[derive(Component, Clone, Debug, Eq, PartialEq)]
pub struct StreamingMessage {
    pub actor_id: MemberId,
    pub text: String,
}

#[derive(Component, Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MaterializedRoomEvent;

#[derive(Component, Clone, Debug, Eq, PartialEq)]
pub struct CollaborativeDocument {
    pub room_id: RoomId,
    pub document_id: String,
    pub kind: DocumentKind,
    pub revision: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DocumentKind {
    Draft,
    Notes,
    Plan,
}

#[derive(Resource, Default)]
pub struct RoomIndex(pub HashMap<RoomId, Entity>);

#[derive(Resource, Default)]
pub struct RoomEventIndex(pub HashMap<EventId, Entity>);

#[derive(Message, Clone, Debug, PartialEq)]
pub enum RoomIntent {
    Append {
        room_id: RoomId,
        actor_id: MemberId,
        client_op_id: ClientOpId,
        message: Message,
    },
}

#[derive(Message, Clone, Debug, PartialEq)]
pub struct RoomOpReceived(pub vmux_wire::room::RoomEvent);

#[derive(Message, Clone, Debug, PartialEq)]
pub struct RoomOpCommitted(pub vmux_wire::room::RoomEvent);

#[derive(Message, Clone, Debug, Eq, PartialEq)]
pub struct CrdtChangeReceived {
    pub room_id: RoomId,
    pub document_id: String,
    pub actor_id: MemberId,
    pub change: Vec<u8>,
}

fn session_identity<'a>(
    page: Option<&'a AgentSession>,
    acp: Option<&'a AcpSession>,
) -> Option<(&'a str, &'a str)> {
    page.map(|session| (session.sid.as_str(), session.provider.as_str()))
        .or_else(|| acp.map(|session| (session.sid.as_str(), session.agent_id.as_str())))
}

fn ensure_implicit_rooms(
    mut commands: Commands,
    sessions: Query<
        (
            Entity,
            &AgentMessages,
            Option<&AgentConversationTitle>,
            Option<&AgentSession>,
            Option<&AcpSession>,
        ),
        (
            Or<(With<AgentSession>, With<AcpSession>)>,
            Without<RoomAgentBinding>,
        ),
    >,
    mut rooms: ResMut<RoomIndex>,
    mut event_index: ResMut<RoomEventIndex>,
) {
    for (session_entity, messages, title, page, acp) in &sessions {
        let Some((sid, agent_name)) = session_identity(page, acp) else {
            continue;
        };
        let room_id = RoomId::for_session(sid);
        let agent_id = MemberId::agent(&room_id);
        let room_entity = rooms.0.get(&room_id).copied().unwrap_or_else(|| {
            let title = title
                .map(|title| title.0.clone())
                .filter(|title| !title.is_empty())
                .unwrap_or_else(|| agent_name.to_string());
            let room_entity = commands
                .spawn((
                    ChatRoom {
                        room_id: room_id.clone(),
                    },
                    RoomMetadata { title },
                    RoomProjection {
                        source_sid: sid.to_string(),
                        through_seq: messages.0.len() as u64,
                    },
                ))
                .id();
            commands.spawn((
                RoomMember {
                    room_id: room_id.clone(),
                    member_id: MemberId::local(&room_id),
                    display_name: "You".to_string(),
                    role: RoomRole::Owner,
                    kind: MemberKind::Human,
                },
                MemberPresence {
                    online: true,
                    last_seen_ms: 0,
                },
                ChildOf(room_entity),
            ));
            commands.spawn((
                RoomMember {
                    room_id: room_id.clone(),
                    member_id: agent_id.clone(),
                    display_name: agent_name.to_string(),
                    role: RoomRole::Participant,
                    kind: MemberKind::Agent,
                },
                MemberPresence {
                    online: true,
                    last_seen_ms: 0,
                },
                ChildOf(room_entity),
            ));
            commands.spawn((
                CollaborativeDocument {
                    room_id: room_id.clone(),
                    document_id: format!("{}:draft", room_id.as_str()),
                    kind: DocumentKind::Draft,
                    revision: 0,
                },
                ChildOf(room_entity),
            ));
            materialize_events(
                &mut commands,
                room_entity,
                sid,
                &messages.0,
                &mut event_index,
            );
            rooms.0.insert(room_id.clone(), room_entity);
            room_entity
        });
        commands.entity(session_entity).insert(RoomAgentBinding {
            room_id,
            member_id: agent_id,
        });
        commands.entity(room_entity).insert(RoomProjection {
            source_sid: sid.to_string(),
            through_seq: messages.0.len() as u64,
        });
    }
}

fn materialize_events(
    commands: &mut Commands,
    room_entity: Entity,
    sid: &str,
    messages: &[Message],
    event_index: &mut RoomEventIndex,
) {
    for event in RoomEvent::from_messages(sid, 0, messages) {
        let event_entity = commands
            .spawn((
                MaterializedRoomEvent,
                RoomEventIdentity {
                    event_id: event.event_id.clone(),
                    actor_id: event.actor_id,
                    client_op_id: event.client_op_id,
                    server_seq: event.server_seq,
                    created_at_ms: event.created_at_ms,
                    reply_to: event.reply_to,
                },
                RoomMessageContent(event.message),
                MessageDelivery::Committed,
                ChildOf(room_entity),
            ))
            .id();
        event_index.0.insert(event.event_id, event_entity);
    }
}

fn sync_room_messages(
    mut commands: Commands,
    sessions: Query<
        (
            &AgentMessages,
            &RoomAgentBinding,
            Option<&AgentSession>,
            Option<&AcpSession>,
        ),
        Changed<AgentMessages>,
    >,
    rooms: Res<RoomIndex>,
    existing: Query<(Entity, &RoomEventIdentity, &ChildOf), With<MaterializedRoomEvent>>,
    mut projections: Query<&mut RoomProjection>,
    mut event_index: ResMut<RoomEventIndex>,
) {
    for (messages, binding, page, acp) in &sessions {
        let Some((sid, _)) = session_identity(page, acp) else {
            continue;
        };
        let Some(&room_entity) = rooms.0.get(&binding.room_id) else {
            continue;
        };
        let events = RoomEvent::from_messages(sid, 0, &messages.0);
        let mut stale = existing
            .iter()
            .filter(|(_, _, child_of)| child_of.parent() == room_entity)
            .map(|(entity, identity, _)| (identity.event_id.clone(), entity))
            .collect::<HashMap<_, _>>();
        for event in events {
            if let Some(entity) = stale.remove(&event.event_id) {
                commands.entity(entity).insert((
                    RoomEventIdentity {
                        event_id: event.event_id.clone(),
                        actor_id: event.actor_id,
                        client_op_id: event.client_op_id,
                        server_seq: event.server_seq,
                        created_at_ms: event.created_at_ms,
                        reply_to: event.reply_to,
                    },
                    RoomMessageContent(event.message),
                    MessageDelivery::Committed,
                ));
            } else {
                let event_id = event.event_id.clone();
                let entity = commands
                    .spawn((
                        MaterializedRoomEvent,
                        RoomEventIdentity {
                            event_id: event.event_id,
                            actor_id: event.actor_id,
                            client_op_id: event.client_op_id,
                            server_seq: event.server_seq,
                            created_at_ms: event.created_at_ms,
                            reply_to: event.reply_to,
                        },
                        RoomMessageContent(event.message),
                        MessageDelivery::Committed,
                        ChildOf(room_entity),
                    ))
                    .id();
                event_index.0.insert(event_id, entity);
            }
        }
        for (event_id, entity) in stale {
            commands.entity(entity).despawn();
            event_index.0.remove(&event_id);
        }
        if let Ok(mut projection) = projections.get_mut(room_entity) {
            projection.through_seq = messages.0.len() as u64;
        }
    }
}

fn sync_room_titles(
    sessions: Query<(&AgentConversationTitle, &RoomAgentBinding), Changed<AgentConversationTitle>>,
    rooms: Res<RoomIndex>,
    mut metadata: Query<&mut RoomMetadata>,
) {
    for (title, binding) in &sessions {
        let Some(&room_entity) = rooms.0.get(&binding.room_id) else {
            continue;
        };
        if let Ok(mut metadata) = metadata.get_mut(room_entity)
            && metadata.title != title.0
        {
            metadata.title.clone_from(&title.0);
        }
    }
}

fn cleanup_orphaned_rooms(
    mut commands: Commands,
    sessions: Query<
        (Option<&AgentSession>, Option<&AcpSession>),
        Or<(With<AgentSession>, With<AcpSession>)>,
    >,
    room_entities: Query<(Entity, &ChatRoom, &RoomProjection)>,
    room_events: Query<(&RoomEventIdentity, &ChildOf), With<MaterializedRoomEvent>>,
    mut rooms: ResMut<RoomIndex>,
    mut event_index: ResMut<RoomEventIndex>,
) {
    let live_sids = sessions
        .iter()
        .filter_map(|(page, acp)| session_identity(page, acp).map(|(sid, _)| sid.to_string()))
        .collect::<HashSet<_>>();
    for (entity, room, projection) in &room_entities {
        if live_sids.contains(&projection.source_sid) {
            continue;
        }
        for (event, child_of) in &room_events {
            if child_of.parent() == entity {
                event_index.0.remove(&event.event_id);
            }
        }
        rooms.0.remove(&room.room_id);
        commands.entity(entity).despawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AgentKind, AgentVariant};

    #[test]
    fn projects_agent_session_into_stable_room_entities() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).add_plugins(RoomPlugin);
        let session = app
            .world_mut()
            .spawn((
                AgentSession {
                    kind: AgentKind::Codex,
                    variant: AgentVariant::Page,
                    sid: "session-1".to_string(),
                    provider: "Codex".to_string(),
                    model: "default".to_string(),
                },
                AgentMessages(vec![Message::user("hello")]),
                AgentConversationTitle("Collaborative room".to_string()),
            ))
            .id();

        app.update();

        let binding = app.world().get::<RoomAgentBinding>(session).unwrap();
        assert_eq!(binding.room_id, RoomId::for_session("session-1"));
        let room_entity = app.world().resource::<RoomIndex>().0[&binding.room_id];
        let first_event_id = EventId::new("session:session-1:event:1");
        let first_event_entity = app.world().resource::<RoomEventIndex>().0[&first_event_id];
        assert_eq!(
            app.world().get::<RoomMetadata>(room_entity),
            Some(&RoomMetadata {
                title: "Collaborative room".to_string()
            })
        );
        let event_count = {
            let world = app.world_mut();
            let mut query = world.query::<&MaterializedRoomEvent>();
            query.iter(world).count()
        };
        assert_eq!(event_count, 1);

        app.world_mut()
            .get_mut::<AgentMessages>(session)
            .unwrap()
            .0
            .push(Message::Assistant {
                blocks: vec![vmux_wire::room::AssistantBlock::Text("hi".to_string())],
            });
        app.world_mut()
            .get_mut::<AgentConversationTitle>(session)
            .unwrap()
            .0 = "Updated room".to_string();
        app.update();

        assert_eq!(
            app.world().resource::<RoomEventIndex>().0[&first_event_id],
            first_event_entity
        );
        assert_eq!(
            app.world().get::<RoomProjection>(room_entity),
            Some(&RoomProjection {
                source_sid: "session-1".to_string(),
                through_seq: 2,
            })
        );
        assert_eq!(
            app.world().get::<RoomMetadata>(room_entity),
            Some(&RoomMetadata {
                title: "Updated room".to_string(),
            })
        );

        app.world_mut().entity_mut(session).despawn();
        app.update();

        assert!(app.world().resource::<RoomIndex>().0.is_empty());
        assert!(app.world().resource::<RoomEventIndex>().0.is_empty());
    }
}
