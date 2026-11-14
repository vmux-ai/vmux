use std::path::PathBuf;
use std::time::SystemTime;

use bevy_ecs::prelude::Component;

#[derive(Component)]
pub struct Selection {
    pub product: String,
}

#[derive(Component, Clone)]
pub struct Entry {
    pub path: PathBuf,
    pub modified: SystemTime,
    pub directory: bool,
    pub symlink: bool,
}

#[derive(Component)]
pub struct Inspected;

#[derive(Component)]
pub struct Target(pub PathBuf);
