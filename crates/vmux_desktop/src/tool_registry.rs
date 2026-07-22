use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::path::Path;
use std::process::{Command, Output};

use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use bevy_cef::prelude::{BinEventEmitterPlugin, BinHostEmitEvent, BinReceive, Browsers};
use vmux_core::page::{PageManifest, PageReady, PrewarmPage};
use vmux_core::profile::registry::{self as manifest_store, RegistryManifest};
use vmux_core::registry::{
    REGISTRY_ACTION_RESULT_EVENT, REGISTRY_SNAPSHOT_EVENT, RegistryAction, RegistryActionRequest,
    RegistryActionResult, RegistryCategory, RegistryItem, RegistryProvider, RegistryRefreshRequest,
    RegistrySnapshot, RegistryStatus,
};
use vmux_layout::LayoutCef;

const PAGE_MANIFEST: PageManifest = PageManifest {
    host: "registry",
    title: "Registry",
    keywords: &[
        "registry", "packages", "tools", "dotfiles", "homebrew", "npm", "mcp", "import",
    ],
    icon: Some(vmux_core::BuiltinIcon::Layers),
    command_bar: true,
};

pub struct ToolRegistryPlugin;

#[derive(Resource)]
struct ToolRegistryState {
    dirty: bool,
    refresh_catalogs: bool,
    generation: u64,
    revision: u64,
    loaded: bool,
    snapshot: RegistrySnapshot,
    subscribers: HashMap<Entity, u64>,
}

impl Default for ToolRegistryState {
    fn default() -> Self {
        Self {
            dirty: true,
            refresh_catalogs: false,
            generation: 1,
            revision: 0,
            loaded: false,
            snapshot: RegistrySnapshot::default(),
            subscribers: HashMap::new(),
        }
    }
}

#[derive(Component)]
struct RegistryScanTask {
    generation: u64,
    task: Task<RegistrySnapshot>,
}

#[derive(Component)]
struct RegistryActionTask {
    target: Entity,
    request: RegistryActionRequest,
    task: Task<Result<String, String>>,
}

#[derive(Resource, Default)]
struct RegistryActionQueue(VecDeque<(Entity, RegistryActionRequest)>);

#[derive(Clone, Debug)]
struct InventoryItem {
    id: String,
    name: String,
    version: Option<String>,
    detail: String,
    status: RegistryStatus,
    removable: bool,
}

impl Plugin for ToolRegistryPlugin {
    fn build(&self, app: &mut App) {
        app.world_mut().spawn((
            PAGE_MANIFEST,
            PrewarmPage {
                host: "registry",
                url: "vmux://registry/",
                title: "Registry",
                pool_size: 1,
            },
        ));
        vmux_core::register_host_spawn(app, "registry");
        app.init_resource::<ToolRegistryState>()
            .init_resource::<RegistryActionQueue>()
            .add_plugins(BinEventEmitterPlugin::<(
                RegistryRefreshRequest,
                RegistryActionRequest,
            )>::default())
            .add_observer(on_refresh_request)
            .add_observer(on_action_request)
            .add_systems(
                Update,
                (
                    start_registry_scan,
                    drain_registry_scan,
                    start_registry_action,
                    drain_registry_actions,
                    emit_registry_snapshot,
                )
                    .chain(),
            );
    }
}

fn on_refresh_request(
    trigger: On<BinReceive<RegistryRefreshRequest>>,
    mut state: ResMut<ToolRegistryState>,
) {
    let request = &trigger.event().payload;
    state.subscribers.insert(trigger.event().webview, 0);
    if request.refresh || !state.loaded {
        state.dirty = true;
        state.refresh_catalogs |= request.refresh;
        state.generation = state.generation.wrapping_add(1);
    }
}

fn on_action_request(
    trigger: On<BinReceive<RegistryActionRequest>>,
    mut state: ResMut<ToolRegistryState>,
    mut queue: ResMut<RegistryActionQueue>,
) {
    let target = trigger.event().webview;
    let request = trigger.event().payload.clone();
    state.subscribers.insert(target, 0);
    queue.0.push_back((target, request));
}

fn start_registry_action(
    mut queue: ResMut<RegistryActionQueue>,
    tasks: Query<(), With<RegistryActionTask>>,
    scans: Query<(), With<RegistryScanTask>>,
    mut commands: Commands,
) {
    if !tasks.is_empty() || !scans.is_empty() {
        return;
    }
    let Some((target, request)) = queue.0.pop_front() else {
        return;
    };
    let task_request = request.clone();
    let task = IoTaskPool::get().spawn(async move { perform_action(&task_request) });
    commands.spawn(RegistryActionTask {
        target,
        request,
        task,
    });
}

