use bevy::prelude::*;
use moonshine_save::prelude::*;

pub struct SpacePlugin;

impl Plugin for SpacePlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Space>()
            .register_type::<SpaceId>()
            .init_resource::<ActiveSpaceEntity>()
            .init_resource::<ActiveSpaceId>()
            .add_systems(
                Update,
                (
                    crate::active::ensure_active_space,
                    sync_active_space_entity,
                    sync_active_space_id,
                )
                    .chain(),
            )
            .add_systems(
                Update,
                (
                    crate::active::ensure_active_tab,
                    crate::active::ensure_active_stack,
                    crate::active::ensure_active_branch,
                ),
            )
            .add_systems(
                PostUpdate,
                sync_space_container_visibility.before(bevy::ui::UiSystems::Layout),
            );
    }
}

#[derive(Component, Reflect, Default)]
#[reflect(Component)]
#[type_path = "vmux_desktop::space"]
#[require(Save)]
pub struct Space;

#[derive(Component, Reflect, Default, Clone, Debug, PartialEq, Eq)]
#[reflect(Component)]
#[type_path = "vmux_desktop::space"]
#[require(Save)]
pub struct SpaceId(pub String);

#[derive(Resource, Default, Debug, PartialEq, Eq)]
pub struct ActiveSpaceEntity(pub Option<Entity>);

#[derive(Resource, Default, Debug, PartialEq, Eq)]
pub struct ActiveSpaceId(pub Option<String>);

pub fn sync_active_space_entity(
    tagged: Query<Entity, (With<Space>, With<vmux_core::Active>)>,
    mut active: ResMut<ActiveSpaceEntity>,
) {
    let current = tagged.iter().next();
    if active.0 != current {
        active.0 = current;
    }
}

pub fn sync_active_space_id(
    active: Res<ActiveSpaceEntity>,
    ids: Query<&SpaceId>,
    mut active_id: ResMut<ActiveSpaceId>,
) {
    let current = active
        .0
        .and_then(|entity| ids.get(entity).ok())
        .map(|id| id.0.clone());
    if active_id.0 != current {
        active_id.0 = current;
    }
}

pub fn space_of(
    entity: Entity,
    child_of: &Query<&ChildOf>,
    spaces: &Query<(), With<Space>>,
) -> Option<Entity> {
    let mut current = entity;
    loop {
        if spaces.get(current).is_ok() {
            return Some(current);
        }
        match child_of.get(current) {
            Ok(parent) => current = parent.parent(),
            Err(_) => return None,
        }
    }
}

pub fn space_container_node() -> Node {
    Node {
        position_type: PositionType::Absolute,
        left: Val::Px(0.0),
        right: Val::Px(0.0),
        top: Val::Px(0.0),
        bottom: Val::Px(0.0),
        width: Val::Percent(100.0),
        height: Val::Percent(100.0),
        ..default()
    }
}

pub fn space_view_bundle() -> impl Bundle {
    (
        space_container_node(),
        Transform::default(),
        GlobalTransform::default(),
        Visibility::default(),
    )
}

pub fn sync_space_container_visibility(
    mut spaces: Query<(&mut Node, &mut Visibility, Has<vmux_core::Active>), With<Space>>,
) {
    for (mut node, mut vis, active) in &mut spaces {
        let target_display = if active { Display::Flex } else { Display::None };
        if node.display != target_display {
            node.display = target_display;
        }
        let target_vis = if active {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *vis != target_vis {
            *vis = target_vis;
        }
    }
}

#[cfg(test)]
mod tests {
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
}
