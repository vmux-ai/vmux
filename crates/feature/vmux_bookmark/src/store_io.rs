use std::path::Path;

pub(crate) struct StoreIo;

impl StoreIo {
    pub(crate) fn read_to_string(path: &Path) -> std::io::Result<String> {
        std::fs::read_to_string(path)
    }
}
