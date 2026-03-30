//! Session file path and deferred save queue (layout pushes paths; `vmux_session` drains and writes).

use std::path::PathBuf;

use bevy::prelude::*;

/// Moonshine session file path (shared by `vmux` and `vmux_webview`).
#[derive(Resource, Clone, Debug)]
pub struct SessionSavePath(pub PathBuf);

/// Paths to flush to disk (populated by layout/input when the snapshot should be persisted).
#[derive(Resource, Default)]
pub struct SessionSaveQueue(pub Vec<PathBuf>);
