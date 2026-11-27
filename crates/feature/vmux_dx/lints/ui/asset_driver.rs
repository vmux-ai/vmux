#![deny(vmux_ecs_driver_io)]

use std::path::PathBuf;

use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;

#[derive(Component)]
struct AssetPath(PathBuf);

impl AssetPath {
    fn read(&self, name: &str) -> Option<Vec<u8>> {
        std::fs::read(self.0.join(name)).ok()
    }
}

struct AssetDriver {
    root: PathBuf,
}

impl AssetDriver {
    fn read(&self, name: &str) -> Option<Vec<u8>> {
        std::fs::read(self.root.join(name)).ok()
    }
}

fn use_assets(paths: Query<&AssetPath>) {
    for path in &paths {
        let _ = path.read("index.css");
        let driver = AssetDriver {
            root: path.0.clone(),
        };
        let _ = driver.read("index.css");
    }
}

struct DemoPlugin;

impl Plugin for DemoPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, use_assets);
    }
}

fn main() {
    let mut app = App::new();
    app.add_plugins(DemoPlugin);
}
