use std::path::PathBuf;

use vmux_setting::AppSettings;

/// Resolves the configured screenshot and recording output directory.
pub(crate) fn output_dir(settings: &AppSettings) -> PathBuf {
    settings
        .recording
        .output_dir
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(vmux_core::profile::recording_dir)
}
