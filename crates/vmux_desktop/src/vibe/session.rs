use bevy::prelude::*;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::SystemTime;

#[derive(Component, Debug)]
pub struct Vibe;

#[derive(Component, Debug, Clone)]
pub struct SessionId(pub String);

#[derive(Component, Debug)]
#[allow(dead_code)]
pub struct PendingVibeSession {
    pub spawn_time: SystemTime,
    pub cwd: PathBuf,
    pub attempts: u8,
}

#[derive(Resource, Default, Debug)]
pub struct VibeSessionToEntity(pub HashMap<String, Entity>);

pub fn track_session_id_inserts(
    mut map: ResMut<VibeSessionToEntity>,
    inserted: Query<(Entity, &SessionId), (Added<SessionId>, With<Vibe>)>,
) {
    for (entity, SessionId(id)) in &inserted {
        map.0.insert(id.clone(), entity);
    }
}

pub fn track_session_id_removals(
    mut map: ResMut<VibeSessionToEntity>,
    mut removed: RemovedComponents<SessionId>,
) {
    for entity in removed.read() {
        map.0.retain(|_, &mut e| e != entity);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_app() -> App {
        let mut app = App::new();
        app.init_resource::<VibeSessionToEntity>();
        app.add_systems(
            Update,
            (track_session_id_inserts, track_session_id_removals).chain(),
        );
        app
    }

    #[test]
    fn session_insert_populates_map_only_for_vibe_entities() {
        let mut app = make_app();
        let with_vibe = app.world_mut().spawn((Vibe, SessionId("abc".into()))).id();
        let without_vibe = app.world_mut().spawn(SessionId("xyz".into())).id();
        app.update();
        let map = app.world().resource::<VibeSessionToEntity>();
        assert_eq!(map.0.get("abc"), Some(&with_vibe));
        assert!(!map.0.contains_key("xyz"));
        let _ = without_vibe;
    }

    #[test]
    fn entity_despawn_removes_session_from_map() {
        let mut app = make_app();
        let e = app.world_mut().spawn((Vibe, SessionId("def".into()))).id();
        app.update();
        app.world_mut().despawn(e);
        app.update();
        let map = app.world().resource::<VibeSessionToEntity>();
        assert!(!map.0.contains_key("def"));
    }
}
