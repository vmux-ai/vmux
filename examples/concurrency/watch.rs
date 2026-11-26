use std::time::{Duration, Instant};

use bevy_app::{App, Update};
use bevy_ecs::prelude::*;
use vmux_app::VmuxPlugin;
use vmux_app::ecs::watch::{FileWatch, FileWatchEvent, FileWatchPlugin, FileWatchSet, Watching};

#[derive(Resource, Default)]
struct SawChange(bool);

fn print_events(mut reader: MessageReader<FileWatchEvent>, mut saw: ResMut<SawChange>) {
    for event in reader.read() {
        for path in &event.paths {
            println!("watched change: {}", path.display());
            saw.0 = true;
        }
    }
}

fn main() {
    let directory = tempfile::tempdir().unwrap();
    let mut app = App::new();
    app.add_plugins((VmuxPlugin::builder().jobs(true).build(), FileWatchPlugin))
        .init_resource::<SawChange>()
        .add_systems(Update, print_events.after(FileWatchSet::Reconcile));

    let watch = app
        .world_mut()
        .spawn(FileWatch::non_recursive(directory.path()))
        .id();
    app.update();
    println!(
        "watching active={}",
        app.world().get::<Watching>(watch).is_some()
    );

    let file = directory.path().join("hello.txt");
    std::fs::write(&file, "hello vmux\n").unwrap();

    let deadline = Instant::now() + Duration::from_secs(5);
    while !app.world().resource::<SawChange>().0 {
        app.update();
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
}