fn start_registry_scan(
    mut state: ResMut<ToolRegistryState>,
    tasks: Query<(), With<RegistryScanTask>>,
    action_tasks: Query<(), With<RegistryActionTask>>,
    queue: Res<RegistryActionQueue>,
    mut commands: Commands,
) {
    if !state.dirty || !tasks.is_empty() || !action_tasks.is_empty() || !queue.0.is_empty() {
        return;
    }
    let generation = state.generation;
    let refresh_catalogs = state.refresh_catalogs;
    state.dirty = false;
    state.refresh_catalogs = false;
    let task = IoTaskPool::get().spawn(async move { scan_registry(refresh_catalogs) });
    commands.spawn(RegistryScanTask { generation, task });
}

fn drain_registry_scan(
    mut tasks: Query<(Entity, &mut RegistryScanTask)>,
    mut state: ResMut<ToolRegistryState>,
    mut commands: Commands,
) {
    for (entity, mut task) in &mut tasks {
        let Some(snapshot) = future::block_on(future::poll_once(&mut task.task)) else {
            continue;
        };
        commands.entity(entity).despawn();
        if task.generation != state.generation {
            state.dirty = true;
            continue;
        }
        state.snapshot = snapshot;
        state.loaded = true;
        state.revision = state.revision.wrapping_add(1);
    }
}

fn drain_registry_actions(
    mut tasks: Query<(Entity, &mut RegistryActionTask)>,
    mut state: ResMut<ToolRegistryState>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    for (entity, mut task) in &mut tasks {
        let Some(result) = future::block_on(future::poll_once(&mut task.task)) else {
            continue;
        };
        commands.entity(entity).despawn();
        let (success, message) = match result {
            Ok(message) => (true, message),
            Err(message) => (false, message),
        };
        let event = RegistryActionResult {
            provider: task.request.provider,
            action: task.request.action,
            id: task.request.id.clone(),
            success,
            message,
        };
        if browsers.has_browser(task.target) && browsers.host_emit_ready(&task.target) {
            commands.trigger(BinHostEmitEvent::from_rkyv(
                task.target,
                REGISTRY_ACTION_RESULT_EVENT,
                &event,
            ));
        }
        if success {
            state.dirty = true;
            state.generation = state.generation.wrapping_add(1);
        }
    }
}

fn emit_registry_snapshot(
    mut state: ResMut<ToolRegistryState>,
    browsers: NonSend<Browsers>,
    layout: Query<(Entity, Ref<PageReady>), With<LayoutCef>>,
    mut layout_revision: Local<u64>,
    mut commands: Commands,
) {
    if !state.loaded {
        return;
    }
    if let Ok((entity, page_ready)) = layout.single()
        && (*layout_revision != state.revision || page_ready.is_changed())
        && browsers.has_browser(entity)
        && browsers.host_emit_ready(&entity)
    {
        commands.trigger(BinHostEmitEvent::from_rkyv(
            entity,
            REGISTRY_SNAPSHOT_EVENT,
            &state.snapshot,
        ));
        *layout_revision = state.revision;
    }
    let revision = state.revision;
    let snapshot = state.snapshot.clone();
    state.subscribers.retain(|entity, sent_revision| {
        if !browsers.has_browser(*entity) {
            return false;
        }
        if *sent_revision != revision && browsers.host_emit_ready(entity) {
            commands.trigger(BinHostEmitEvent::from_rkyv(
                *entity,
                REGISTRY_SNAPSHOT_EVENT,
                &snapshot,
            ));
            *sent_revision = revision;
        }
        true
    });
}

