#![deny(vmux_ecs_ownership)]

use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct PrepareSet;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum StageSet {
    Accept,
    Apply,
}

#[derive(SystemSet, Clone, Debug, PartialEq, Eq, Hash)]
enum PayloadSet {
    Bad(u32),
}

#[derive(Component)]
struct Flag(bool);

#[derive(Bundle)]
struct SpawnedBundle {
    flag: Flag,
}

#[derive(Bundle)]
struct UnspawnedBundle {
    flag: Flag,
}

fn spawn_all(mut commands: Commands) {
    commands.spawn(SpawnedBundle { flag: Flag(true) });
}

struct BoundaryPlugin;

impl Plugin for BoundaryPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, spawn_all.in_set(PrepareSet).in_set(StageSet::Apply));
    }
}

fn main() {
    App::new().add_plugins(BoundaryPlugin);
}
