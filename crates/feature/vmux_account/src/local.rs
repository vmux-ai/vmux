use std::path::PathBuf;

use bevy_app::{App, Update};
use bevy_ecs::prelude::*;
use vmux_ecs::{JobWake, Operation};
use vmux_job::{Job, JobOwner, JobPlugin, JobRequest, JobResult, JobSet};

use crate::local_io::Database;
use crate::{
    Commit, Loaded, Location, ReadCompleted, ReadRequest, Root, Set, WriteCompleted, WriteFailure,
    WriteRequest,
};

pub struct LocalPlugin;

impl bevy_app::Plugin for LocalPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            JobPlugin::<ReadJob>::default(),
            JobPlugin::<WriteJob>::default(),
        ))
        .add_systems(Update, (read, write).in_set(Set::Dispatch))
        .add_systems(Update, (receive_read, receive_write).in_set(JobSet::Apply));
    }
}

#[derive(Clone)]
struct ReadJob {
    data: PathBuf,
}

impl Job for ReadJob {
    type Output = Result<Loaded, String>;

    fn run(self) -> Self::Output {
        Database::read(&self.data)
    }
}

#[derive(Clone)]
struct WriteJob {
    data: PathBuf,
    request: WriteRequest,
}

impl Job for WriteJob {
    type Output = Result<Commit, WriteFailure>;

    fn run(self) -> Self::Output {
        Database::write(&self.data, &self.request)
    }
}

fn read(
    mut requests: MessageReader<ReadRequest>,
    locations: Query<&Location, With<Root>>,
    active: Query<&ChildOf, With<Operation>>,
    wake: Option<Res<JobWake>>,
    mut commands: Commands,
) {
    for request in requests.read() {
        let Ok(parent) = active.get(request.entity) else {
            continue;
        };
        if parent.parent() != request.root {
            continue;
        }
        let Ok(location) = locations.get(request.root) else {
            continue;
        };
        commands.entity(request.entity).insert(JobRequest(ReadJob {
            data: location.0.clone(),
        }));
        if let Some(wake) = &wake {
            wake.0.wake_by_ref();
        }
    }
}

fn write(
    mut requests: MessageReader<WriteRequest>,
    locations: Query<&Location, With<Root>>,
    active: Query<&ChildOf, With<Operation>>,
    wake: Option<Res<JobWake>>,
    mut commands: Commands,
    #[cfg(feature = "profile-sync")] sync_context: crate::sync_runtime::Context,
    #[cfg(feature = "profile-sync")] mut replies: MessageWriter<WriteCompleted>,
) {
    for request in requests.read() {
        let Ok(parent) = active.get(request.entity) else {
            continue;
        };
        if parent.parent() != request.root {
            continue;
        }
        #[cfg(feature = "profile-sync")]
        if request
            .scope
            .as_ref()
            .is_some_and(|scope| !sync_context.accepts_scope(request.root, scope))
        {
            replies.write(WriteCompleted {
                entity: request.entity,
                result: Some(Err(WriteFailure::Conflict)),
            });
            continue;
        }
        let Ok(location) = locations.get(request.root) else {
            continue;
        };
        commands.entity(request.entity).insert(JobRequest(WriteJob {
            data: location.0.clone(),
            request: request.clone(),
        }));
        if let Some(wake) = &wake {
            wake.0.wake_by_ref();
        }
    }
}

fn receive_read(
    mut results: Results<ReadJob>,
    mut replies: MessageWriter<ReadCompleted>,
    mut commands: Commands,
) {
    for (entity, mut result, owner, parent) in &mut results {
        if !owner.accepts(parent) {
            continue;
        }
        let value = std::mem::replace(&mut result.0, Err("read result consumed".into()));
        let value = match value {
            Ok(value) => value,
            Err(error) => Err(error),
        };
        replies.write(ReadCompleted {
            entity,
            result: Some(value),
        });
        commands
            .entity(entity)
            .remove::<JobResult<ReadJob>>()
            .remove::<JobRequest<ReadJob>>();
    }
}

fn receive_write(
    mut results: Results<WriteJob>,
    mut replies: MessageWriter<WriteCompleted>,
    mut commands: Commands,
) {
    for (entity, mut result, owner, parent) in &mut results {
        if !owner.accepts(parent) {
            continue;
        }
        let value = std::mem::replace(&mut result.0, Err("write result consumed".into()));
        let value = match value {
            Ok(value) => value,
            Err(error) => Err(WriteFailure::Storage(error)),
        };
        replies.write(WriteCompleted {
            entity,
            result: Some(value),
        });
        commands
            .entity(entity)
            .remove::<JobResult<WriteJob>>()
            .remove::<JobRequest<WriteJob>>();
    }
}

type Results<'w, 's, J> = Query<
    'w,
    's,
    (
        Entity,
        &'static mut JobResult<J>,
        &'static JobOwner<J>,
        Option<&'static ChildOf>,
    ),
>;
