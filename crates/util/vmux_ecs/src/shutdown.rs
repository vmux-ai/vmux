use std::num::NonZero;
use std::time::Duration;

use bevy_app::{App, AppExit, Last, Plugin};
use bevy_ecs::prelude::*;
use bevy_ecs::schedule::ScheduleLabel;

pub struct ShutdownPlugin;

impl Plugin for ShutdownPlugin {
    fn build(&self, app: &mut App) {
        if app.world().contains_resource::<ShutdownState>() {
            return;
        }
        app.init_schedule(Shutdown)
            .init_schedule(ShutdownComplete)
            .init_resource::<ShutdownState>()
            .init_resource::<ShutdownTimeout>()
            .add_message::<AppExit>()
            .add_message::<ShutdownRequest>()
            .configure_sets(Last, ShutdownSet::Request)
            .configure_sets(
                Shutdown,
                (
                    ShutdownSet::Request,
                    ShutdownSet::Drain,
                    ShutdownSet::Complete,
                )
                    .chain(),
            )
            .configure_sets(ShutdownComplete, ShutdownSet::Complete)
            .add_systems(Last, accept_requests.in_set(ShutdownSet::Request));
    }

    fn is_unique(&self) -> bool {
        false
    }
}

#[derive(ScheduleLabel, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Shutdown;

#[derive(ScheduleLabel, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ShutdownComplete;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ShutdownSet {
    Request,
    Drain,
    Complete,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ShutdownPhase {
    #[default]
    Running,
    Draining,
    Complete,
}

#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub struct ShutdownState {
    phase: ShutdownPhase,
    exit: Option<AppExit>,
}

impl ShutdownState {
    pub fn phase(&self) -> ShutdownPhase {
        self.phase
    }

    pub fn is_draining(&self) -> bool {
        self.phase == ShutdownPhase::Draining
    }

    pub fn is_complete(&self) -> bool {
        self.phase == ShutdownPhase::Complete
    }

    pub fn exit(&self) -> Option<AppExit> {
        self.exit.clone()
    }

    pub fn begin(&mut self, exit: AppExit) -> bool {
        if self.phase != ShutdownPhase::Running {
            return false;
        }
        self.phase = ShutdownPhase::Draining;
        self.exit = Some(exit);
        true
    }

    pub fn finish(&mut self) {
        self.phase = ShutdownPhase::Complete;
    }
}

#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShutdownTimeout(pub Duration);

impl Default for ShutdownTimeout {
    fn default() -> Self {
        Self(Self::DEFAULT)
    }
}

impl ShutdownTimeout {
    pub const DEFAULT: Duration = Duration::from_secs(3);

    pub const fn new(duration: Duration) -> Self {
        Self(duration)
    }

    pub const fn duration(self) -> Duration {
        self.0
    }
}

#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub struct ShutdownRequest {
    pub exit: AppExit,
}

impl Default for ShutdownRequest {
    fn default() -> Self {
        Self::success()
    }
}

impl ShutdownRequest {
    pub const fn new(exit: AppExit) -> Self {
        Self { exit }
    }

    pub const fn success() -> Self {
        Self {
            exit: AppExit::Success,
        }
    }

    pub const fn error(code: NonZero<u8>) -> Self {
        Self {
            exit: AppExit::Error(code),
        }
    }
}

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ShutdownBarrier;

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ShutdownPending;

fn accept_requests(
    mut requests: MessageReader<ShutdownRequest>,
    mut state: ResMut<ShutdownState>,
    mut exits: MessageWriter<AppExit>,
) {
    for request in requests.read() {
        if state.begin(request.exit.clone()) {
            exits.write(request.exit.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shutdown_plugin_is_idempotent_and_preserves_custom_timeout() {
        let mut app = App::new();
        let timeout = Duration::from_millis(250);
        app.insert_resource(ShutdownTimeout::new(timeout))
            .add_plugins((ShutdownPlugin, ShutdownPlugin));
        assert_eq!(
            app.world().resource::<ShutdownTimeout>().duration(),
            timeout
        );
        assert_eq!(
            app.world().resource::<ShutdownState>().phase(),
            ShutdownPhase::Running
        );
        assert!(app.world_mut().try_run_schedule(Shutdown).is_ok());
        assert!(app.world_mut().try_run_schedule(ShutdownComplete).is_ok());
    }

    #[test]
    fn shutdown_request_transitions_to_draining_and_emits_app_exit_once() {
        let mut app = App::new();
        app.add_plugins(ShutdownPlugin);
        let code = NonZero::new(7).unwrap();
        app.world_mut().write_message(ShutdownRequest::error(code));
        app.world_mut().write_message(ShutdownRequest::success());
        app.update();

        let state = app.world().resource::<ShutdownState>();
        assert!(state.is_draining());
        assert!(!state.is_complete());
        assert_eq!(state.exit(), Some(AppExit::Error(code)));
        assert_eq!(app.should_exit(), Some(AppExit::Error(code)));

        app.world_mut().resource_mut::<ShutdownState>().finish();
        let state = app.world().resource::<ShutdownState>();
        assert!(state.is_complete());
        assert!(!state.is_draining());
        assert_eq!(state.exit(), Some(AppExit::Error(code)));
    }

    #[test]
    fn shutdown_barrier_tracks_active_drain_participants() {
        let mut app = App::new();
        app.add_plugins(ShutdownPlugin);
        let barrier = app.world_mut().spawn(ShutdownBarrier).id();
        let world = app.world_mut();
        assert_eq!(
            world
                .query_filtered::<Entity, With<ShutdownBarrier>>()
                .iter(world)
                .count(),
            1
        );
        app.world_mut()
            .entity_mut(barrier)
            .remove::<ShutdownBarrier>();
        let world = app.world_mut();
        assert_eq!(
            world
                .query_filtered::<Entity, With<ShutdownBarrier>>()
                .iter(world)
                .count(),
            0
        );
    }
}
