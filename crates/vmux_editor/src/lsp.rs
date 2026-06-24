use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use bevy::prelude::*;

pub mod archive;
pub mod catalog;
pub mod client;
pub mod download;
pub mod framing;
pub mod install;
pub mod lint;
pub mod manager;
pub mod manager_page;
pub mod purl;
pub mod registry;
pub mod store;
pub mod target;

/// Diagnostics for one file: its path plus the server's latest diagnostic set.
pub type PathDiagnostics = (PathBuf, Vec<lsp_types::Diagnostic>);

/// Diagnostics produced by any server, keyed by absolute file path. Drained once
/// per frame by `manager::drain_lsp_diagnostics`. Mirrors `vmux_git::GitOutbox`.
#[derive(Resource, Clone, Default)]
pub struct LspOutbox(pub Arc<Mutex<Vec<PathDiagnostics>>>);

/// Linter findings (already converted to `FileDiagnostic`) per file path,
/// produced off-thread by the lint runner and merged with LSP diagnostics.
#[derive(Resource, Clone, Default)]
pub struct LintOutbox(pub Arc<Mutex<Vec<(PathBuf, Vec<vmux_core::event::FileDiagnostic>)>>>);

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
        app.add_plugins(manager_page::ManagerPlugin);
    }
}
