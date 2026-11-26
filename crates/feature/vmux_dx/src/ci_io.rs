use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use std::marker::PhantomData;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use vmux_ecs::OperationState;
use vmux_job::blocking::Lane;
use vmux_job::{Job, JobPlugin, JobResult, JobSet};
pub(crate) mod cache;
pub(crate) mod fs;
pub(crate) mod process;
pub(crate) trait IoJob: Job<Output = Result<Self::Data, String>> {
    type Data: Component + Clone;
}
pub(crate) struct IoPlugin<J: IoJob>(PhantomData<fn() -> J>);
impl<J: IoJob> Default for IoPlugin<J> {
    fn default() -> Self {
        Self(PhantomData)
    }
}
impl<J: IoJob> Plugin for IoPlugin<J> {
    fn build(&self, app: &mut App) {
        app.add_plugins(JobPlugin::<Tracked<J>>::default())
            .add_systems(Update, adopt::<J>.in_set(JobSet::Apply));
    }
}
pub(crate) struct Tracked<J: IoJob> {
    job: J,
    guard: Arc<Guard>,
}
impl<J: IoJob> Clone for Tracked<J> {
    fn clone(&self) -> Self {
        Self {
            job: self.job.clone(),
            guard: self.guard.clone(),
        }
    }
}
impl<J: IoJob> Tracked<J> {
    pub(crate) fn new(job: J) -> (Self, Flight) {
        let pending = Arc::new(AtomicBool::new(true));
        let flight = Flight(pending.clone());
        let guard = Arc::new(Guard(pending));
        (Self { job, guard }, flight)
    }
}
impl<J: IoJob> Job for Tracked<J> {
    type Output = Report<J::Data>;
    fn lane(&self) -> Lane {
        self.job.lane()
    }
    fn run(self) -> Self::Output {
        let result = if vmux_job::blocking::BlockingCancellation::canceled() {
            Err("CI operation canceled".into())
        } else {
            self.job.run()
        };
        Report {
            result,
            _guard: self.guard,
        }
    }
}
pub(crate) struct Report<T> {
    result: Result<T, String>,
    _guard: Arc<Guard>,
}
struct Guard(Arc<AtomicBool>);
impl Drop for Guard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}
#[derive(Component)]
pub(crate) struct Flight(Arc<AtomicBool>);
impl Flight {
    pub(crate) fn pending(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}
#[derive(Component)]
pub(crate) struct Owner(pub Entity);
#[derive(Component, Clone)]
pub(crate) struct Failed(pub String);
fn adopt<J: IoJob>(results: AdoptResults<'_, '_, J>, mut commands: Commands) {
    for (entity, result, owner, parent, state) in &results {
        if owner.0 != parent.parent()
            || !matches!(state, OperationState::Pending | OperationState::Running)
        {
            continue;
        }
        match &result.0 {
            Ok(report) => match &report.result {
                Ok(data) => {
                    commands
                        .entity(entity)
                        .insert((data.clone(), OperationState::Succeeded));
                }
                Err(error) => {
                    commands
                        .entity(entity)
                        .insert((Failed(error.clone()), OperationState::Failed));
                }
            },
            Err(error) => {
                commands
                    .entity(entity)
                    .insert((Failed(error.clone()), OperationState::Failed));
            }
        }
        commands.entity(entity).remove::<JobResult<Tracked<J>>>();
    }
}
type AdoptResults<'w, 's, J> = Query<
    'w,
    's,
    (
        Entity,
        &'static JobResult<Tracked<J>>,
        &'static Owner,
        &'static ChildOf,
        &'static OperationState,
    ),
>;
