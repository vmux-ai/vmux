use std::io;
use std::path::Path;

use super::NativeRelocation;

impl NativeRelocation {
    pub fn relocate_no_replace(_source: &Path, _target: &Path) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "atomic no-replace rename is unsupported on this platform",
        ))
    }
}
