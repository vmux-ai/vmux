use vmux_core::extension::store;

use super::runtime::{self, PreparedRuntime};

pub fn apply_env() -> Result<Vec<PreparedRuntime>, String> {
    let root = store::root();
    let profile = vmux_core::profile::active_profile_name();
    let mut idx = store::Index::load(&root)?;
    let mut prepared = Vec::new();
    let mut index_changed = false;
    for entry in idx.entries.iter_mut().filter(|entry| entry.enabled) {
        let item = runtime::prepare_runtime(&root, &profile, entry)?;
        if entry.source_hash.is_empty() {
            entry.source_hash.clone_from(&item.source_hash);
            index_changed = true;
        }
        prepared.push(item);
    }
    if index_changed {
        idx.save(&root)?;
    }
    let dirs = prepared
        .iter()
        .map(|item| item.dir.to_string_lossy())
        .collect::<Vec<_>>();
    if dirs.is_empty() {
        unsafe { std::env::remove_var("VMUX_LOAD_EXTENSIONS") };
    } else {
        unsafe { std::env::set_var("VMUX_LOAD_EXTENSIONS", dirs.join(",")) };
    }
    std::fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    std::fs::write(root.join("loaded.txt"), idx.enabled_ids().join("\n"))
        .map_err(|error| error.to_string())?;
    Ok(prepared)
}

pub fn loaded_ids() -> Vec<String> {
    let path = store::root().join("loaded.txt");
    std::fs::read_to_string(path)
        .ok()
        .map(|s| {
            s.lines()
                .filter(|l| !l.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}
