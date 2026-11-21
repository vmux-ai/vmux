#![deny(vmux_ecs_ownership)]

use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use std::sync::Arc;

struct Shared(u32);

#[derive(Component)]
struct First {
    shared: Shared,
}

#[derive(Resource)]
struct Second {
    shared: Shared,
}

#[derive(Resource)]
struct Callback {
    call: Arc<fn()>,
}

fn read(_: Query<&First>, _: Res<Second>, _: Res<Callback>) {}

struct OwnershipPlugin;

impl Plugin for OwnershipPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, read);
    }
}

fn main() {
    App::new().add_plugins(OwnershipPlugin);
}
