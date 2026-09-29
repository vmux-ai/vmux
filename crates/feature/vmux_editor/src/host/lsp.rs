use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use bevy::prelude::*;
use vmux_core::tool::{ToolOperationKey, ToolOperationKind, ToolProvider, ToolStatus};
use vmux_tool::{
    ToolInventory, ToolInventoryItem, ToolOperator, ToolProviderId, ToolProviderSnapshot,
    ToolScanner, ToolStore, ToolsManifest,
};

pub mod archive;
pub mod catalog;
pub mod client;
pub mod download;
pub mod framing;
pub mod install;
pub mod lint;
pub mod manager;
pub mod manager_page;
pub mod package_path;
pub mod purl;
pub mod reader;
pub mod registry;
pub mod semantic;
pub mod server_request;
pub mod store;
pub mod target;
pub mod wire;
pub mod workspace_edit;

pub struct LspPlugin;

impl Plugin for LspPlugin {
    fn build(&self, app: &mut App) {
        let (diagnostics, inbox) = LspDiagnosticsSender::channel();
        app.add_plugins(server_request::ServerRequestPlugin)
            .add_systems(Startup, spawn_tool_provider);
        manager::build(app, diagnostics, inbox);
        app.add_plugins(manager_page::ManagerPlugin);
    }
}

fn spawn_tool_provider(mut commands: Commands) {
    commands.spawn((
        Name::new("LSP tool provider"),
        ToolProviderId(ToolProvider::Lsp),
        ToolScanner::new(scan_tools),
        ToolOperator::new(operate_tool),
    ));
}

fn scan_tools(
    _tool_store: &ToolStore,
    manifest: &mut ToolsManifest,
    refresh: bool,
) -> Result<ToolProviderSnapshot, String> {
    let store = store::LspStore::current();
    let catalog = if refresh {
        catalog::ensure_catalog(&store, true).unwrap_or_default()
    } else if store.catalog_path().is_file() {
        let source =
            std::fs::read_to_string(store.catalog_path()).map_err(|error| error.to_string())?;
        catalog::parse_registry(&source).unwrap_or_default()
    } else {
        Vec::new()
    };
    let catalog_by_name = catalog
        .iter()
        .map(|package| (package.name.clone(), package))
        .collect::<BTreeMap<_, _>>();
    let receipts = store.installed();
    let mut inventory = receipts
        .into_values()
        .map(|receipt| {
            let package = catalog_by_name.get(&receipt.name).copied();
            let latest = package
                .and_then(|package| purl::parse(&package.source_id))
                .and_then(|purl| purl.version);
            ToolInventoryItem {
                id: receipt.name.as_str().to_string(),
                name: receipt.name.as_str().to_string(),
                icon: None,
                version: receipt.version.clone(),
                detail: package
                    .map(|package| package.description.clone())
                    .filter(|detail| !detail.is_empty())
                    .unwrap_or_else(|| "Vmux-managed language tool".to_string()),
                status: if receipt.version.is_some()
                    && latest.is_some()
                    && receipt.version != latest
                {
                    ToolStatus::Outdated
                } else {
                    ToolStatus::Installed
                },
                removable: true,
            }
        })
        .collect::<Vec<_>>();
    let installed = inventory
        .iter()
        .map(|item| item.id.clone())
        .collect::<BTreeSet<_>>();
    for package in catalog {
        if installed.contains(package.name.as_str()) {
            continue;
        }
        let on_path = package.bin.keys().any(|command| {
            matches!(
                store.resolve_command(command.as_str()),
                store::Resolution::OnPath
            )
        });
        if on_path {
            inventory.push(ToolInventoryItem {
                id: package.name.as_str().to_string(),
                name: package.name.as_str().to_string(),
                icon: None,
                version: None,
                detail: "Available on PATH".to_string(),
                status: ToolStatus::Installed,
                removable: false,
            });
        }
    }
    Ok(ToolInventory::new(ToolProvider::Lsp, inventory)
        .reconcile(manifest)
        .into())
}

