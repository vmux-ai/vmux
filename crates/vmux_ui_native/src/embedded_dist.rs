//! Extract [`rust_embed::RustEmbed`] folders to a temp dir for embedded HTTP static serving.

use std::path::PathBuf;

use rust_embed::RustEmbed;

/// Writes all files from an embedded folder to a fresh temp directory and returns its path if
/// `index.html` exists at the root.
pub fn extract_embedded_dist_to_temp<E: RustEmbed>(temp_dir_prefix: &str) -> Option<PathBuf> {
    if E::iter().next().is_none() {
        return None;
    }
    let base = std::env::temp_dir().join(format!("{temp_dir_prefix}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).ok()?;
    for path in E::iter() {
        let rel = path.as_ref();
        let embedded = E::get(rel)?;
        let out = base.join(rel);
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).ok()?;
        }
        std::fs::write(&out, embedded.data.as_ref())
            .map_err(|e| {
                bevy::log::error!("vmux_ui_native: extract embedded {rel}: {e}");
                e
            })
            .ok()?;
    }
    base.join("index.html").is_file().then_some(base)
}
