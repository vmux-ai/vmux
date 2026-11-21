#![deny(vmux_ecs_ownership)]

use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use bevy_ecs::system::SystemParam;
use bevy_ecs::query::QueryData;
use bevy_ecs::prelude::Component as StateComponent;

struct Coordinate(f32);
struct Value<T>(T);

#[derive(StateComponent)]
struct State {
    coordinate: Coordinate,
    value: Value<u32>,
    samples: Vec<u32>,
}

type Alias = State;
type Inputs<'w, 's> = Query<'w, 's, &'static Alias>;

#[derive(Resource)]
struct Clock(u64);

#[derive(Message)]
struct Request(u32);

#[derive(QueryData)]
struct View {
    state: &'static State,
}

#[derive(SystemParam)]
struct Parameters<'w, 's> {
    input: Inputs<'w, 's>,
    clock: Res<'w, Clock>,
    requests: MessageReader<'w, 's, Request>,
}

fn read(_: Parameters, _: Query<View>) {}

struct OwnershipPlugin;

impl Plugin for OwnershipPlugin {
    fn build(&self, app: &mut App) {
        let system = read;
        app.add_systems(Update, system);
    }
}

fn main() {
    App::new().add_plugins(OwnershipPlugin);
}
