use std::task::Waker;

use bevy_app::{App, First, Last, Plugin};
use bevy_ecs::lifecycle::HookContext;
use bevy_ecs::prelude::*;
use bevy_ecs::world::DeferredWorld;

pub use clock::Clock;
pub use deadline::{
    BackoffPolicy, Deadline, DeadlinePlugin, DeadlineReached, DeadlineScheduler, DeadlineSet,
    Debounce, RetryBackoff,
};

mod clock;
mod deadline;

pub struct TimePlugin;

impl Plugin for TimePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Clock>()
            .init_resource::<DeadlineScheduler>()
            .configure_sets(First, (DeadlineSet::Wake, DeadlineSet::Fire).chain())
            .configure_sets(Last, DeadlineSet::Sync)
            .add_systems(First, sync_wake.in_set(DeadlineSet::Wake));
    }
}

#[derive(Resource, Clone)]
#[component(on_insert = bind_wake, on_remove = unbind_wake)]
pub struct TimeWake(pub Waker);

impl TimeWake {
    pub fn wake(&self) {
        self.0.wake_by_ref();
    }
}

fn bind_wake(world: DeferredWorld, context: HookContext) {
    let Some(wake) = world.get::<TimeWake>(context.entity) else {
        return;
    };
    let waker = wake.0.clone();
    let is_fake = world.get_resource::<Clock>().is_some_and(Clock::is_fake);
    if let Some(scheduler) = world.get_resource::<DeadlineScheduler>() {
        scheduler.set_waker(Some(waker), !is_fake);
    }
}

fn unbind_wake(world: DeferredWorld, _context: HookContext) {
    let is_fake = world.get_resource::<Clock>().is_some_and(Clock::is_fake);
    if let Some(scheduler) = world.get_resource::<DeadlineScheduler>() {
        scheduler.set_waker(None, !is_fake);
    }
}

fn sync_wake(clock: Res<Clock>, scheduler: Res<DeadlineScheduler>, wake: Option<Res<TimeWake>>) {
    if let Some(wake) = wake {
        if wake.is_changed() || scheduler.is_added() || !scheduler.has_waker() {
            scheduler.set_waker(Some(wake.0.clone()), !clock.is_fake());
        }
    } else if scheduler.has_waker() {
        scheduler.set_waker(None, !clock.is_fake());
    }
}
