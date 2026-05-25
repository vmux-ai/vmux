use bevy::prelude::*;
use moonshine_save::prelude::*;

pub use vmux_core::profile::{
    active_profile_name, cef_cache_path, profile_dir, session_path, shared_data_dir,
};

pub struct ProfilePlugin;

impl Plugin for ProfilePlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Profile>();
    }
}

#[derive(Component, Reflect, Default)]
#[reflect(Component)]
#[type_path = "vmux_desktop::profile"]
#[require(Save)]
pub struct Profile {
    pub name: String,
}
