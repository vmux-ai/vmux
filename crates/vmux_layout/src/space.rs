use bevy::prelude::*;
use moonshine_save::prelude::*;

pub struct SpacePlugin;

impl Plugin for SpacePlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Space>()
            .register_type::<SpaceId>()
            .register_type::<ActiveSpaceTag>()
            .init_resource::<ActiveSpaceEntity>()
            .add_systems(
                Update,
                (ensure_active_space_tagged, sync_active_space_entity).chain(),
            );
    }
}

#[derive(Component, Reflect, Default)]
#[reflect(Component)]
#[type_path = "vmux_desktop::space"]
#[require(Save)]
pub struct Space;

#[derive(Component, Reflect, Default, Clone, PartialEq, Eq)]
#[reflect(Component)]
#[type_path = "vmux_desktop::space"]
#[require(Save)]
pub struct SpaceId(pub String);

#[derive(Component, Reflect, Default)]
#[reflect(Component)]
#[type_path = "vmux_desktop::space"]
#[require(Save)]
pub struct ActiveSpaceTag;

#[derive(Resource, Default, Debug, PartialEq, Eq)]
pub struct ActiveSpaceEntity(pub Option<Entity>);

pub fn sync_active_space_entity(
    tagged: Query<Entity, With<ActiveSpaceTag>>,
    mut active: ResMut<ActiveSpaceEntity>,
) {
    let current = tagged.iter().next();
    if active.0 != current {
        active.0 = current;
    }
}

pub fn ensure_active_space_tagged(
    tagged: Query<(), With<ActiveSpaceTag>>,
    spaces: Query<Entity, With<Space>>,
    mut commands: Commands,
) {
    if !tagged.is_empty() {
        return;
    }
    if let Some(entity) = spaces.iter().next() {
        commands.entity(entity).insert(ActiveSpaceTag);
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
            .spawn((Space, SpaceId("default".to_string()), ActiveSpaceTag))
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
    fn ensure_active_space_tagged_tags_sole_untagged_space() {
        let mut app = App::new();
        app.init_resource::<ActiveSpaceEntity>().add_systems(
            Update,
            (ensure_active_space_tagged, sync_active_space_entity).chain(),
        );
        let space = app
            .world_mut()
            .spawn((Space, SpaceId("default".to_string())))
            .id();
        app.update();
        assert!(app.world().entity(space).contains::<ActiveSpaceTag>());
        assert_eq!(app.world().resource::<ActiveSpaceEntity>().0, Some(space));
    }

    #[test]
    fn ensure_active_space_tagged_is_noop_when_already_tagged() {
        let mut app = App::new();
        app.add_systems(Update, ensure_active_space_tagged);
        let a = app
            .world_mut()
            .spawn((Space, SpaceId("a".to_string()), ActiveSpaceTag))
            .id();
        let b = app
            .world_mut()
            .spawn((Space, SpaceId("b".to_string())))
            .id();
        app.update();
        assert!(app.world().entity(a).contains::<ActiveSpaceTag>());
        assert!(!app.world().entity(b).contains::<ActiveSpaceTag>());
    }
}