fn scan_registry(refresh_catalogs: bool) -> RegistrySnapshot {
    let (manifest, manifest_error) = match manifest_store::load_manifest() {
        Ok(manifest) => (manifest, None),
        Err(error) => (RegistryManifest::default(), Some(error)),
    };
    let mut categories = Vec::new();
    let mut errors = manifest_error.into_iter().collect::<Vec<_>>();
    let providers = [
        (
            RegistryProvider::HomebrewFormula,
            scan_homebrew(false, refresh_catalogs),
        ),
        (
            RegistryProvider::HomebrewCask,
            scan_homebrew(true, refresh_catalogs),
        ),
        (RegistryProvider::Npm, scan_npm(refresh_catalogs)),
        (RegistryProvider::Acp, scan_acp(refresh_catalogs)),
        (RegistryProvider::Lsp, scan_lsp(refresh_catalogs)),
    ];
    for (provider, result) in providers {
        let inventory = match result {
            Ok(inventory) => inventory,
            Err(error) => {
                errors.push(format!("{}: {error}", provider.title()));
                Vec::new()
            }
        };
        categories.push(build_category(provider, inventory, &manifest));
    }
    categories.push(scan_mcp(&manifest, &mut errors));
    categories.push(scan_dotfiles(&manifest));
    let installed = categories
        .iter()
        .flat_map(|category| &category.items)
        .filter(|item| {
            matches!(
                item.status,
                RegistryStatus::Installed | RegistryStatus::Outdated
            )
        })
        .count() as u32;
    let updates = categories
        .iter()
        .flat_map(|category| &category.items)
        .filter(|item| item.status == RegistryStatus::Outdated)
        .count() as u32;
    let conflicts = categories
        .iter()
        .flat_map(|category| &category.items)
        .filter(|item| item.status == RegistryStatus::Conflict)
        .count() as u32;
    RegistrySnapshot {
        root: manifest_store::root_dir().to_string_lossy().into_owned(),
        categories,
        installed,
        updates,
        conflicts,
        error: errors.join("\n"),
    }
}

fn build_category(
    provider: RegistryProvider,
    inventory: Vec<InventoryItem>,
    manifest: &RegistryManifest,
) -> RegistryCategory {
    let mut items = inventory
        .into_iter()
        .map(|item| {
            let managed = manifest.contains(provider.id(), &item.id);
            RegistryItem {
                provider,
                actions: package_actions(item.status, managed, item.removable),
                id: item.id,
                name: item.name,
                version: item.version,
                detail: item.detail,
                status: item.status,
                managed,
            }
        })
        .collect::<Vec<_>>();
    let existing = items
        .iter()
        .map(|item| item.id.clone())
        .collect::<BTreeSet<_>>();
    for name in manifest_store::managed_package_set(manifest, provider.id()) {
        if !existing.contains(&name) {
            items.push(RegistryItem {
                provider,
                id: name.clone(),
                name,
                version: None,
                detail: "Declared in registry.toml".to_string(),
                status: RegistryStatus::Missing,
                managed: true,
                actions: vec![RegistryAction::Install, RegistryAction::Forget],
            });
        }
    }
    items.sort_by(|left, right| {
        left.name
            .to_ascii_lowercase()
            .cmp(&right.name.to_ascii_lowercase())
            .then_with(|| left.name.cmp(&right.name))
    });
    RegistryCategory { provider, items }
}

fn package_actions(status: RegistryStatus, managed: bool, removable: bool) -> Vec<RegistryAction> {
    let mut actions = Vec::new();
    if !managed && matches!(status, RegistryStatus::Installed | RegistryStatus::Outdated) {
        actions.push(RegistryAction::Adopt);
    }
    if status == RegistryStatus::Outdated {
        actions.push(RegistryAction::Update);
    }
    if status == RegistryStatus::Missing {
        actions.push(RegistryAction::Install);
    }
    if removable {
        actions.push(RegistryAction::Uninstall);
    }
    actions
}

fn scan_homebrew(cask: bool, refresh: bool) -> Result<Vec<InventoryItem>, String> {
    if vmux_agent::exec::find_executable("brew").is_none() {
        return Ok(Vec::new());
    }
    let mut args = vec!["list"];
    args.push(if cask { "--cask" } else { "--formula" });
    args.push("--versions");
    let output = command_output("brew", &args, true)?;
    let outdated = if refresh {
        let mut outdated_args = vec!["outdated"];
        outdated_args.push(if cask { "--cask" } else { "--formula" });
        command_output("brew", &outdated_args, false)
            .map(|output| parse_name_lines(&output.stdout))
            .unwrap_or_default()
    } else {
        BTreeSet::new()
    };
    Ok(parse_brew_versions(&output.stdout)
        .into_iter()
        .map(|(name, version)| {
            let status = if outdated.contains(&name) {
                RegistryStatus::Outdated
            } else {
                RegistryStatus::Installed
            };
            let removable = !cask || name != "vmux";
            InventoryItem {
                id: name.clone(),
                name,
                version,
                detail: if cask {
                    "Homebrew cask".to_string()
                } else {
                    "Homebrew formula".to_string()
                },
                status,
                removable,
            }
        })
        .collect())
}

fn parse_brew_versions(bytes: &[u8]) -> Vec<(String, Option<String>)> {
    String::from_utf8_lossy(bytes)
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let name = fields.next()?.to_string();
            let version = fields.collect::<Vec<_>>().join(" ");
            Some((name, (!version.is_empty()).then_some(version)))
        })
        .collect()
}

