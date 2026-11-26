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

fn handle_write(
    request: On<StoreWriteRequest<NoteStore>>,
    notes: Query<&Title, With<Note>>,
    mut commands: Commands,
) {
    let payload = notes
        .iter()
        .map(|title| title.0.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    println!(
        "write requested: path={} rev={} flush={}",
        request.path.display(),
        request.revision,
        request.flush,
    );
    let result = match std::fs::write(&request.path, payload) {
        Ok(()) => StoreWriteResult::<NoteStore>::ok(request.entity, request.revision),
        Err(error) => {
            StoreWriteResult::<NoteStore>::err(request.entity, request.revision, error.to_string())
        }
    };
    commands.trigger(result);
}

fn main() {
    let directory = tempfile::tempdir().unwrap();
    let store_path = directory.path().join("notes.ron");
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
            StorePath::new(store_path.clone()),
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
    let saved = std::fs::read_to_string(&store_path).unwrap();
    println!(
        "after debounce + write ok: dirty={} saved={saved:?}",
        app.world().get::<State>(store).unwrap().is_dirty()
    );
}
