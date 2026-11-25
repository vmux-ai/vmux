use std::time::{Duration, Instant};

use bevy_app::App;
use bevy_ecs::prelude::*;
use vmux_app::ecs::{IoTask, IoTaskPlugin, TaskError};

#[derive(Component, Debug)]
struct FetchedCount(usize);

fn main() {
    let mut app = App::new();
    app.add_plugins(IoTaskPlugin::<(FetchedCount,)>::default());

    let ok_entity = app
        .world_mut()
        .spawn(IoTask::spawn(async { Ok::<_, String>(FetchedCount(42)) }))
        .id();
    let err_entity = app
        .world_mut()
        .spawn(IoTask::spawn(async {
            Err::<FetchedCount, String>("upstream timeout".into())
        }))
        .id();

    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        app.update();
        let ok = app.world().get::<FetchedCount>(ok_entity);
        let err = app.world().get::<TaskError<FetchedCount>>(err_entity);
        if let (Some(ok), Some(err)) = (ok, err) {
            println!("ok={} err={err}", ok.0);
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
}
