use bevy::prelude::*;
use moonshine_save::prelude::*;

pub struct SpacePlugin;

impl Plugin for SpacePlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Space>()
            .register_type::<SpaceId>()
            .register_type::<ActiveSpaceTag>()
            .init_resource::<ActiveSpaceEntity>()
            .add_systems(Update, sync_active_space_entity);
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
}
