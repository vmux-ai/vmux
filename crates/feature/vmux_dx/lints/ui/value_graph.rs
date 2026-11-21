#![deny(vmux_ecs_ownership)]

use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use bevy_ecs::entity::EntityHashMap;

struct Row(u32);

#[derive(Component)]
struct Ordered {
    first: Row,
    retained: Vec<Row>,
}

#[derive(Resource)]
struct Wrapped {
    rows: EntityHashMap<Row>,
}

#[derive(Component)]
struct Recursive {
    next: Option<Box<Recursive>>,
}

fn read(_: Query<&Ordered>, _: Res<Wrapped>, _: Query<&Recursive>) {}

struct OwnershipPlugin;

impl Plugin for OwnershipPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, read);
    }
}

fn main() {
    App::new().add_plugins(OwnershipPlugin);
}
