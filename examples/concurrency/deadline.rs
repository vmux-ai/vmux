use std::time::Duration;

use bevy_app::App;
use bevy_ecs::prelude::*;
use vmux_app::VmuxPlugin;
use vmux_app::ecs::ExpiresAt;
use vmux_app::ecs::time::{Clock, Deadline, DeadlinePlugin, DeadlineReached, Debounce};

struct Refresh;
struct Autosave;

fn on_refresh(reached: On<DeadlineReached<Refresh>>) {
    println!("refresh deadline reached for {:?}", reached.entity);
}

fn on_autosave(reached: On<DeadlineReached<Autosave>>) {
    println!("autosave debounce reached for {:?}", reached.entity);
}

fn main() {
    let clock = Clock::fake(1_700_000_000_000);
    let mut app = App::new();
    app.insert_resource(clock.clone())
        .add_plugins((
            VmuxPlugin::builder().jobs(true).build(),
            DeadlinePlugin::<Refresh>::default(),
            DeadlinePlugin::<Autosave>::default(),
        ))
        .add_observer(on_refresh)
        .add_observer(on_autosave);

    let item = app
        .world_mut()
        .spawn((
            Deadline::<Refresh>::after(&clock, Duration::from_secs(2)),
            Debounce::<Autosave>::new(Duration::from_secs(3)),
            ExpiresAt::after(&clock, Duration::from_secs(10)),
        ))
        .id();
    app.world_mut()
        .get_mut::<Debounce<Autosave>>(item)
        .unwrap()
        .reset(&clock);
    app.update();

    for step in [2, 1, 7] {
        clock.advance(Duration::from_secs(step));
        app.update();
    }
    println!(
        "expired and despawned={}",
        app.world().get_entity(item).is_err()
    );
}
