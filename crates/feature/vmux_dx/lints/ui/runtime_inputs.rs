#![deny(vmux_ecs_runtime)]

use bevy_app::{App, Update};
use bevy_ecs::prelude::*;
use bevy_ecs::system::SystemParam;

#[derive(Component)]
struct State;

type Inputs<'w, 's> = Query<'w, 's, &'static State>;

#[derive(SystemParam)]
struct Access<'w, 's> {
    input: Inputs<'w, 's>,
}

struct Engine;

impl Engine {
    fn inspect(&self, _: Inputs) {}

    fn inspect_access(&self, _: Access) {}
}

fn isolated(_: Inputs) {}

fn shared(_: Inputs) {}

pub fn exposed(_: Inputs) {}

fn detached(_: Inputs) {}

fn unregistered(input: Inputs) {
    detached(input);
}

fn generic<T: SystemParam>(_: T) {}

fn first(input: Inputs) {
    shared(input);
}

fn second(input: Inputs) {
    shared(input);
}

fn invoke(input: Inputs) {
    exposed(input);
}

fn main() {
    App::new().add_systems(Update, (first, second, invoke));
}
