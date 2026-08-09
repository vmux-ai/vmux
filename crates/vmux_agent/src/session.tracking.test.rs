use super::*;

fn make_app() -> App {
    let mut app = App::new();
    app.init_resource::<AgentSessionToEntity>().add_systems(
        Update,
        (track_session_id_inserts, track_session_id_removals).chain(),
    );
    app
}

#[test]
fn insert_populates_map_only_for_agent_session_entities() {
    let mut app = make_app();
    let with = app
        .world_mut()
        .spawn((
            AgentSession {
                kind: AgentKind::Codex,
            },
            SessionId("c1".into()),
        ))
        .id();
    let without = app.world_mut().spawn(SessionId("nope".into())).id();
    app.update();
    let map = app.world().resource::<AgentSessionToEntity>();
    assert_eq!(map.0.get(&(AgentKind::Codex, "c1".into())), Some(&with));
    assert!(!map.0.contains_key(&(AgentKind::Codex, "nope".into())));
    let _ = without;
}

#[test]
fn entity_despawn_removes_session_from_map() {
    let mut app = make_app();
    let e = app
        .world_mut()
        .spawn((
            AgentSession {
                kind: AgentKind::Vibe,
            },
            SessionId("v1".into()),
        ))
        .id();
    app.update();
    app.world_mut().despawn(e);
    app.update();
    let map = app.world().resource::<AgentSessionToEntity>();
    assert!(!map.0.contains_key(&(AgentKind::Vibe, "v1".into())));
}
