//! Static history UI (`dist/`) shipped in the binary; extracted to a temp dir for loopback HTTP.

use std::path::PathBuf;

use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "dist"]
struct HistoryWebDist;

/// Writes embedded `dist` files to a fresh temp directory and returns its path.
pub fn extract_embedded_history_dist() -> Option<PathBuf> {
    if HistoryWebDist::iter().next().is_none() {
        return None;
    }
    let base = std::env::temp_dir().join(format!("vmux-history-ui-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).ok()?;
    for path in HistoryWebDist::iter() {
        let rel = path.as_ref();
        let data = HistoryWebDist::get(rel)?;
        let out = base.join(rel);
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).ok()?;
        }
        std::fs::write(&out, data.data.as_ref()).map_err(|e| {
            bevy::log::error!("vmux history: extract {}: {e}", out.display());
            e
        })
        .ok()?;
    }
    base.join("index.html").is_file().then_some(base)
}
