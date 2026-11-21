#![deny(vmux_ecs_ownership)]

use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::{Component, Query};

#[derive(Component)]
struct Unused(u32);

#[derive(Component)]
struct Unregistered(u32);

fn forgotten(_: Query<&Unregistered>) {}

mod fake {
    use std::default::Default as Component;

    #[derive(Component)]
    struct Forged(u32);
}

#[derive(Component)]
struct State(u32);

fn read(_: Query<&State>) {}

struct OwnershipPlugin;

impl Plugin for OwnershipPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, read);
    }
}

fn main() {
    App::new().add_plugins(OwnershipPlugin);
}
