#[cfg(not(unix))]
mod other;
#[cfg(unix)]
mod unix;

pub(crate) struct FileAttributes;
