use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use bevy::prelude::*;
use bevy::prelude::{App, Commands, Component, Plugin};
use vmux_feature::manifest::FeaturePlugin;
use vmux_tool::ToolProviderBinding;

pub use vmux_lsp::{
    DiagnosticsInbox as LspDiagnosticsInbox, DiagnosticsSender as LspDiagnosticsSender,
    ReceivedDiagnostics as PathDiagnostics,
};
pub use vmux_lsp::{Document as OpenDoc, ServerKey};

pub mod activation;
pub mod activation_io;
pub mod archive;
pub mod catalog;
mod catalog_io;
mod catalog_runtime;
pub mod client;
pub mod download;
pub mod framing;
pub mod install_io;
mod install_runtime;
mod inventory;
pub mod lint;
pub mod manager;
pub mod manager_page;
pub mod package_path;
pub mod purl;
pub mod reader;
pub mod registry;
mod removal;
pub mod semantic;
pub mod server_request;
pub mod store;
pub mod target;
mod tool_runtime;
pub mod wire;
pub mod workspace_edit;

pub struct LspPlugin;

impl Plugin for LspPlugin {
    fn build(&self, app: &mut App) {
        let (diagnostics, inbox) = LspDiagnosticsSender::channel();
        app.add_plugins(FeaturePlugin::<crate::Feature>::default());
        app.add_plugins((
            registry::RegistryPlugin,
            catalog::CatalogPlugin,
            inventory::LspInventoryPlugin,
            tool_runtime::LspToolPlugin,
            server_request::ServerRequestPlugin,
            manager::ManagerPlugin::new(diagnostics, inbox),
        ))
        .add_systems(Startup, spawn_tool_provider);
        app.add_plugins(manager_page::ManagerPlugin);
    }
}

fn spawn_tool_provider(mut commands: Commands) {
    let provider = commands
        .spawn(ToolProviderBinding::new::<crate::Feature>(0))
        .id();
    commands.entity(provider).insert((
        vmux_tool::observation::Handler,
        vmux_tool::observation::PackageProvider,
    ));
}

#[derive(Component, Clone)]
pub struct LintDiagnosticsSender(
    crossbeam_channel::Sender<(PathBuf, Vec<vmux_ecs::event::FileDiagnostic>)>,
);

impl LintDiagnosticsSender {
    fn channel() -> (Self, LintDiagnosticsInbox) {
        let (sender, receiver) = crossbeam_channel::unbounded();
        (Self(sender), LintDiagnosticsInbox(receiver))
    }

    fn send(&self, diagnostics: (PathBuf, Vec<vmux_ecs::event::FileDiagnostic>)) {
        let _ = self.0.send(diagnostics);
    }
}

#[derive(Component, Clone)]
struct LintDiagnosticsInbox(
    crossbeam_channel::Receiver<(PathBuf, Vec<vmux_ecs::event::FileDiagnostic>)>,
);

impl LintDiagnosticsInbox {
    fn drain(&self) -> Vec<(PathBuf, Vec<vmux_ecs::event::FileDiagnostic>)> {
        self.0.try_iter().collect()
    }
}

pub type PendingMap = Arc<Mutex<HashMap<i64, crossbeam_channel::Sender<serde_json::Value>>>>;
