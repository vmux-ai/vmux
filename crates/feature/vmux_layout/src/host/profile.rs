use bevy::prelude::*;
use moonshine_save::prelude::*;

pub use vmux_core::profile::{cef_cache_path, profile_dir, session_path, shared_data_dir};

#[derive(Component, Reflect, Default)]
#[reflect(Component)]
#[type_path = "vmux_desktop::profile"]
#[require(Save)]
pub struct Profile {
    pub name: String,
}