fn parse_name_lines(bytes: &[u8]) -> BTreeSet<String> {
    String::from_utf8_lossy(bytes)
        .lines()
        .filter_map(|line| line.split_whitespace().next().map(str::to_string))
        .collect()
}

fn scan_npm(refresh: bool) -> Result<Vec<InventoryItem>, String> {
    if vmux_agent::exec::find_executable("npm").is_none() {
        return Ok(Vec::new());
    }
    let output = command_output("npm", &["list", "--global", "--depth=0", "--json"], false)?;
    if output.stdout.is_empty() && !output.status.success() {
        return Err(command_error("npm", &output));
    }
    let outdated_output = refresh
        .then(|| command_output("npm", &["outdated", "--global", "--json"], false).ok())
        .flatten();
    let outdated = outdated_output
        .as_ref()
        .and_then(|output| serde_json::from_slice::<serde_json::Value>(&output.stdout).ok())
        .and_then(|value| {
            value
                .as_object()
                .map(|packages| packages.keys().cloned().collect())
        })
        .unwrap_or_default();
    parse_npm_inventory(&output.stdout, &outdated)
}

fn parse_npm_inventory(
    bytes: &[u8],
    outdated: &BTreeSet<String>,
) -> Result<Vec<InventoryItem>, String> {
    let document: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
    let dependencies = document
        .get("dependencies")
        .and_then(|dependencies| dependencies.as_object())
        .cloned()
        .unwrap_or_default();
    Ok(dependencies
        .into_iter()
        .map(|(name, metadata)| {
            let status = if outdated.contains(&name) {
                RegistryStatus::Outdated
            } else {
                RegistryStatus::Installed
            };
            InventoryItem {
                id: name.clone(),
                name,
                version: metadata
                    .get("version")
                    .and_then(|version| version.as_str())
                    .map(str::to_string),
                detail: "Global npm package".to_string(),
                status,
                removable: true,
            }
        })
        .collect())
}

