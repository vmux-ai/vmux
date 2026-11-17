use std::path::PathBuf;
use std::time::Instant;

#[derive(Default)]
pub(super) struct SelfWrites(pub Inputs);

#[derive(Default)]
pub(super) struct Inputs(Vec<Completed>);

pub(super) struct Completed {
    pub path: PathBuf,
    pub at: Instant,
}

impl Inputs {
    pub(super) fn insert(&mut self, path: PathBuf, at: Instant) {
        self.0.push(Completed { path, at });
    }

    pub(super) fn take(&mut self) -> Vec<Completed> {
        std::mem::take(&mut self.0)
    }
}
