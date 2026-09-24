use std::path::Path;

use crate::manifest::{
    ToolStore, add_packages, expand_user_path, load_manifest_from, normalize_names,
    write_manifest_to,
};

pub fn import_npm_manifest(path: &Path) -> Result<usize, String> {
    ToolStore::current().import_npm_manifest(path)
}

impl ToolStore {
    pub fn import_npm_manifest(&self, path: &Path) -> Result<usize, String> {
        self.migrate_legacy_storage()?;
        let path = self.expand_user_path(path)?;
        import_npm_manifest_to(&path, &self.manifest_path())
    }
}

pub fn import_npm_manifest_to(path: &Path, manifest_path: &Path) -> Result<usize, String> {
    let path = expand_user_path(path)?;
    let source = std::fs::read_to_string(&path).map_err(|error| error.to_string())?;
    let packages = parse_npm_manifest(&source)?;
    if packages.is_empty() {
        return Err(format!("no dependencies found in {}", path.display()));
    }
    let mut manifest = load_manifest_from(manifest_path)?;
    let imported = add_packages(&mut manifest, "npm", &packages);
    write_manifest_to(manifest_path, &manifest)?;
    Ok(imported)
}

pub fn parse_npm_manifest(source: &str) -> Result<Vec<String>, String> {
    let document: serde_json::Value =
        serde_json::from_str(source).map_err(|error| error.to_string())?;
    let mut packages = Vec::new();
    for field in ["dependencies", "devDependencies", "optionalDependencies"] {
        if let Some(entries) = document.get(field).and_then(serde_json::Value::as_object) {
            packages.extend(entries.keys().cloned());
        }
    }
    normalize_names(&mut packages);
    Ok(packages)
}