fn scan_acp(refresh: bool) -> Result<Vec<InventoryItem>, String> {
    let catalog = if refresh {
        vmux_agent::acp_registry::fetch_blocking()
            .ok()
            .or_else(vmux_agent::acp_registry::load_cached)
    } else {
        vmux_agent::acp_registry::load_cached()
    };
    let catalog = catalog
        .map(|registry| {
            registry
                .agents
                .into_iter()
                .map(|agent| (agent.id.clone(), agent))
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    let receipts = vmux_editor::lsp::store::installed(&vmux_agent::acp_registry::agents_dir());
    Ok(receipts
        .into_values()
        .filter(|receipt| receipt.source_id.starts_with("acp:"))
        .map(|receipt| {
            let agent = catalog.get(&receipt.name);
            let latest = agent.and_then(|agent| agent.version.clone());
            InventoryItem {
                id: receipt.name.clone(),
                name: agent
                    .map(|agent| agent.name.clone())
                    .unwrap_or_else(|| receipt.name.clone()),
                version: receipt.version.clone(),
                detail: agent
                    .and_then(|agent| agent.description.clone())
                    .unwrap_or_else(|| "ACP agent".to_string()),
                status: if receipt.version.is_some()
                    && latest.is_some()
                    && receipt.version != latest
                {
                    RegistryStatus::Outdated
                } else {
                    RegistryStatus::Installed
                },
                removable: true,
            }
        })
        .collect())
}

fn scan_lsp(refresh: bool) -> Result<Vec<InventoryItem>, String> {
    let root = vmux_editor::lsp::store::default_root();
    let catalog = if refresh {
        vmux_editor::lsp::catalog::ensure_catalog(&root, true).unwrap_or_default()
    } else if vmux_editor::lsp::catalog::cached_path(&root).is_file() {
        let source = std::fs::read_to_string(vmux_editor::lsp::catalog::cached_path(&root))
            .map_err(|error| error.to_string())?;
        vmux_editor::lsp::catalog::parse_registry(&source).unwrap_or_default()
    } else {
        Vec::new()
    };
    let catalog_by_name = catalog
        .iter()
        .map(|package| (package.name.clone(), package))
        .collect::<BTreeMap<_, _>>();
    let receipts = vmux_editor::lsp::store::installed(&root);
    let mut inventory = receipts
        .into_values()
        .map(|receipt| {
            let package = catalog_by_name.get(&receipt.name).copied();
            let latest = package
                .and_then(|package| vmux_editor::lsp::purl::parse(&package.source_id))
                .and_then(|purl| purl.version);
            InventoryItem {
                id: receipt.name.clone(),
                name: receipt.name.clone(),
                version: receipt.version.clone(),
                detail: package
                    .map(|package| package.description.clone())
                    .filter(|detail| !detail.is_empty())
                    .unwrap_or_else(|| "Vmux-managed language tool".to_string()),
                status: if receipt.version.is_some()
                    && latest.is_some()
                    && receipt.version != latest
                {
                    RegistryStatus::Outdated
                } else {
                    RegistryStatus::Installed
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
        if installed.contains(&package.name) {
            continue;
        }
        let on_path = package.bin.keys().any(|command| {
            matches!(
                vmux_editor::lsp::store::resolved_command(&root, command),
                vmux_editor::lsp::store::Resolution::OnPath
            )
        });
        if on_path {
            inventory.push(InventoryItem {
                id: package.name.clone(),
                name: package.name,
                version: None,
                detail: "Available on PATH".to_string(),
                status: RegistryStatus::Installed,
                removable: false,
            });
        }
    }
    Ok(inventory)
}

fn scan_mcp(manifest: &RegistryManifest, errors: &mut Vec<String>) -> RegistryCategory {
    let (discovered, discovery_errors) = manifest_store::discover_mcp_servers();
    errors.extend(
        discovery_errors
            .into_iter()
            .map(|error| format!("MCP Servers: {error}")),
    );
    let mut names = discovered.keys().cloned().collect::<BTreeSet<_>>();
    names.extend(manifest.mcp.servers.keys().cloned());
    let items = names
        .into_iter()
        .map(|name| {
            let managed = manifest.mcp.servers.contains_key(&name);
            let external = discovered.get(&name);
            let status = if managed {
                RegistryStatus::Installed
            } else if external.is_some_and(|server| server.conflict) {
                RegistryStatus::Conflict
            } else {
                RegistryStatus::Available
            };
            let definition = manifest
                .mcp
                .servers
                .get(&name)
                .or_else(|| external.map(|server| &server.definition));
            let transport = definition
                .map(|server| format!("{:?}", server.transport).to_ascii_lowercase())
                .unwrap_or_else(|| "unknown".to_string());
            let sources = external
                .map(|server| {
                    server
                        .sources
                        .iter()
                        .map(|path| path.to_string_lossy())
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_default();
            let detail = if external.is_some_and(|server| server.conflict) && !managed {
                format!("Conflicting definitions in {sources}")
            } else if managed && sources.is_empty() {
                format!("{transport} · Registry managed")
            } else if managed {
                format!("{transport} · Registry managed · imported from {sources}")
            } else {
                format!("{transport} · configured in {sources}")
            };
            let actions = if managed {
                vec![RegistryAction::Forget]
            } else if status == RegistryStatus::Available {
                vec![RegistryAction::Adopt]
            } else {
                Vec::new()
            };
            RegistryItem {
                provider: RegistryProvider::Mcp,
                id: name.clone(),
                name,
                version: None,
                detail,
                status,
                managed,
                actions,
            }
        })
        .collect();
    RegistryCategory {
        provider: RegistryProvider::Mcp,
        items,
    }
}

fn scan_dotfiles(manifest: &RegistryManifest) -> RegistryCategory {
    let mut package_names = manifest_store::dotfile_packages()
        .into_iter()
        .collect::<BTreeSet<_>>();
    package_names.extend(manifest.dotfiles.packages.iter().cloned());
    let mut items = Vec::new();
    for package in package_names {
        let managed = manifest.dotfiles.packages.contains(&package);
        let (status, detail, actions) = match manifest_store::plan_dotfile_package(&package) {
            Ok(plan) => {
                let detail = format!(
                    "{} linked · {} missing · {} conflicts",
                    plan.linked(),
                    plan.missing(),
                    plan.conflicts()
                );
                let status = if plan.conflicts() > 0 {
                    RegistryStatus::Conflict
                } else if plan.missing() > 0 {
                    if managed {
                        RegistryStatus::Missing
                    } else {
                        RegistryStatus::Available
                    }
                } else {
                    RegistryStatus::Installed
                };
                let actions = if managed {
                    vec![RegistryAction::Link, RegistryAction::Unlink]
                } else {
                    vec![RegistryAction::Link]
                };
                (status, detail, actions)
            }
            Err(error) => (
                RegistryStatus::Missing,
                error,
                if managed {
                    vec![RegistryAction::Unlink]
                } else {
                    Vec::new()
                },
            ),
        };
        items.push(RegistryItem {
            provider: RegistryProvider::Dotfiles,
            id: package.clone(),
            name: package,
            version: None,
            detail,
            status,
            managed,
            actions,
        });
    }
    RegistryCategory {
        provider: RegistryProvider::Dotfiles,
        items,
    }
}

fn perform_action(request: &RegistryActionRequest) -> Result<String, String> {
    if request.action == RegistryAction::Apply {
        return apply_manifest();
    }
    if request.action == RegistryAction::Import {
        return import_provider(request.provider, request.value.trim());
    }
    if request.id.trim().is_empty() {
        return Err("package name is required".to_string());
    }
    match request.action {
        RegistryAction::Install => {
            set_manifest_entry(request.provider, &request.id, true)?;
            install_provider(request.provider, &request.id)?;
            Ok(format!("{} installed", request.id))
        }
        RegistryAction::Update => {
            set_manifest_entry(request.provider, &request.id, true)?;
            update_provider(request.provider, &request.id)?;
            Ok(format!("{} updated", request.id))
        }
        RegistryAction::Uninstall => {
            uninstall_provider(request.provider, &request.id)?;
            set_manifest_entry(request.provider, &request.id, false)?;
            Ok(format!("{} removed", request.id))
        }
        RegistryAction::Forget => {
            set_manifest_entry(request.provider, &request.id, false)?;
            Ok(format!("{} removed from registry.toml", request.id))
        }
        RegistryAction::Adopt => {
            if request.provider == RegistryProvider::Dotfiles {
                if request.value.trim().is_empty() {
                    return Err("dotfile path is required".to_string());
                }
                let destination = manifest_store::adopt_dotfile(
                    Path::new(request.value.trim()),
                    request.id.trim(),
                )?;
                Ok(format!("adopted {}", destination.display()))
            } else if request.provider == RegistryProvider::Mcp {
                manifest_store::import_discovered_mcp_server(&request.id)?;
                Ok(format!("{} is now managed", request.id))
            } else {
                set_manifest_entry(request.provider, &request.id, true)?;
                Ok(format!("{} is now managed", request.id))
            }
        }
        RegistryAction::Link => {
            if request.provider != RegistryProvider::Dotfiles {
                return Err("link is only valid for dotfiles".to_string());
            }
            set_manifest_entry(request.provider, &request.id, true)?;
            let linked = manifest_store::apply_dotfile_package(&request.id)?;
            Ok(format!("linked {linked} file(s)"))
        }
        RegistryAction::Unlink => {
            if request.provider != RegistryProvider::Dotfiles {
                return Err("unlink is only valid for dotfiles".to_string());
            }
            let removed = match manifest_store::unlink_dotfile_package(&request.id) {
                Ok(removed) => removed,
                Err(error) if error.contains("does not exist") => 0,
                Err(error) => return Err(error),
            };
            set_manifest_entry(request.provider, &request.id, false)?;
            Ok(format!("unlinked {removed} file(s)"))
        }
        RegistryAction::Apply | RegistryAction::Import => unreachable!(),
    }
}

fn import_provider(provider: RegistryProvider, path: &str) -> Result<String, String> {
    match provider {
        RegistryProvider::HomebrewFormula | RegistryProvider::HomebrewCask => {
            if !path.is_empty() {
                let (formulae, casks) = manifest_store::import_brewfile(Path::new(path))?;
                Ok(format!("imported {formulae} formulae and {casks} casks"))
            } else {
                let formulae = scan_homebrew(false, false)?;
                let casks = scan_homebrew(true, false)?;
                let mut manifest = manifest_store::load_manifest()?;
                let formulae =
                    import_inventory(&mut manifest, RegistryProvider::HomebrewFormula, formulae);
                let casks = import_inventory(&mut manifest, RegistryProvider::HomebrewCask, casks);
                manifest_store::write_manifest(&manifest)?;
                Ok(format!("imported {formulae} formulae and {casks} casks"))
            }
        }
        RegistryProvider::Npm => {
            if !path.is_empty() {
                let imported = manifest_store::import_npm_manifest(Path::new(path))?;
                Ok(format!("imported {imported} npm package(s)"))
            } else {
                import_scanned_inventory(provider, scan_npm(false)?)
            }
        }
        RegistryProvider::Acp => import_scanned_inventory(provider, scan_acp(false)?),
        RegistryProvider::Lsp => import_scanned_inventory(provider, scan_lsp(false)?),
        RegistryProvider::Mcp => {
            let imported = if path.is_empty() {
                manifest_store::import_default_mcp_configs()?
            } else {
                manifest_store::import_mcp_config(Path::new(path))?
            };
            Ok(format!("imported {imported} MCP server(s)"))
        }
        RegistryProvider::Dotfiles => {
            if path.is_empty() {
                let packages = manifest_store::dotfile_packages();
                let mut manifest = manifest_store::load_manifest()?;
                let mut imported = 0;
                for package in packages {
                    imported += usize::from(!manifest.dotfiles.packages.contains(&package));
                    manifest.set_dotfile_package(&package, true);
                }
                manifest_store::write_manifest(&manifest)?;
                Ok(format!("imported {imported} dotfile package(s)"))
            } else {
                let imported = manifest_store::import_dotfiles(Path::new(path))?;
                Ok(format!("imported {imported} dotfile package(s)"))
            }
        }
    }
}

fn import_scanned_inventory(
    provider: RegistryProvider,
    inventory: Vec<InventoryItem>,
) -> Result<String, String> {
    let mut manifest = manifest_store::load_manifest()?;
    let imported = import_inventory(&mut manifest, provider, inventory);
    manifest_store::write_manifest(&manifest)?;
    Ok(format!("imported {imported} {} item(s)", provider.id()))
}

fn import_inventory(
    manifest: &mut RegistryManifest,
    provider: RegistryProvider,
    inventory: Vec<InventoryItem>,
) -> usize {
    let mut imported = 0;
    for item in inventory.into_iter().filter(|item| {
        matches!(
            item.status,
            RegistryStatus::Installed | RegistryStatus::Outdated
        )
    }) {
        imported += usize::from(!manifest.contains(provider.id(), &item.id));
        manifest.set_package(provider.id(), &item.id, true);
    }
    imported
}

fn apply_manifest() -> Result<String, String> {
    let manifest = manifest_store::load_manifest()?;
    let snapshot = scan_registry(false);
    let mut installed = 0;
    for item in snapshot
        .categories
        .iter()
        .flat_map(|category| &category.items)
        .filter(|item| item.managed && item.status == RegistryStatus::Missing)
        .filter(|item| {
            !matches!(
                item.provider,
                RegistryProvider::Dotfiles | RegistryProvider::Mcp
            )
        })
    {
        install_provider(item.provider, &item.id)?;
        installed += 1;
    }
    let linked = manifest_store::apply_enabled_dotfiles(&manifest)?;
    Ok(format!(
        "installed {installed} package(s), linked {linked} file(s)"
    ))
}

fn set_manifest_entry(provider: RegistryProvider, id: &str, enabled: bool) -> Result<(), String> {
    let mut manifest = manifest_store::load_manifest()?;
    if provider == RegistryProvider::Dotfiles {
        manifest.set_dotfile_package(id, enabled);
    } else if provider == RegistryProvider::Mcp {
        if enabled {
            return Err("MCP servers must be imported from a config".to_string());
        }
        manifest.mcp.servers.remove(id);
    } else {
        manifest.set_package(provider.id(), id, enabled);
    }
    manifest_store::write_manifest(&manifest)
}

fn install_provider(provider: RegistryProvider, id: &str) -> Result<(), String> {
    match provider {
        RegistryProvider::HomebrewFormula => {
            command_output("brew", &["install", id], true)?;
        }
        RegistryProvider::HomebrewCask => {
            command_output("brew", &["install", "--cask", id], true)?;
        }
        RegistryProvider::Npm => {
            command_output("npm", &["install", "--global", id], true)?;
        }
        RegistryProvider::Acp => {
            vmux_agent::acp_install::resolve_from_registry(id, |_, _, _| {})?;
        }
        RegistryProvider::Lsp => {
            let root = vmux_editor::lsp::store::default_root();
            let packages = vmux_editor::lsp::catalog::ensure_catalog(&root, false)?;
            let package = packages
                .iter()
                .find(|package| package.name == id)
                .ok_or_else(|| format!("language tool not found: {id}"))?;
            vmux_editor::lsp::install::install(
                package,
                &root,
                vmux_editor::lsp::target::host_target(),
                |_, _, _| {},
            )?;
        }
        RegistryProvider::Dotfiles => {
            manifest_store::apply_dotfile_package(id)?;
        }
        RegistryProvider::Mcp => return Err("MCP servers are configuration, not packages".into()),
    }
    Ok(())
}

fn uninstall_provider(provider: RegistryProvider, id: &str) -> Result<(), String> {
    match provider {
        RegistryProvider::HomebrewFormula => {
            command_output("brew", &["uninstall", id], true)?;
        }
        RegistryProvider::HomebrewCask => {
            command_output("brew", &["uninstall", "--cask", id], true)?;
        }
        RegistryProvider::Npm => {
            command_output("npm", &["uninstall", "--global", id], true)?;
        }
        RegistryProvider::Acp => vmux_agent::acp_install::uninstall(id)?,
        RegistryProvider::Lsp => {
            vmux_editor::lsp::store::remove(&vmux_editor::lsp::store::default_root(), id)
                .map_err(|error| error.to_string())?;
        }
        RegistryProvider::Dotfiles => {
            manifest_store::unlink_dotfile_package(id)?;
        }
        RegistryProvider::Mcp => return Err("forget the MCP server instead".to_string()),
    }
    Ok(())
}

fn update_provider(provider: RegistryProvider, id: &str) -> Result<(), String> {
    match provider {
        RegistryProvider::HomebrewFormula => {
            command_output("brew", &["upgrade", id], true)?;
        }
        RegistryProvider::HomebrewCask => {
            command_output("brew", &["upgrade", "--cask", id], true)?;
        }
        RegistryProvider::Npm => {
            command_output("npm", &["update", "--global", id], true)?;
        }
        RegistryProvider::Acp | RegistryProvider::Lsp | RegistryProvider::Dotfiles => {
            install_provider(provider, id)?;
        }
        RegistryProvider::Mcp => return Err("MCP servers do not update through Registry".into()),
    }
    Ok(())
}

fn command_output(program: &str, args: &[&str], require_success: bool) -> Result<Output, String> {
    let executable = vmux_agent::exec::find_executable(program)
        .ok_or_else(|| format!("{program} is not installed"))?;
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
    let output = Command::new(executable)
        .args(args)
        .envs(
            vmux_terminal::shell_env::login_shell_env(&shell)
                .iter()
                .cloned(),
        )
        .output()
        .map_err(|error| error.to_string())?;
    if require_success && !output.status.success() {
        return Err(command_error(program, &output));
    }
    Ok(output)
}

fn command_error(program: &str, output: &Output) -> String {
    let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if detail.is_empty() {
        format!("{program} exited with {}", output.status)
    } else {
        detail
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_brew_inventory_with_versions() {
        assert_eq!(
            parse_brew_versions(b"ripgrep 14.1.1\nopenssl@3 3.5.0 3.5.1\n"),
            vec![
                ("ripgrep".to_string(), Some("14.1.1".to_string())),
                ("openssl@3".to_string(), Some("3.5.0 3.5.1".to_string())),
            ]
        );
    }

    #[test]
    fn category_adds_declared_missing_packages() {
        let mut manifest = RegistryManifest::default();
        manifest.set_package(RegistryProvider::Npm.id(), "typescript", true);
        let category = build_category(RegistryProvider::Npm, Vec::new(), &manifest);
        assert_eq!(category.items.len(), 1);
        assert_eq!(category.items[0].status, RegistryStatus::Missing);
        assert!(category.items[0].managed);
        assert_eq!(
            category.items[0].actions,
            [RegistryAction::Install, RegistryAction::Forget]
        );
    }

    #[test]
    fn parses_scoped_npm_packages_and_outdated_state() {
        let inventory = parse_npm_inventory(
            br#"{"dependencies":{"@scope/tool":{"version":"2.0.0"},"typescript":{"version":"5.9.0"}}}"#,
            &BTreeSet::from(["@scope/tool".to_string()]),
        )
        .unwrap();
        assert_eq!(inventory.len(), 2);
        let scoped = inventory
            .iter()
            .find(|item| item.id == "@scope/tool")
            .unwrap();
        assert_eq!(scoped.version.as_deref(), Some("2.0.0"));
        assert_eq!(scoped.status, RegistryStatus::Outdated);
    }

    #[test]
    fn bulk_import_adopts_only_installed_inventory() {
        let mut manifest = RegistryManifest::default();
        let imported = import_inventory(
            &mut manifest,
            RegistryProvider::Npm,
            vec![
                InventoryItem {
                    id: "installed".to_string(),
                    name: "installed".to_string(),
                    version: Some("1".to_string()),
                    detail: String::new(),
                    status: RegistryStatus::Installed,
                    removable: true,
                },
                InventoryItem {
                    id: "missing".to_string(),
                    name: "missing".to_string(),
                    version: None,
                    detail: String::new(),
                    status: RegistryStatus::Missing,
                    removable: true,
                },
            ],
        );

        assert_eq!(imported, 1);
        assert!(manifest.contains("npm", "installed"));
        assert!(!manifest.contains("npm", "missing"));
    }

    #[test]
    fn unmanaged_installed_packages_can_be_adopted() {
        assert_eq!(
            package_actions(RegistryStatus::Installed, false, true),
            [RegistryAction::Adopt, RegistryAction::Uninstall]
        );
        assert_eq!(
            package_actions(RegistryStatus::Outdated, true, true),
            [RegistryAction::Update, RegistryAction::Uninstall]
        );
    }
}
