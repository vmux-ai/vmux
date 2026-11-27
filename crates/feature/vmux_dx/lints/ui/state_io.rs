use std::path::PathBuf;

use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;

#[derive(Resource)]
struct AssetConfig {
    root: PathBuf,
}

impl AssetConfig {
    fn read(&self, name: &str) -> Option<Vec<u8>> {
        std::fs::read(self.root.join(name)).ok()
    }
}

#[derive(EntityEvent)]
struct ReadAsset {
    #[event_target]
    entity: Entity,
    root: PathBuf,
}

impl ReadAsset {
    fn read(&self, name: &str) -> Option<Vec<u8>> {
        std::fs::read(self.root.join(name)).ok()
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

fn use_assets(config: Res<AssetConfig>) {
    let _ = config.read("index.css");
    let driver = AssetDriver {
        root: config.root.clone(),
    };
    let _ = driver.read("index.css");
}

fn on_read(trigger: On<ReadAsset>) {
    let _ = trigger.event().read("index.css");
}

struct DemoPlugin;

impl Plugin for DemoPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(AssetConfig {
            root: PathBuf::new(),
        })
        .add_observer(on_read)
        .add_systems(Update, use_assets);
    }
}

fn main() {
    let mut app = App::new();
    app.add_plugins(DemoPlugin);
}
