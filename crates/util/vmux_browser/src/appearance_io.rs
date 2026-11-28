use std::path::Path;

pub(crate) struct AppearanceIo;

impl AppearanceIo {
    pub(crate) fn read_catalog(directory: &Path, tags: [&str; 2]) -> Option<String> {
        tags.into_iter()
            .find_map(|tag| std::fs::read_to_string(directory.join(format!("{tag}.ftl"))).ok())
    }
}
