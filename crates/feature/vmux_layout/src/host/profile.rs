use bevy::prelude::*;
use moonshine_save::prelude::*;

#[derive(Component, Reflect, Default)]
#[reflect(Component)]
#[type_path = "vmux_desktop::profile"]
#[require(Save)]
pub struct Profile {
    pub name: String,
}
