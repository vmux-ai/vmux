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

struct Wrapper<T>(std::marker::PhantomData<T>);

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
        Ok(Self(std::marker::PhantomData))
    }
}

impl<T> Wrapper<T> {
    fn unguarded(&self, _: Commands) {}
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

fn captured(_: Inputs) {}

fn disconnected(input: Inputs) {
    let invoke = || captured(input);
    invoke();
}

#[derive(Component)]
struct UnusedWriter {
    run: for<'w, 's> fn(&mut Commands<'w, 's>),
}

impl UnusedWriter {
    fn new() -> Self {
        Self { run: unread }
    }
}

fn unread(_: &mut Commands) {}

struct PlainWriter {
    run: for<'w, 's> fn(&mut Commands<'w, 's>),
}

impl PlainWriter {
    fn new() -> Self {
        Self { run: plain_helper }
    }
}

fn plain_helper(_: &mut Commands) {}

fn read_plain(writer: NonSend<PlainWriter>, mut commands: Commands) {
    (writer.run)(&mut commands);
}

#[derive(Component)]
struct MixedWriter {
    run: for<'w, 's> fn(&mut Commands<'w, 's>),
}

impl MixedWriter {
    fn new() -> Self {
        Self {
            run: mixed_callback,
        }
    }
}

fn mixed_callback(_: &mut Commands) {}

fn invoke_mixed(writers: Query<&MixedWriter>, mut commands: Commands) {
    for writer in &writers {
        (writer.run)(&mut commands);
    }
}

fn call_mixed(mut commands: Commands) {
    mixed_callback(&mut commands);
}

#[derive(Component)]
struct CopiedWriter {
    run: for<'w, 's> fn(&mut Commands<'w, 's>),
}

impl CopiedWriter {
    fn new() -> Self {
        Self {
            run: copied_callback,
        }
    }
}

fn copied_callback(_: &mut Commands) {}

fn invoke_copied(writers: Query<&CopiedWriter>, mut commands: Commands) {
    for writer in &writers {
        (writer.run)(&mut commands);
    }
}

fn copy_field(writers: Query<&CopiedWriter>, mut commands: Commands) {
    for writer in &writers {
        let run = writer.run;
        run(&mut commands);
    }
}

fn optional_commands(_: Option<Commands>) {}

fn vector_commands(_: Vec<Commands>) {}

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
    App::new().add_systems(
        Update,
        (
            first,
            second,
            invoke,
            read_plain,
            invoke_mixed,
            call_mixed,
            invoke_copied,
            copy_field,
        ),
    );
}
