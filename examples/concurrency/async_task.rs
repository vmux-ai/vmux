use std::time::{Duration, Instant};

use bevy_app::App;
use bevy_tasks::{IoTaskPool, TaskPoolBuilder};
use vmux_app::ecs::{AsyncTask, TaskPlugin, TaskResult, TaskToken};

fn main() {
    IoTaskPool::get_or_init(|| TaskPoolBuilder::new().num_threads(2).build());

    let mut app = App::new();
    app.add_plugins(TaskPlugin::<String>::default());

    let active = app
        .world_mut()
        .spawn(AsyncTask::new(
            IoTaskPool::get().spawn(async { "computed payload".to_string() }),
        ))
        .id();

    let canceled = app
        .world_mut()
        .spawn(AsyncTask::new(
            IoTaskPool::get().spawn(async { "stale payload".to_string() }),
        ))
        .id();
    app.world().get::<TaskToken>(canceled).unwrap().cancel();

    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        app.update();
        if let Some(mut result) = app.world_mut().get_mut::<TaskResult<String>>(active) {
            let value = result.take().unwrap().unwrap();
            println!("active result: {value}");
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    println!(
        "canceled discarded={}",
        app.world().get::<TaskResult<String>>(canceled).is_none()
    );
}
