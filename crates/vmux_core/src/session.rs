//! Session file path and deferred save queue (layout pushes paths; `vmux_session` drains and writes).

use std::path::PathBuf;

use bevy::prelude::*;

/// Moonshine session file path (shared by `vmux` and `vmux_webview`).
#[derive(Resource, Clone, Debug)]
pub struct SessionSavePath(pub PathBuf);

/// Path to `navigation_history.ron` under the vmux cache directory.
#[derive(Resource, Clone, Debug)]
pub struct NavigationHistoryPath(pub PathBuf);

/// Paths to flush to disk (populated by layout/input when the snapshot should be persisted).
#[derive(Resource, Default)]
pub struct SessionSaveQueue(pub Vec<PathBuf>);

/// Paths to flush `navigation_history.ron` (populated when [`crate::NavigationHistory`] changes).
#[derive(Resource, Default)]
pub struct NavigationHistorySaveQueue(pub Vec<PathBuf>);
