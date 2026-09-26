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

impl Plugin for LspPlugin {
    fn build(&self, app: &mut App) {
        let outbox = LspOutbox::default();
        app.insert_resource(outbox.clone())
            .add_plugins(server_request::ServerRequestPlugin)
            .add_systems(Startup, spawn_tool_provider);
        manager::build(app, outbox);
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
    let root = store::default_root();
    let catalog = if refresh {
        catalog::ensure_catalog(&root, true).unwrap_or_default()
    } else if catalog::cached_path(&root).is_file() {
        let source = std::fs::read_to_string(catalog::cached_path(&root))
            .map_err(|error| error.to_string())?;
        catalog::parse_registry(&source).unwrap_or_default()
    } else {
        Vec::new()
    };
    let catalog_by_name = catalog
        .iter()
        .map(|package| (package.name.clone(), package))
        .collect::<BTreeMap<_, _>>();
    let receipts = store::installed(&root);
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
                store::resolved_command(&root, command.as_str()),
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
            let root = store::default_root();
            let packages = catalog::ensure_catalog(&root, false)?;
            let package = packages
                .iter()
                .find(|package| package.name.as_str() == id)
                .ok_or_else(|| format!("language tool not found: {id}"))?;
            install::install(package, &root, target::host_target(), |_, _, _| {})?;
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
            store::remove(&store::default_root(), &name).map_err(|error| error.to_string())?;
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
    pub refs: u32,
}

pub type PendingMap = Arc<Mutex<HashMap<i64, crossbeam_channel::Sender<serde_json::Value>>>>;
