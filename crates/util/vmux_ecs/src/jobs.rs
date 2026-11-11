use std::any::Any;
use std::marker::PhantomData;
use std::task::Waker;

use bevy_app::{App, Plugin, PostUpdate, Update};
use bevy_ecs::prelude::*;

use crate::blocking::{BlockingRuntime, BlockingTask, Lane, SpawnError};
use crate::{Operation, OperationPlugin, OperationState};

pub struct JobsPlugin;

impl Plugin for JobsPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<OperationPlugin>() {
            app.add_plugins(OperationPlugin);
        }
        app.init_resource::<BlockingRuntime>()
            .configure_sets(
                Update,
                (JobSet::Submit, JobSet::Collect, JobSet::Apply).chain(),
            )
            .configure_sets(PostUpdate, JobSet::Cleanup);
    }
}

pub struct JobPlugin<J: Job>(PhantomData<fn() -> J>);

impl<J: Job> Default for JobPlugin<J> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<J: Job> Plugin for JobPlugin<J> {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<JobsPlugin>() {
            app.add_plugins(JobsPlugin);
        }
        app.add_message::<JobReady<J>>()
            .add_systems(
                Update,
                (cancel::<J>, submit::<J>).chain().in_set(JobSet::Submit),
            )
            .add_systems(Update, collect::<J>.in_set(JobSet::Collect))
            .add_systems(
                PostUpdate,
                (cancel::<J>, cleanup::<J>).chain().in_set(JobSet::Cleanup),
            );
    }
}

pub trait Job: Clone + Send + Sync + 'static {
    type Output: Send + Sync + 'static;

    fn run(self) -> Self::Output;

    fn lane(&self) -> Lane {
        Lane::Short
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum JobSet {
    Submit,
    Collect,
    Apply,
    Cleanup,
}

#[derive(Resource, Clone)]
pub struct JobWake(pub Waker);

#[derive(Component)]
#[component(immutable)]
pub struct JobRequest<J: Job>(pub J);

#[derive(Component)]
pub struct JobResult<J: Job>(pub Result<J::Output, String>);

#[derive(Message)]
pub struct JobReady<J: Job> {
    pub entity: Entity,
    marker: PhantomData<fn() -> J>,
}

impl<J: Job> JobReady<J> {
    fn of(entity: Entity) -> Self {
        Self {
            entity,
            marker: PhantomData,
        }
    }
}

#[derive(Component)]
struct JobRunning<J: Job> {
    task: BlockingTask<J::Output>,
}

#[derive(Component)]
#[component(immutable)]
pub struct JobOwner<J: Job> {
    owner: Option<Entity>,
    marker: PhantomData<fn() -> J>,
}

impl<J: Job> JobOwner<J> {
    pub fn accepts(&self, parent: Option<&ChildOf>) -> bool {
        parent.map(ChildOf::parent) == self.owner
    }

    fn of(parent: Option<&ChildOf>) -> Self {
        Self {
            owner: parent.map(ChildOf::parent),
            marker: PhantomData,
        }
    }
}

type PendingJobs<'w, 's, J> = Query<
    'w,
    's,
    (
        Entity,
        &'static JobRequest<J>,
        Option<&'static ChildOf>,
        &'static OperationState,
        Option<&'static JobOwner<J>>,
    ),
    (
        With<Operation>,
        Without<JobRunning<J>>,
        Without<JobResult<J>>,
    ),
>;

type RunningJobs<'w, 's, J> = Query<
    'w,
    's,
    (
        Entity,
        &'static mut JobRunning<J>,
        &'static JobOwner<J>,
        Option<&'static ChildOf>,
        &'static OperationState,
    ),
    With<Operation>,
>;

type OwnedJobs<'w, 's, J> = Query<
    'w,
    's,
    (
        Entity,
        &'static JobOwner<J>,
        Option<&'static ChildOf>,
        &'static OperationState,
    ),
    With<Operation>,
>;

type CompletedJobs<'w, 's, J> = Query<
    'w,
    's,
    (Entity, &'static OperationState),
    (
        With<Operation>,
        Or<(With<JobRequest<J>>, With<JobRunning<J>>, With<JobResult<J>>)>,
    ),
>;

fn submit<J: Job>(
    requests: PendingJobs<J>,
    runtime: Res<BlockingRuntime>,
    wake: Option<Res<JobWake>>,
    mut ready: MessageWriter<JobReady<J>>,
    mut commands: Commands,
) {
    for (entity, request, parent, state, owner) in &requests {
        if !matches!(state, OperationState::Pending | OperationState::Running) {
            continue;
        }
        if let Some(owner) = owner {
            if !owner.accepts(parent) {
                continue;
            }
        } else {
            commands.entity(entity).insert(JobOwner::<J>::of(parent));
        }
        let job = request.0.clone();
        let lane = job.lane();
        let wake = wake.as_deref().map(|wake| wake.0.clone());
        match runtime.try_spawn(lane, wake, move || job.run()) {
            Ok(task) => {
                commands
                    .entity(entity)
                    .remove::<JobRequest<J>>()
                    .insert((OperationState::Running, JobRunning::<J> { task }));
            }
            Err(SpawnError::Full) => {}
            Err(error) => {
                commands
                    .entity(entity)
                    .remove::<JobRequest<J>>()
                    .insert(JobResult::<J>(Err(error.to_string())));
                ready.write(JobReady::of(entity));
            }
        }
    }
}

fn collect<J: Job>(
    mut running: RunningJobs<J>,
    mut ready: MessageWriter<JobReady<J>>,
    mut commands: Commands,
) {
    for (entity, mut running, owner, parent, state) in &mut running {
        if !matches!(state, OperationState::Pending | OperationState::Running)
            || !owner.accepts(parent)
        {
            continue;
        }
        let Some(result) = running.task.poll() else {
            continue;
        };
        let result = result.map_err(|panic| WorkerPanic(panic).message());
        commands
            .entity(entity)
            .remove::<JobRunning<J>>()
            .insert(JobResult::<J>(result));
        ready.write(JobReady::of(entity));
    }
}

fn cancel<J: Job>(jobs: OwnedJobs<J>, mut commands: Commands) {
    for (entity, owner, parent, state) in &jobs {
        let changed_owner = !owner.accepts(parent);
        if !changed_owner && matches!(state, OperationState::Pending | OperationState::Running) {
            continue;
        }
        commands
            .entity(entity)
            .remove::<(JobRunning<J>, JobRequest<J>, JobResult<J>, JobOwner<J>)>();
        if changed_owner && matches!(state, OperationState::Pending | OperationState::Running) {
            commands.entity(entity).insert(OperationState::Canceled);
        }
    }
}

fn cleanup<J: Job>(jobs: CompletedJobs<J>, mut commands: Commands) {
    for (entity, state) in &jobs {
        if matches!(state, OperationState::Pending | OperationState::Running) {
            continue;
        }
        commands
            .entity(entity)
            .remove::<(JobRequest<J>, JobRunning<J>, JobResult<J>, JobOwner<J>)>();
    }
}

struct WorkerPanic(Box<dyn Any + Send>);

impl WorkerPanic {
    fn message(&self) -> String {
        if let Some(message) = self.0.downcast_ref::<String>() {
            return message.clone();
        }
        if let Some(message) = self.0.downcast_ref::<&str>() {
            return message.to_string();
        }
        "blocking job panicked".to_string()
    }
}
