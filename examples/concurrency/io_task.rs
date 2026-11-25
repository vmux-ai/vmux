use std::time::{Duration, Instant};

use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use vmux_app::ecs::{IoTask, IoTaskPlugin, TaskError};

#[derive(Component, Debug)]
struct FetchedCount(usize);

#[derive(Resource, Default)]
struct ObservedResults {
    ok: Option<usize>,
    err: Option<String>,
}

struct ExampleIoTaskPlugin;

impl Plugin for ExampleIoTaskPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(IoTaskPlugin::<(FetchedCount,)>::default())
            .init_resource::<ObservedResults>()
            .add_systems(Update, collect_results);
    }
}

fn collect_results(
    completed: Query<&FetchedCount, Added<FetchedCount>>,
    failed: Query<&TaskError<FetchedCount>, Added<TaskError<FetchedCount>>>,
    mut observed: ResMut<ObservedResults>,
) {
    for count in &completed {
        observed.ok = Some(count.0);
    }
    for error in &failed {
        observed.err = Some(error.to_string());
    }
}

fn main() {
    let mut app = App::new();
    app.add_plugins(ExampleIoTaskPlugin);

    app.world_mut()
        .spawn(IoTask::spawn(async { Ok::<_, String>(FetchedCount(42)) }));
    app.world_mut().spawn(IoTask::spawn(async {
        Err::<FetchedCount, String>("upstream timeout".into())
    }));

    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        app.update();
        let observed = app.world().resource::<ObservedResults>();
        if let (Some(ok), Some(err)) = (observed.ok, observed.err.as_deref()) {
            println!("ok={ok} err={err}");
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
}