fn operate_tool(
    tool_store: &ToolStore,
    operation: &ToolOperationKey,
    _value: &str,
) -> Result<String, String> {
    let id = operation.item_id.trim();
    match operation.kind {
        ToolOperationKind::Install | ToolOperationKind::Update => {
            if id.is_empty() {
                return Err("package name is required".to_string());
            }
            let store = store::LspStore::current();
            let packages = catalog::ensure_catalog(&store, false)?;
            let package = packages
                .iter()
                .find(|package| package.name.as_str() == id)
                .ok_or_else(|| format!("language tool not found: {id}"))?;
            install::install(package, &store, target::host_target(), |_, _, _| {})?;
            tool_store.set_managed_package(ToolProvider::Lsp, id, true)?;
            let operation = if operation.kind == ToolOperationKind::Install {
                "installed"
            } else {
                "updated"
            };
            Ok(format!("{id} {operation}"))
        }
        ToolOperationKind::Uninstall => {
            if id.is_empty() {
                return Err("package name is required".to_string());
            }
            let name = package_path::PackageName::parse(id)?;
            store::LspStore::current()
                .remove(&name)
                .map_err(|error| error.to_string())?;
            tool_store.set_managed_package(ToolProvider::Lsp, id, false)?;
            Ok(format!("{id} removed"))
        }
        ToolOperationKind::Forget => {
            tool_store.set_managed_package(ToolProvider::Lsp, id, false)?;
            Ok(format!("{id} removed from tools.toml"))
        }
        ToolOperationKind::Adopt => {
            tool_store.set_managed_package(ToolProvider::Lsp, id, true)?;
            Ok(format!("{id} is now managed"))
        }
        ToolOperationKind::Import => {
            let mut manifest = tool_store.load()?;
            let before = manifest.managed_packages(ToolProvider::Lsp.id()).len();
            let _ = scan_tools(tool_store, &mut manifest, false)?;
            let imported = manifest
                .managed_packages(ToolProvider::Lsp.id())
                .len()
                .saturating_sub(before);
            tool_store.save(&manifest)?;
            Ok(format!("imported {imported} lsp item(s)"))
        }
        _ => Err(format!("LSP does not support {:?}", operation.kind)),
    }
}

pub type PathDiagnostics = (PathBuf, Vec<lsp_types::Diagnostic>);

#[derive(Clone)]
pub struct LspDiagnosticsSender(crossbeam_channel::Sender<PathDiagnostics>);

impl LspDiagnosticsSender {
    pub fn channel() -> (Self, LspDiagnosticsInbox) {
        let (sender, receiver) = crossbeam_channel::unbounded();
        (Self(sender), LspDiagnosticsInbox(receiver))
    }

    pub(crate) fn send(&self, diagnostics: PathDiagnostics) {
        let _ = self.0.send(diagnostics);
    }
}

impl Default for LspDiagnosticsSender {
    fn default() -> Self {
        let (sender, _) = crossbeam_channel::unbounded();
        Self(sender)
    }
}

#[derive(Component)]
pub struct LspDiagnosticsInbox(pub crossbeam_channel::Receiver<PathDiagnostics>);

impl LspDiagnosticsInbox {
    pub(crate) fn drain(&self) -> Vec<PathDiagnostics> {
        self.0.try_iter().collect()
    }
}

pub type PathLintDiagnostics = (PathBuf, Vec<vmux_core::event::FileDiagnostic>);

#[derive(Component, Clone)]
pub struct LintDiagnosticsSender(crossbeam_channel::Sender<PathLintDiagnostics>);

impl LintDiagnosticsSender {
    fn channel() -> (Self, LintDiagnosticsInbox) {
        let (sender, receiver) = crossbeam_channel::unbounded();
        (Self(sender), LintDiagnosticsInbox(receiver))
    }

    fn send(&self, diagnostics: PathLintDiagnostics) {
        let _ = self.0.send(diagnostics);
    }
}

#[derive(Component)]
struct LintDiagnosticsInbox(crossbeam_channel::Receiver<PathLintDiagnostics>);

impl LintDiagnosticsInbox {
    fn drain(&self) -> Vec<PathLintDiagnostics> {
        self.0.try_iter().collect()
    }
}

pub type ServerKey = (PathBuf, String);

pub struct OpenDoc {
    pub key: ServerKey,
    pub version: i32,
    pub refs: u32,
}

pub type PendingMap = Arc<Mutex<HashMap<i64, crossbeam_channel::Sender<serde_json::Value>>>>;
