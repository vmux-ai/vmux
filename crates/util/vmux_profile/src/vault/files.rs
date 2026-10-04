#[cfg(not(unix))]
mod other;
#[cfg(unix)]
mod unix;

pub(super) struct FileAttributes;
