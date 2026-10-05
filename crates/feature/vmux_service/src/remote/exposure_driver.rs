use bevy::prelude::Component;

use crate::RemotePaths;

#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub(super) struct RemoteExposure(pub(super) bool);

impl RemoteExposure {
    pub(super) fn current() -> Self {
        Self::read(&RemotePaths::current().state())
    }

    fn read(path: &std::path::Path) -> Self {
        Self(std::fs::read_to_string(path).is_ok_and(|state| state.trim() == "enabled"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_exposure_marker() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("remote-state");
        assert!(!RemoteExposure::read(&path).0);
        std::fs::write(&path, b"disabled\n").unwrap();
        assert!(!RemoteExposure::read(&path).0);
        std::fs::write(&path, b"enabled\n").unwrap();
        assert!(RemoteExposure::read(&path).0);
    }
}
