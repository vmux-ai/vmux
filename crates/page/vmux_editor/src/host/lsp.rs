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
pub mod reader;
pub mod registry;
pub mod semantic;
pub mod server_request;
pub mod store;
pub mod target;
pub mod wire;
pub mod workspace_edit;

impl Plugin for LspPlugin {
    fn build(&self, app: &mut App) {
        let outbox = LspOutbox::default();
        app.insert_resource(outbox.clone())
            .add_plugins(server_request::ServerRequestPlugin);
        manager::build(app, outbox);
        app.add_plugins(manager_page::ManagerPlugin);
    }
}

pub struct LspPlugin;

pub type PathDiagnostics = (PathBuf, Vec<lsp_types::Diagnostic>);

#[derive(Resource, Clone, Default)]
pub struct LspOutbox(pub Arc<Mutex<Vec<PathDiagnostics>>>);

pub type PathLintDiagnostics = (PathBuf, Vec<vmux_core::event::FileDiagnostic>);

#[derive(Resource, Clone, Default)]
pub struct LintOutbox(pub Arc<Mutex<Vec<PathLintDiagnostics>>>);

pub type ServerKey = (PathBuf, String);

pub struct OpenDoc {
    pub key: ServerKey,
    pub version: i32,
    /// How many editor panes are showing this document. `didClose` waits for zero.
    pub refs: u32,
}

pub type PendingMap = Arc<Mutex<HashMap<i64, std::sync::mpsc::Sender<serde_json::Value>>>>;
