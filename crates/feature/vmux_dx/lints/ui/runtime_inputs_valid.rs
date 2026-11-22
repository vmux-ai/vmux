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

impl Access<'_, '_> {
    fn inspect(&self, input: Inputs) {
        parameter_helper(input);
    }
}

fn parameter_helper(_: Inputs) {}

fn inner(_: Inputs) {}

fn outer(input: Inputs) {
    inner(input);
}

fn apply(input: Inputs) {
    outer(input);
}

fn inspect(access: Access, input: Inputs) {
    access.inspect(input);
    generic_helper(access);
}

fn generic_helper<T: SystemParam>(_: T) {}

mod unrelated {
    pub struct Query;
    pub trait SystemParam {}
}

struct Engine;

impl unrelated::SystemParam for Engine {}

impl Engine {
    fn unrelated(&self, _: unrelated::Query) {}

    fn containers(
        &self,
        _: Option<u32>,
        _: Vec<u32>,
        _: Result<u32, String>,
        _: std::marker::PhantomData<State>,
    ) {}
}

fn ordinary(_: Option<u32>, _: Vec<u32>) {}

trait Framework {
    fn inspect(&self, _: Inputs);
}

fn main() {
    App::new().add_systems(Update, (apply, inspect));
    Engine.unrelated(unrelated::Query);
}
