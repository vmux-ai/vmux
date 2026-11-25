use std::time::Duration;

use bevy_app::App;
use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use moonshine_save::prelude::Save;
use vmux_app::VmuxPlugin;
use vmux_app::ecs::store::{
    AutoSave, AutoSavePlugin, AutoSavePolicy, SchemaPlugin, State, StorePath, StoreWriteRequest,
    StoreWriteResult,
};
use vmux_app::ecs::time::{BackoffPolicy, Clock};

#[derive(Component, Reflect, Default)]
#[reflect(Component)]
#[require(Save)]
struct Note;

#[derive(Component, Reflect, Default)]
#[reflect(Component)]
struct Title(String);

#[vmux_ecs::store(track; Note, Title)]
struct NoteStore;

fn handle_write(request: On<StoreWriteRequest<NoteStore>>, mut commands: Commands) {
    println!(
        "write requested: path={} rev={} flush={}",
        request.path.display(),
        request.revision,
        request.flush,
    );
    commands.trigger(StoreWriteResult::<NoteStore>::ok(
        request.entity,
        request.revision,
    ));
}

fn main() {
    let clock = Clock::fake(1_700_000_000_000);
    let mut app = App::new();
    app.insert_resource(clock.clone())
        .add_plugins((
            VmuxPlugin::builder().build(),
            SchemaPlugin::<NoteStore>::default(),
            AutoSavePlugin::<NoteStore>::default(),
        ))
        .add_observer(handle_write);

    let store = app
        .world_mut()
        .spawn((
            AutoSave::<NoteStore>::default(),
            StorePath::new("/tmp/vmux-example-notes.ron"),
            AutoSavePolicy::new(
                Duration::from_millis(500),
                Some(Duration::from_secs(2)),
                BackoffPolicy::exponential(Duration::from_secs(1), Duration::from_secs(8)),
            ),
        ))
        .id();

    let note = app
        .world_mut()
        .spawn((Note, Title("Initial draft".into())))
        .id();
    app.update();
    println!(
        "after spawn: dirty={}",
        app.world().get::<State>(store).unwrap().is_dirty()
    );

    clock.advance(Duration::from_millis(250));
    app.world_mut().get_mut::<Title>(note).unwrap().0 = "Final draft".into();
    app.update();

    clock.advance(Duration::from_millis(500));
    app.update();
    println!(
        "after debounce + write ok: dirty={}",
        app.world().get::<State>(store).unwrap().is_dirty()
    );
}
