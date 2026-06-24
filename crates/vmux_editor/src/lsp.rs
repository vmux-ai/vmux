use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use bevy::prelude::*;

pub mod client;
pub mod framing;
pub mod manager;
pub mod registry;

/// Diagnostics produced by any server, keyed by absolute file path. Drained once
/// per frame by `manager::drain_lsp_diagnostics`. Mirrors `vmux_git::GitOutbox`.
#[derive(Resource, Clone, Default)]
pub struct LspOutbox(pub Arc<Mutex<Vec<(PathBuf, Vec<lsp_types::Diagnostic>)>>>);

/// Identifies a running server: workspace root + server command.
pub type ServerKey = (PathBuf, String);

/// A document currently opened against a server.
pub struct OpenDoc {
    pub key: ServerKey,
    pub version: i32,
}

pub type PendingMap = Arc<Mutex<HashMap<i64, std::sync::mpsc::Sender<serde_json::Value>>>>;

pub struct LspPlugin;

impl Plugin for LspPlugin {
    fn build(&self, app: &mut App) {
        let outbox = LspOutbox::default();
        app.insert_resource(outbox.clone());
        manager::build(app, outbox);
    }
}
