use std::io;
use std::path::Path;

use super::{DirectoryAnchor, NativeRelocation};

impl NativeRelocation {
    pub fn relocate_no_replace(source: &Path, target: &Path) -> io::Result<()> {
        let source = DirectoryAnchor::open(source)?;
        let target = DirectoryAnchor::open(target)?;
        let status = unsafe {
            libc::renameat2(
                source.raw_fd(),
                source.name_ptr(),
                target.raw_fd(),
                target.name_ptr(),
                libc::RENAME_NOREPLACE,
            )
        };
        if status == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }
}
