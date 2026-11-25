use bevy_app::App;
use bevy_ecs::prelude::*;
use vmux_app::ecs::{SelectRequest, Selected, SelectionPlugin};

#[derive(Clone, Copy)]
struct FileList;

fn main() {
    let mut app = App::new();
    app.add_plugins(SelectionPlugin::<FileList>::default());

    let list = app.world_mut().spawn(Name::new("Sidebar")).id();
    let alpha = app
        .world_mut()
        .spawn((Name::new("alpha.rs"), ChildOf(list)))
        .id();
    let beta = app
        .world_mut()
        .spawn((Name::new("beta.rs"), ChildOf(list)))
        .id();

    app.world_mut()
        .write_message(SelectRequest::<FileList>::select(alpha));
    app.update();
    println!(
        "selected after first request: alpha={} beta={}",
        app.world().get::<Selected<FileList>>(alpha).is_some(),
        app.world().get::<Selected<FileList>>(beta).is_some(),
    );

    app.world_mut()
        .write_message(SelectRequest::<FileList>::select(beta));
    app.update();
    println!(
        "selected after second request: alpha={} beta={}",
        app.world().get::<Selected<FileList>>(alpha).is_some(),
        app.world().get::<Selected<FileList>>(beta).is_some(),
    );
}
