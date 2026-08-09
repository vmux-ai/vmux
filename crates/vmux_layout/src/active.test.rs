use super::*;

#[test]
fn ensure_active_tab_marks_max_last_activated_child() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_systems(Update, ensure_active_tab);
    let space = app.world_mut().spawn(Space).id();
    let older = app
        .world_mut()
        .spawn((Tab::default(), LastActivatedAt(1), ChildOf(space)))
        .id();
    let newer = app
        .world_mut()
        .spawn((Tab::default(), LastActivatedAt(5), ChildOf(space)))
        .id();
    app.update();
    assert!(app.world().entity(newer).contains::<Active>());
    assert!(!app.world().entity(older).contains::<Active>());
    let active_count = app
        .world_mut()
        .query_filtered::<Entity, (With<Tab>, With<Active>)>()
        .iter(app.world())
        .count();
    assert_eq!(active_count, 1);
}

#[test]
fn ensure_active_tab_moves_active_off_stale_child() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_systems(Update, ensure_active_tab);
    let space = app.world_mut().spawn(Space).id();
    let stale = app
        .world_mut()
        .spawn((Tab::default(), LastActivatedAt(1), Active, ChildOf(space)))
        .id();
    let newer = app
        .world_mut()
        .spawn((Tab::default(), LastActivatedAt(5), ChildOf(space)))
        .id();
    app.update();
    assert!(app.world().entity(newer).contains::<Active>());
    assert!(!app.world().entity(stale).contains::<Active>());
}

#[test]
fn ensure_active_space_marks_max_last_activated_space() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_systems(Update, ensure_active_space);
    let older = app.world_mut().spawn((Space, LastActivatedAt(1))).id();
    let newer = app.world_mut().spawn((Space, LastActivatedAt(9))).id();
    app.update();
    assert!(app.world().entity(newer).contains::<Active>());
    assert!(!app.world().entity(older).contains::<Active>());
}
