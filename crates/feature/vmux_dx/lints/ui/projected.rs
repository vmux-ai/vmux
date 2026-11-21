#![deny(vmux_ecs_ownership)]

use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;

struct Value(u32);

trait Payload: Send + Sync + 'static {
    type Value: Send + Sync + 'static;
}

impl Payload for u32 {
    type Value = Value;
}

#[derive(Component)]
struct State<T: Payload> {
    value: T::Value,
}

fn read(_: Query<&State<u32>>) {}

struct OwnershipPlugin;

impl Plugin for OwnershipPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, read);
    }
}

fn main() {
    App::new().add_plugins(OwnershipPlugin);
}
