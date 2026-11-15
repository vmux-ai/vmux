use std::path::PathBuf;

use bevy_ecs::prelude::Component;
use vmux_ecs::blocking::Lane;
use vmux_ecs::jobs::Job;

use crate::execution::Failure;
use crate::storage_io::{Done, StorageJob};

#[derive(Component, Clone)]
#[component(immutable)]
pub(crate) struct Rename {
    pub source: PathBuf,
    pub destination: PathBuf,
}

impl StorageJob for Rename {
    type Data = Done;
}

impl Job for Rename {
    type Output = Result<Done, Failure>;

    fn lane(&self) -> Lane {
        Lane::Process
    }

    fn run(self) -> Self::Output {
        if vmux_ecs::blocking::BlockingCancellation::canceled() {
            return Err(Failure::message("Layout reset canceled"));
        }
        std::fs::rename(self.source, self.destination)
            .map(|()| Done)
            .map_err(|error| Failure::message(error.to_string()))
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
        self.0.run()
    }
}
