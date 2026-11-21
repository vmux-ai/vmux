#![deny(vmux_ecs_ownership)]

use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use std::marker::PhantomData;

#[derive(Component)]
struct FilterOnly(u32);

struct Phantom(u32);

#[derive(Component)]
struct State {
    marker: PhantomData<Phantom>,
}

fn read(_: Query<&State, With<FilterOnly>>, _: Local<u32>) {}

struct OwnershipPlugin;

impl Plugin for OwnershipPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, read);
    }
}

fn main() {
    App::new().add_plugins(OwnershipPlugin);
}
