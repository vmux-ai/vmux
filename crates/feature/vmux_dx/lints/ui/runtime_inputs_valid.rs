use std::marker::PhantomData;

use bevy_app::{App, Update};
use bevy_ecs::prelude::*;
use bevy_ecs::system::SystemParam;

#[derive(Component)]
#[component(on_insert = renew)]
#[relationship(relationship_target = Views)]
struct Binding(Entity);

#[derive(Component)]
#[relationship_target(relationship = Binding)]
struct Views(Vec<Entity>);

fn renew(_: bevy_ecs::world::DeferredWorld, _: bevy_ecs::lifecycle::HookContext) {}

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

struct Wrapper<T>(PhantomData<T>);

unsafe impl<T: SystemParam> SystemParam for Wrapper<T> {
    type State = ();
    type Item<'w, 's> = Self;

    fn init_state(_: &mut World) -> Self::State {}

    fn init_access(
        _: &Self::State,
        _: &mut bevy_ecs::system::SystemMeta,
        _: &mut bevy_ecs::query::FilteredAccessSet,
        _: &mut World,
    ) {
    }

    unsafe fn get_param<'w, 's>(
        _: &'s mut Self::State,
        _: &bevy_ecs::system::SystemMeta,
        _: bevy_ecs::world::unsafe_world_cell::UnsafeWorldCell<'w>,
        _: bevy_ecs::change_detection::Tick,
    ) -> Result<Self::Item<'w, 's>, bevy_ecs::system::SystemParamValidationError> {
        Ok(Self(PhantomData))
    }
}

impl<T> Wrapper<T> {
    fn inspect(&self, _: Commands)
    where
        T: SystemParam,
    {
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

fn walk(input: &Inputs) {
    if input.iter().next().is_some() {
        let recurse = || walk(input);
        recurse();
    }
}

fn traverse(input: Inputs) {
    walk(&input);
}

fn tick(_: Commands) {}

fn poll(_: Commands) {}

fn cached(_: Commands) {}

#[derive(Component, Clone, Copy)]
struct Writer {
    run: for<'w, 's> fn(&mut Commands<'w, 's>),
}

impl Writer {
    fn new() -> Self {
        Self { run: field_helper }
    }
}

fn field_helper(_: &mut Commands) {}

#[derive(Component, Clone, Copy)]
struct TupleWriter(for<'w, 's> fn(&mut Commands<'w, 's>));

impl TupleWriter {
    fn new() -> Self {
        Self(tuple_helper)
    }
}

fn tuple_helper(_: &mut Commands) {}

fn forward(writers: Query<(&Writer, &TupleWriter)>, mut commands: Commands) {
    for (writer, tuple) in &writers {
        (writer.run)(&mut commands);
        (tuple.0)(&mut commands);
    }
}

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
        _: PhantomData<State>,
        _: Option<PhantomData<State>>,
        _: Vec<PhantomData<State>>,
    ) {
    }
}

fn ordinary(
    _: Option<u32>,
    _: Vec<u32>,
    _: Option<PhantomData<State>>,
    _: Vec<PhantomData<State>>,
) {
}

trait Framework {
    fn inspect(&self, _: Inputs);
}

fn main() {
    let mut world = World::new();
    world.register_system(tick);
    World::register_system(&mut world, poll);
    world.register_system_cached(cached);
    App::new().add_systems(Update, (apply, inspect, forward, traverse));
    Engine.unrelated(unrelated::Query);
}
