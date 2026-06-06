use bevy::prelude::*;
use moonshine_save::prelude::*;

pub struct SpacePlugin;

impl Plugin for SpacePlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Space>();
    }
}

#[derive(Component, Reflect, Default)]
#[reflect(Component)]
#[type_path = "vmux_desktop::space"]
#[require(Save)]
pub struct Space;
