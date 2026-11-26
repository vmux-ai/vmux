use bevy_app::App;
use vmux_app::ecs::{LeaseGranted, LeasePlugin, LeaseRequest};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct FileKey(&'static str);

fn main() {
    let mut app = App::new();
    app.add_plugins(LeasePlugin::<FileKey>::default());

    let writer = app
        .world_mut()
        .spawn(LeaseRequest::exclusive(FileKey("notes.md")))
        .id();
    let reader_a = app
        .world_mut()
        .spawn(LeaseRequest::shared(FileKey("notes.md")))
        .id();
    let reader_b = app
        .world_mut()
        .spawn(LeaseRequest::shared(FileKey("notes.md")))
        .id();

    app.update();
    println!(
        "step 1: writer={:?} reader_a={:?} reader_b={:?}",
        app.world()
            .get::<LeaseGranted<FileKey>>(writer)
            .map(LeaseGranted::mode),
        app.world()
            .get::<LeaseGranted<FileKey>>(reader_a)
            .map(LeaseGranted::mode),
        app.world()
            .get::<LeaseGranted<FileKey>>(reader_b)
            .map(LeaseGranted::mode),
    );

    app.world_mut().despawn(writer);
    println!(
        "step 2 (after writer despawn): reader_a={:?} reader_b={:?}",
        app.world()
            .get::<LeaseGranted<FileKey>>(reader_a)
            .map(LeaseGranted::mode),
        app.world()
            .get::<LeaseGranted<FileKey>>(reader_b)
            .map(LeaseGranted::mode),
    );
}
