use super::*;

#[test]
fn active_space_entity_tracks_tagged_space() {
    let mut app = App::new();
    app.init_resource::<ActiveSpaceEntity>()
        .add_systems(Update, sync_active_space_entity);
    let space = app
        .world_mut()
        .spawn((Space, SpaceId("default".to_string()), vmux_core::Active))
        .id();
    app.update();
    assert_eq!(app.world().resource::<ActiveSpaceEntity>().0, Some(space));
}

#[test]
fn active_space_entity_clears_when_no_tag() {
    let mut app = App::new();
    app.init_resource::<ActiveSpaceEntity>()
        .add_systems(Update, sync_active_space_entity);
    app.insert_resource(ActiveSpaceEntity(Some(Entity::from_bits(42))));
    app.update();
    assert_eq!(app.world().resource::<ActiveSpaceEntity>().0, None);
}

#[test]
fn active_space_id_tracks_active_entity() {
    let mut app = App::new();
    app.init_resource::<ActiveSpaceEntity>()
        .init_resource::<ActiveSpaceId>()
        .add_systems(
            Update,
            (sync_active_space_entity, sync_active_space_id).chain(),
        );
    app.world_mut()
        .spawn((Space, SpaceId("work".to_string()), vmux_core::Active));
    app.update();
    assert_eq!(
        app.world().resource::<ActiveSpaceId>().0.as_deref(),
        Some("work")
    );
}

#[test]
fn space_of_walks_up_to_nearest_space() {
    use bevy::ecs::system::RunSystemOnce;
    let mut app = App::new();
    let space = app
        .world_mut()
        .spawn((Space, SpaceId("s".to_string())))
        .id();
    let tab = app
        .world_mut()
        .spawn((crate::tab::Tab::default(), ChildOf(space)))
        .id();
    let stack = app.world_mut().spawn(ChildOf(tab)).id();
    let found = app
        .world_mut()
        .run_system_once(
            move |child_of: Query<&ChildOf>, spaces: Query<(), With<Space>>| {
                space_of(stack, &child_of, &spaces)
            },
        )
        .unwrap();
    assert_eq!(found, Some(space));
}

#[test]
fn space_container_bundle_is_absolute_fill_node() {
    assert_eq!(space_container_node().position_type, PositionType::Absolute);
}

#[test]
fn inactive_space_container_is_hidden_but_alive() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_systems(Update, sync_space_container_visibility);
    let active = app
        .world_mut()
        .spawn((
            Space,
            vmux_core::Active,
            space_container_node(),
            Visibility::default(),
        ))
        .id();
    let bg = app
        .world_mut()
        .spawn((Space, space_container_node(), Visibility::default()))
        .id();
    app.update();
    assert_eq!(
        app.world().get::<Node>(active).unwrap().display,
        Display::Flex
    );
    assert_eq!(app.world().get::<Node>(bg).unwrap().display, Display::None);
    assert!(app.world().get_entity(bg).is_ok());
}
