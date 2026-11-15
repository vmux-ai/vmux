use std::path::PathBuf;
use std::time::SystemTime;

use bevy_ecs::prelude::Component;

#[derive(Component, Clone)]
pub struct Entry {
    pub path: PathBuf,
    pub modified: SystemTime,
    pub directory: bool,
    pub symlink: bool,
}
