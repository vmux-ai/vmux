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
