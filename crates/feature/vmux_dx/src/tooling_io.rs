use std::ffi::OsString;
use std::fs;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;

use bevy_ecs::prelude::Component;
use vmux_job::Job;
use vmux_job::blocking::Lane;

use crate::execution::Failure;
use crate::source::Source;
use crate::storage_io::{Done, StorageJob};
use crate::target_lock::TargetLock;

#[derive(Component, Clone)]
pub(crate) struct SourceKey(pub PathBuf);

impl StorageJob for SourceKey {
    type Data = Fingerprint;
}

impl Job for SourceKey {
    type Output = Result<Fingerprint, Failure>;

    fn lane(&self) -> Lane {
        Lane::Process
    }

    fn run(self) -> Self::Output {
        Source::key(&self.0, vmux_job::blocking::BlockingCancellation::canceled)
            .map(Fingerprint)
            .map_err(|error| Failure::message(error.to_string()))
    }
}

#[derive(Component, Clone)]
pub(crate) struct Fingerprint(pub String);

#[derive(Component, Clone)]
pub(crate) struct CreateDirectory(pub PathBuf);

impl StorageJob for CreateDirectory {
    type Data = Done;
}

impl Job for CreateDirectory {
    type Output = Result<Done, Failure>;

    fn lane(&self) -> Lane {
        Lane::Process
    }

    fn run(self) -> Self::Output {
        fs::create_dir_all(self.0)
            .map(|()| Done)
            .map_err(|error| Failure::message(error.to_string()))
    }
}

#[derive(Component, Clone)]
pub(crate) struct Acquire {
    pub root: PathBuf,
    pub target: PathBuf,
}

impl StorageJob for Acquire {
    type Data = Lock;
}

impl Job for Acquire {
    type Output = Result<Lock, Failure>;

    fn lane(&self) -> Lane {
        Lane::Process
    }

    fn run(self) -> Self::Output {
        TargetLock::acquire(&self.root, &self.target)
            .map(|lock| Lock(Arc::new(lock)))
            .map_err(Failure::from_error)
    }
}

#[derive(Component, Clone)]
pub(crate) struct Lock(pub Arc<TargetLock>);

#[derive(Component, Clone)]
pub(crate) struct Handoff {
    pub binary: PathBuf,
    pub arguments: Vec<OsString>,
}

impl StorageJob for Handoff {
    type Data = Done;
}

impl Job for Handoff {
    type Output = Result<Done, Failure>;

    fn lane(&self) -> Lane {
        Lane::Process
    }

    fn run(self) -> Self::Output {
        let error = Command::new(self.binary)
            .args(self.arguments)
            .env("VMUX_DX_HANDOFF", "1")
            .exec();
        Err(Failure::message(error.to_string()))
    }
}

pub(crate) struct Scoped<J: StorageJob>(pub J);

impl<J: StorageJob> Clone for Scoped<J> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<J: StorageJob> Job for Scoped<J> {
    type Output = Result<J::Data, Failure>;

    fn lane(&self) -> Lane {
        self.0.lane()
    }

    fn run(self) -> Self::Output {
        if vmux_job::blocking::BlockingCancellation::canceled() {
            return Err(Failure::message("CLI bootstrap canceled"));
        }
        self.0.run()
    }
}
