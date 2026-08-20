//! The phone's ECS host, ticked by the event loop that already owns the app.
//!
//! The desktop runs Bevy on the main thread and hands the webview a corner of its window. Dioxus
//! is the other way round here — `LaunchBuilder::mobile()` owns the loop and never returns — but
//! that loop hands out a `FnMut` on every event, which is a place to put a world. So the world
//! lives on the same thread as the pages it answers, and neither a channel nor a pump stands
//! between them: a page's send is a borrow, and what the world produces is delivered by the call
//! that produced it.
//!
//! Two event loops was never an option. `UIApplicationMain` may be called once per process and
//! both tao and winit assert on it, so whichever ran second would panic.
//!
//! **This is reactive, not a frame loop, and that is load-bearing.** The turn boundary is tao's
//! `MainEventsCleared`, which on iOS is emitted from a CFRunLoop observer at
//! `kCFRunLoopBeforeWaiting` (`tao/src/platform_impl/ios/event_loop.rs:281`) — the instant the run
//! loop is about to go to sleep because it has nothing left to do. An idle phone never reaches it,
//! so an idle world costs nothing. Dioxus asks for `ControlFlow::Wait` and resets it on every
//! event, so the loop blocks rather than spins.
//!
//! What it means is that the world advances once per batch of real work, never on a timer. That is
//! the same bargain the desktop makes with `UpdateMode::Reactive`, and the reason neither is
//! allowed to be `Continuous`. Anything needing a turn without an event — a QUIC reply landing on
//! a tokio task — pokes the loop through Dioxus's own proxy rather than by polling for it.

use std::cell::RefCell;

use bevy_app::{App, PluginsState};
use bevy_window::AppLifecycle;

// Lifecycle reported by UIKit, held until the world's next turn.
//
// A thread local rather than a handle the observer keeps: both run on the main thread, and this
// way the observer needs to know nothing about the world's borrow state. Draining happens at a
// point `World::tick` chooses, which is what stops a notification arriving mid-turn from
// re-entering the schedule.
thread_local! {
    static REPORTED: RefCell<Vec<AppLifecycle>> = const { RefCell::new(Vec::new()) };
}

/// One Bevy world, advanced a turn at a time.
pub struct World {
    app: App,
    lifecycle: AppLifecycle,
    /// Set once the app has exited, so a loop that keeps delivering events stops running systems.
    finished: bool,
}

impl World {
    /// Build the world and run its plugins to completion, ready to tick.
    pub fn new(plugins: impl FnOnce(&mut App)) -> Self {
        let mut app = App::new();
        app.add_message::<AppLifecycle>();
        plugins(&mut app);
        while app.plugins_state() == PluginsState::Adding {
            bevy_tasks::tick_global_task_pools_on_main_thread();
        }
        app.finish();
        app.cleanup();
        Self {
            app,
            lifecycle: AppLifecycle::Idle,
            finished: false,
        }
    }

    /// Report what UIKit just said about the app. Called from the lifecycle observer.
    pub fn report(lifecycle: AppLifecycle) {
        REPORTED.with_borrow_mut(|reported| reported.push(lifecycle));
    }

    /// Advance the world one turn, unless the app is in the background.
    ///
    /// `WillSuspend` is owed exactly one more turn, which is Bevy's own contract and the only
    /// place a plugin has to save from — `bevy_winit` reads it the same way, refusing to update
    /// once `Suspended` (`state.rs:726`).
    pub fn tick(&mut self) {
        if self.finished {
            return;
        }
        self.drain_reported();
        if !self.is_active() {
            return;
        }
        self.app.update();
        if self.lifecycle == AppLifecycle::WillSuspend {
            self.lifecycle = AppLifecycle::Suspended;
        }
        if self.app.should_exit().is_some() {
            self.finished = true;
        }
    }

    /// Whether the world is owed a turn: running, or owed the one frame `WillSuspend` promises.
    fn is_active(&self) -> bool {
        matches!(
            self.lifecycle,
            AppLifecycle::Running | AppLifecycle::WillSuspend | AppLifecycle::WillResume
        )
    }

    fn drain_reported(&mut self) {
        let reported = REPORTED.with_borrow_mut(std::mem::take);
        for lifecycle in reported {
            self.lifecycle = lifecycle;
            self.app.world_mut().write_message(lifecycle);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_app::{AppExit, Update};
    use bevy_ecs::resource::Resource;
    use bevy_ecs::system::ResMut;

    #[derive(Resource, Default)]
    struct Turns(usize);

    impl World {
        /// A world that counts the turns it is given, with nothing else in it.
        fn counting() -> Self {
            REPORTED.with_borrow_mut(Vec::clear);
            Self::new(|app| {
                app.init_resource::<Turns>()
                    .add_systems(Update, |mut turns: ResMut<Turns>| turns.0 += 1);
            })
        }

        fn turns(&self) -> usize {
            self.app.world().resource::<Turns>().0
        }
    }

    #[test]
    fn an_idle_world_does_not_run_until_it_is_told_the_app_is_running() {
        let mut world = World::counting();
        world.tick();
        assert_eq!(world.turns(), 0, "a world nobody has resumed must not run");

        World::report(AppLifecycle::Running);
        world.tick();
        assert_eq!(world.turns(), 1);
    }

    #[test]
    fn suspending_owes_exactly_one_more_turn_and_then_stops() {
        let mut world = World::counting();
        World::report(AppLifecycle::Running);
        world.tick();

        World::report(AppLifecycle::WillSuspend);
        world.tick();
        let owed = world.turns();
        assert_eq!(owed, 2, "WillSuspend is owed the frame a plugin saves from");

        for _ in 0..5 {
            world.tick();
        }
        assert_eq!(owed, world.turns(), "a suspended world must not run");
    }

    #[test]
    fn a_resumed_world_runs_again() {
        let mut world = World::counting();
        World::report(AppLifecycle::Running);
        World::report(AppLifecycle::WillSuspend);
        world.tick();
        world.tick();
        let suspended = world.turns();

        World::report(AppLifecycle::Running);
        world.tick();
        assert_eq!(world.turns(), suspended + 1);
    }

    #[test]
    fn a_world_that_has_exited_stops_running_systems() {
        let mut world = World::counting();
        World::report(AppLifecycle::Running);
        world.tick();
        world.app.world_mut().write_message(AppExit::Success);
        world.tick();
        let exited = world.turns();

        world.tick();
        assert_eq!(exited, world.turns(), "an exited world must not run again");
    }
}
