use vmux_core::extension::store;

pub fn apply_env() {
    let root = store::root();
    let Ok(idx) = store::Index::load(&root) else {
        return;
    };
    let dirs: Vec<String> = idx
        .enabled_dirs(&root)
        .into_iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();
    if !dirs.is_empty() {
        unsafe { std::env::set_var("VMUX_LOAD_EXTENSIONS", dirs.join(",")) };
    }
    let _ = std::fs::create_dir_all(&root);
    let _ = std::fs::write(root.join("loaded.txt"), idx.enabled_ids().join("\n"));
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
