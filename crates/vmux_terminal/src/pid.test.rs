use super::*;

fn make_app() -> App {
    let mut app = App::new();
    app.init_resource::<PidToEntity>()
        .add_systems(Update, (track_pid_inserts, track_pid_removals).chain());
    app
}

#[test]
fn pid_insert_populates_map() {
    let mut app = make_app();
    let e = app.world_mut().spawn(Pid(7777)).id();
    app.update();
    let map = app.world().resource::<PidToEntity>();
    assert_eq!(map.0.get(&7777), Some(&e));
}

#[test]
fn entity_despawn_removes_pid_from_map() {
    let mut app = make_app();
    let e = app.world_mut().spawn(Pid(8888)).id();
    app.update();
    app.world_mut().despawn(e);
    app.update();
    let map = app.world().resource::<PidToEntity>();
    assert!(!map.0.contains_key(&8888));
}

#[test]
fn changing_pid_updates_map() {
    let mut app = make_app();
    let e = app.world_mut().spawn(Pid(9000)).id();
    app.update();
    app.world_mut().entity_mut(e).remove::<Pid>();
    app.update();
    app.world_mut().entity_mut(e).insert(Pid(9001));
    app.update();
    let map = app.world().resource::<PidToEntity>();
    assert_eq!(map.0.get(&9001), Some(&e));
    assert!(!map.0.contains_key(&9000));
}
