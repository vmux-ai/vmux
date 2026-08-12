//! Filesystem attributes the Vault records alongside each entry, isolated by
//! platform family.

#[cfg(not(unix))]
mod other;
#[cfg(unix)]
mod unix;

/// POSIX metadata for a Vault entry — permission bits and symlink targets.
/// Every operation is implemented once per platform family in a sibling module,
/// exactly one of which is compiled.
pub(super) struct FileAttributes;
