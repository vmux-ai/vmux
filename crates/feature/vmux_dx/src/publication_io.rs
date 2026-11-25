use vmux_job::Job;

use crate::execution::Failure;
use crate::storage_io::StorageJob;

pub(crate) struct Scoped<J: StorageJob>(pub J);

impl<J: StorageJob> Clone for Scoped<J> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<J: StorageJob> Job for Scoped<J> {
    type Output = Result<J::Data, Failure>;

    fn lane(&self) -> vmux_job::blocking::Lane {
        self.0.lane()
    }

    fn run(self) -> Self::Output {
        self.0.run()
    }
}
