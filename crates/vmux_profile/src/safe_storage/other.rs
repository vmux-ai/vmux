use super::RootKeyStore;

impl RootKeyStore {
    pub(super) fn read(_silent: bool) -> Result<Option<Vec<u8>>, String> {
        Err("Vmux Safe Storage is only available on macOS".to_string())
    }

    pub(super) fn write(_payload: &[u8]) -> Result<(), String> {
        Err("Vmux Safe Storage is only available on macOS".to_string())
    }
}
