use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::path::Path;
use std::process::{Command, Output};

use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use bevy_cef::prelude::{BinEventEmitterPlugin, BinHostEmitEvent, BinReceive, Browsers};
use vmux_command::{AppCommand, BrowserCommand, open::OpenCommand};
use vmux_core::page::{PageManifest, PageReady, PrewarmPage};
use vmux_core::profile::tools::{self as manifest_store, ToolsManifest};
use vmux_core::tools::{
    TOOL_ACTION_RESULT_EVENT, TOOLS_SNAPSHOT_EVENT, ToolAction, ToolActionRequest,
    ToolActionResult, ToolCategory, ToolItem, ToolOpenRequest, ToolProvider, ToolStatus,
    ToolsRefreshRequest, ToolsSnapshot,
};
use vmux_core::vault::{
    VAULT_ACTION_RESULT_EVENT, VaultAction, VaultActionRequest, VaultActionResult,
    VaultRefreshRequest, VaultRepository, VaultSnapshot,
};
use vmux_layout::LayoutCef;

const PAGE_MANIFEST: PageManifest = PageManifest {
    host: "tools",
    title: "Tools",
    keywords: &[
        "packages", "tools", "dotfiles", "homebrew", "npm", "mcp", "import",
    ],
    icon: Some(vmux_core::BuiltinIcon::Hammer),
    command_bar: true,
};

const VAULT_PAGE_MANIFEST: PageManifest = PageManifest {
    host: "vault",
    title: "Vault",
    keywords: &["vault", "sync", "git", "backup", "dotfiles", "knowledge"],
    icon: Some(vmux_core::BuiltinIcon::Vault),
    command_bar: true,
};

pub struct ToolsPlugin;

#[derive(Resource)]
struct ToolsState {
    dirty: bool,
    refresh_catalogs: bool,
    load_vault_repositories: bool,
    generation: u64,
    revision: u64,
    loaded: bool,
    snapshot: ToolsSnapshot,
    subscribers: HashMap<Entity, u64>,
}

impl Default for ToolsState {
    fn default() -> Self {
        Self {
            dirty: true,
            refresh_catalogs: false,
            load_vault_repositories: false,
            generation: 1,
            revision: 0,
            loaded: false,
            snapshot: ToolsSnapshot::default(),
            subscribers: HashMap::new(),
        }
    }
}

#[derive(Component)]
struct ToolsScanTask {
    generation: u64,
    task: Task<ToolsSnapshot>,
}

#[derive(Component)]
struct ToolActionTask {
    target: Entity,
    request: ToolActionRequest,
    task: Task<Result<String, String>>,
}

#[derive(Resource, Default)]
struct ToolActionQueue(VecDeque<(Entity, ToolActionRequest)>);

#[derive(Component)]
struct VaultActionTask {
    target: Entity,
    request: VaultActionRequest,
    task: Task<Result<String, String>>,
}

#[derive(Resource, Default)]
struct VaultActionQueue(VecDeque<(Entity, VaultActionRequest)>);

#[derive(Clone, Debug)]
struct InventoryItem {
    id: String,
    name: String,
    version: Option<String>,
    detail: String,
    status: ToolStatus,
    removable: bool,
}

impl Plugin for ToolsPlugin {
    fn build(&self, app: &mut App) {
        app.world_mut().spawn((
            PAGE_MANIFEST,
            PrewarmPage {
                host: "tools",
                url: "vmux://tools/",
                title: "Tools",
                pool_size: 1,
            },
        ));
        app.world_mut().spawn((
            VAULT_PAGE_MANIFEST,
            PrewarmPage {
                host: "vault",
                url: "vmux://vault/",
                title: "Vault",
                pool_size: 1,
            },
        ));
        vmux_core::register_host_spawn(app, "tools");
        vmux_core::register_host_spawn(app, "vault");
        app.init_resource::<ToolsState>()
            .init_resource::<ToolActionQueue>()
            .init_resource::<VaultActionQueue>()
            .add_plugins(BinEventEmitterPlugin::<(
                ToolsRefreshRequest,
                ToolActionRequest,
                ToolOpenRequest,
                VaultActionRequest,
                VaultRefreshRequest,
            )>::default())
            .add_observer(on_refresh_request)
            .add_observer(on_action_request)
            .add_observer(on_vault_action_request)
            .add_observer(on_vault_refresh_request)
            .add_observer(on_open_request)
            .add_systems(
                Update,
                (
                    start_tools_scan,
                    drain_tools_scan,
                    start_tool_action,
                    drain_tool_actions,
                    start_vault_action,
                    drain_vault_actions,
                    emit_tools_snapshot,
                )
                    .chain(),
            );
    }
}

fn on_open_request(
    trigger: On<BinReceive<ToolOpenRequest>>,
    mut commands: MessageWriter<AppCommand>,
) {
    let path = Path::new(trigger.event().payload.path.trim());
    if path == manifest_store::brewfile_path() && !path.exists() {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(path, "");
    }
    let Ok(url) = url::Url::from_file_path(path) else {
        return;
    };
    commands.write(AppCommand::Browser(BrowserCommand::Open(
        OpenCommand::InNewStack {
            url: Some(url.to_string()),
        },
    )));
}

fn on_refresh_request(trigger: On<BinReceive<ToolsRefreshRequest>>, mut state: ResMut<ToolsState>) {
    let request = &trigger.event().payload;
    state.subscribers.insert(trigger.event().webview, 0);
    if request.refresh || !state.loaded {
        state.dirty = true;
        state.refresh_catalogs |= request.refresh;
        state.generation = state.generation.wrapping_add(1);
    }
}

fn on_action_request(
    trigger: On<BinReceive<ToolActionRequest>>,
    mut state: ResMut<ToolsState>,
    mut queue: ResMut<ToolActionQueue>,
) {
    let target = trigger.event().webview;
    let request = trigger.event().payload.clone();
    state.subscribers.insert(target, 0);
    queue.0.push_back((target, request));
}

fn on_vault_action_request(
    trigger: On<BinReceive<VaultActionRequest>>,
    mut state: ResMut<ToolsState>,
    mut queue: ResMut<VaultActionQueue>,
) {
    let target = trigger.event().webview;
    state.subscribers.insert(target, 0);
    queue.0.push_back((target, trigger.event().payload.clone()));
}

fn on_vault_refresh_request(
    trigger: On<BinReceive<VaultRefreshRequest>>,
    mut state: ResMut<ToolsState>,
) {
    state.subscribers.insert(trigger.event().webview, 0);
    state.dirty = true;
    state.load_vault_repositories = true;
    state.generation = state.generation.wrapping_add(1);
}

fn start_tool_action(
    mut queue: ResMut<ToolActionQueue>,
    tasks: Query<(), With<ToolActionTask>>,
    vault_tasks: Query<(), With<VaultActionTask>>,
    scans: Query<(), With<ToolsScanTask>>,
    mut commands: Commands,
) {
    if !tasks.is_empty() || !vault_tasks.is_empty() || !scans.is_empty() {
        return;
    }
    let Some((target, request)) = queue.0.pop_front() else {
        return;
    };
    let task_request = request.clone();
    let task = IoTaskPool::get().spawn(async move { perform_action(&task_request) });
    commands.spawn(ToolActionTask {
        target,
        request,
        task,
    });
}

fn start_vault_action(
    mut queue: ResMut<VaultActionQueue>,
    tasks: Query<(), With<VaultActionTask>>,
    tool_tasks: Query<(), With<ToolActionTask>>,
    scans: Query<(), With<ToolsScanTask>>,
    mut commands: Commands,
) {
    if !tasks.is_empty() || !tool_tasks.is_empty() || !scans.is_empty() {
        return;
    }
    let Some((target, request)) = queue.0.pop_front() else {
        return;
    };
    let task_request = request.clone();
    let task = IoTaskPool::get().spawn(async move { perform_vault_action(&task_request) });
    commands.spawn(VaultActionTask {
        target,
        request,
        task,
    });
}

fn start_tools_scan(
    mut state: ResMut<ToolsState>,
    tasks: Query<(), With<ToolsScanTask>>,
    action_tasks: Query<(), With<ToolActionTask>>,
    vault_tasks: Query<(), With<VaultActionTask>>,
    queue: Res<ToolActionQueue>,
    vault_queue: Res<VaultActionQueue>,
    mut commands: Commands,
) {
    if !state.dirty
        || !tasks.is_empty()
        || !action_tasks.is_empty()
        || !vault_tasks.is_empty()
        || !queue.0.is_empty()
        || !vault_queue.0.is_empty()
    {
        return;
    }
    let generation = state.generation;
    let refresh_catalogs = state.refresh_catalogs;
    let load_vault_repositories = state.load_vault_repositories;
    let previous_vault = state.snapshot.vault.clone();
    state.dirty = false;
    state.refresh_catalogs = false;
    state.load_vault_repositories = false;
    let task = IoTaskPool::get().spawn(async move {
        scan_tools(refresh_catalogs, load_vault_repositories, previous_vault)
    });
    commands.spawn(ToolsScanTask { generation, task });
}

fn drain_tools_scan(
    mut tasks: Query<(Entity, &mut ToolsScanTask)>,
    mut state: ResMut<ToolsState>,
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

fn drain_tool_actions(
    mut tasks: Query<(Entity, &mut ToolActionTask)>,
    mut state: ResMut<ToolsState>,
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
        let event = ToolActionResult {
            provider: task.request.provider,
            action: task.request.action,
            id: task.request.id.clone(),
            success,
            message,
        };
        if browsers.has_browser(task.target) && browsers.host_emit_ready(&task.target) {
            commands.trigger(BinHostEmitEvent::from_rkyv(
                task.target,
                TOOL_ACTION_RESULT_EVENT,
                &event,
            ));
        }
        if success {
            state.dirty = true;
            state.generation = state.generation.wrapping_add(1);
        }
    }
}

fn drain_vault_actions(
    mut tasks: Query<(Entity, &mut VaultActionTask)>,
    mut state: ResMut<ToolsState>,
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
        let event = VaultActionResult {
            action: task.request.action,
            success,
            message,
        };
        if browsers.has_browser(task.target) && browsers.host_emit_ready(&task.target) {
            commands.trigger(BinHostEmitEvent::from_rkyv(
                task.target,
                VAULT_ACTION_RESULT_EVENT,
                &event,
            ));
        }
        state.dirty = true;
        state.load_vault_repositories = true;
        state.generation = state.generation.wrapping_add(1);
    }
}

fn emit_tools_snapshot(
    mut state: ResMut<ToolsState>,
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
            TOOLS_SNAPSHOT_EVENT,
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
                TOOLS_SNAPSHOT_EVENT,
                &snapshot,
            ));
            *sent_revision = revision;
        }
        true
    });
}

fn scan_tools(
    refresh_catalogs: bool,
    load_vault_repositories: bool,
    previous_vault: VaultSnapshot,
) -> ToolsSnapshot {
    let (mut manifest, manifest_error) = match manifest_store::load_manifest() {
        Ok(manifest) => (manifest, None),
        Err(error) => (ToolsManifest::default(), Some(error)),
    };
    let can_persist = manifest_error.is_none();
    let original_manifest = manifest.clone();
    let mut categories = Vec::new();
    let mut errors = manifest_error.into_iter().collect::<Vec<_>>();
    let providers = [
        (
            ToolProvider::HomebrewFormula,
            scan_homebrew(false, refresh_catalogs),
        ),
        (
            ToolProvider::HomebrewCask,
            scan_homebrew(true, refresh_catalogs),
        ),
        (ToolProvider::Npm, scan_npm(refresh_catalogs)),
        (ToolProvider::Acp, scan_acp(refresh_catalogs)),
        (ToolProvider::Lsp, scan_lsp(refresh_catalogs)),
    ];
    let mut inventories = Vec::new();
    for (provider, result) in providers {
        let inventory = match result {
            Ok(inventory) => inventory,
            Err(error) => {
                errors.push(format!("{}: {error}", provider.title()));
                Vec::new()
            }
        };
        import_inventory(&mut manifest, provider, inventory.clone());
        inventories.push((provider, inventory));
    }
    categories.extend(
        inventories
            .into_iter()
            .map(|(provider, inventory)| build_category(provider, inventory, &manifest)),
    );
    categories.push(scan_mcp(&mut manifest, &mut errors));
    categories.push(scan_dotfiles(&mut manifest));
    if can_persist
        && manifest != original_manifest
        && let Err(error) = manifest_store::write_manifest(&manifest)
    {
        errors.push(format!("Tools: {error}"));
    }
    let installed = categories
        .iter()
        .flat_map(|category| &category.items)
        .filter(|item| matches!(item.status, ToolStatus::Installed | ToolStatus::Outdated))
        .count() as u32;
    let updates = categories
        .iter()
        .flat_map(|category| &category.items)
        .filter(|item| item.status == ToolStatus::Outdated)
        .count() as u32;
    let conflicts = categories
        .iter()
        .flat_map(|category| &category.items)
        .filter(|item| item.status == ToolStatus::Conflict)
        .count() as u32;
    ToolsSnapshot {
        root: manifest_store::root_dir().to_string_lossy().into_owned(),
        vault: scan_vault(load_vault_repositories, previous_vault),
        categories,
        installed,
        updates,
        conflicts,
        error: errors.join("\n"),
    }
}

fn scan_vault(load_repositories: bool, previous: VaultSnapshot) -> VaultSnapshot {
    let status = if load_repositories {
        vmux_core::profile::vault::status_with_repositories()
    } else {
        vmux_core::profile::vault::status()
    };
    let mut snapshot = VaultSnapshot {
        root: status.root.to_string_lossy().into_owned(),
        initialized: status.initialized,
        remote: status.remote,
        branch: status.branch,
        dirty: status.dirty,
        ahead: status.ahead,
        behind: status.behind,
        github_owner: status.github_owner,
        repositories: status
            .repositories
            .into_iter()
            .map(|repository| VaultRepository {
                name: repository.name,
                url: repository.url,
                private: repository.private,
                empty: repository.empty,
            })
            .collect(),
        error: status.error,
    };
    if !load_repositories && (!snapshot.initialized || snapshot.remote.is_empty()) {
        snapshot.github_owner = previous.github_owner;
        snapshot.repositories = previous.repositories;
        snapshot.error = previous.error;
    }
    snapshot
}

fn build_category(
    provider: ToolProvider,
    inventory: Vec<InventoryItem>,
    manifest: &ToolsManifest,
) -> ToolCategory {
    let mut items = inventory
        .into_iter()
        .map(|item| {
            let managed = manifest.contains(provider.id(), &item.id);
            ToolItem {
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
            items.push(ToolItem {
                provider,
                id: name.clone(),
                name,
                version: None,
                detail: "Declared in tools.toml".to_string(),
                status: ToolStatus::Missing,
                managed: true,
                actions: vec![ToolAction::Install, ToolAction::Forget],
            });
        }
    }
    items.sort_by(|left, right| {
        left.name
            .to_ascii_lowercase()
            .cmp(&right.name.to_ascii_lowercase())
            .then_with(|| left.name.cmp(&right.name))
    });
    ToolCategory { provider, items }
}

fn package_actions(status: ToolStatus, managed: bool, removable: bool) -> Vec<ToolAction> {
    let mut actions = Vec::new();
    if !managed && matches!(status, ToolStatus::Installed | ToolStatus::Outdated) {
        actions.push(ToolAction::Adopt);
    }
    if status == ToolStatus::Outdated {
        actions.push(ToolAction::Update);
    }
    if status == ToolStatus::Missing {
        actions.push(ToolAction::Install);
    }
    if removable {
        actions.push(ToolAction::Uninstall);
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
                ToolStatus::Outdated
            } else {
                ToolStatus::Installed
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
                ToolStatus::Outdated
            } else {
                ToolStatus::Installed
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
                    ToolStatus::Outdated
                } else {
                    ToolStatus::Installed
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
                status: ToolStatus::Installed,
                removable: false,
            });
        }
    }
    Ok(inventory)
}

fn scan_mcp(manifest: &mut ToolsManifest, errors: &mut Vec<String>) -> ToolCategory {
    let (discovered, discovery_errors) = manifest_store::discover_mcp_servers();
    errors.extend(
        discovery_errors
            .into_iter()
            .map(|error| format!("MCP Servers: {error}")),
    );
    for (name, server) in &discovered {
        if name != "vmux" && !server.conflict {
            manifest
                .mcp
                .servers
                .entry(name.clone())
                .or_insert_with(|| server.definition.clone());
        }
    }
    let mut names = discovered.keys().cloned().collect::<BTreeSet<_>>();
    names.extend(manifest.mcp.servers.keys().cloned());
    let items = names
        .into_iter()
        .map(|name| {
            let managed = manifest.mcp.servers.contains_key(&name);
            let external = discovered.get(&name);
            let status = if managed {
                ToolStatus::Installed
            } else if external.is_some_and(|server| server.conflict) {
                ToolStatus::Conflict
            } else {
                ToolStatus::Available
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
                format!("{transport} · Tools managed")
            } else if managed {
                format!("{transport} · Tools managed · imported from {sources}")
            } else {
                format!("{transport} · configured in {sources}")
            };
            let actions = if managed {
                vec![ToolAction::Forget]
            } else if status == ToolStatus::Available {
                vec![ToolAction::Adopt]
            } else {
                Vec::new()
            };
            ToolItem {
                provider: ToolProvider::Mcp,
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
    ToolCategory {
        provider: ToolProvider::Mcp,
        items,
    }
}

fn scan_dotfiles(manifest: &mut ToolsManifest) -> ToolCategory {
    let discovered = manifest_store::dotfile_packages().unwrap_or_default();
    for package in &discovered {
        manifest.set_dotfile_package(package, true);
    }
    let mut package_names = discovered.into_iter().collect::<BTreeSet<_>>();
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
                    ToolStatus::Conflict
                } else if plan.missing() > 0 {
                    if managed {
                        ToolStatus::Missing
                    } else {
                        ToolStatus::Available
                    }
                } else {
                    ToolStatus::Installed
                };
                let actions = if managed {
                    vec![ToolAction::Link, ToolAction::Unlink]
                } else {
                    vec![ToolAction::Link]
                };
                (status, detail, actions)
            }
            Err(error) => (
                ToolStatus::Missing,
                error,
                if managed {
                    vec![ToolAction::Unlink]
                } else {
                    Vec::new()
                },
            ),
        };
        items.push(ToolItem {
            provider: ToolProvider::Dotfiles,
            id: package.clone(),
            name: package,
            version: None,
            detail,
            status,
            managed,
            actions,
        });
    }
    ToolCategory {
        provider: ToolProvider::Dotfiles,
        items,
    }
}

fn perform_action(request: &ToolActionRequest) -> Result<String, String> {
    if request.action == ToolAction::Apply {
        return apply_manifest();
    }
    if request.action == ToolAction::Import {
        return import_provider(request.provider, request.value.trim());
    }
    if request.id.trim().is_empty() {
        return Err("package name is required".to_string());
    }
    match request.action {
        ToolAction::Install => {
            set_manifest_entry(request.provider, &request.id, true)?;
            install_provider(request.provider, &request.id)?;
            Ok(format!("{} installed", request.id))
        }
        ToolAction::Update => {
            set_manifest_entry(request.provider, &request.id, true)?;
            update_provider(request.provider, &request.id)?;
            Ok(format!("{} updated", request.id))
        }
        ToolAction::Uninstall => {
            uninstall_provider(request.provider, &request.id)?;
            set_manifest_entry(request.provider, &request.id, false)?;
            Ok(format!("{} removed", request.id))
        }
        ToolAction::Forget => {
            set_manifest_entry(request.provider, &request.id, false)?;
            Ok(format!("{} removed from tools.toml", request.id))
        }
        ToolAction::Adopt => {
            if request.provider == ToolProvider::Dotfiles {
                if request.value.trim().is_empty() {
                    return Err("dotfile path is required".to_string());
                }
                let destination = manifest_store::adopt_dotfile(
                    Path::new(request.value.trim()),
                    request.id.trim(),
                )?;
                Ok(format!("adopted {}", destination.display()))
            } else if request.provider == ToolProvider::Mcp {
                manifest_store::import_discovered_mcp_server(&request.id)?;
                Ok(format!("{} is now managed", request.id))
            } else {
                set_manifest_entry(request.provider, &request.id, true)?;
                Ok(format!("{} is now managed", request.id))
            }
        }
        ToolAction::Link => {
            if request.provider != ToolProvider::Dotfiles {
                return Err("link is only valid for dotfiles".to_string());
            }
            set_manifest_entry(request.provider, &request.id, true)?;
            let linked = manifest_store::apply_dotfile_package(&request.id)?;
            Ok(format!("linked {linked} file(s)"))
        }
        ToolAction::Unlink => {
            if request.provider != ToolProvider::Dotfiles {
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
        ToolAction::Apply | ToolAction::Import => unreachable!(),
    }
}

fn perform_vault_action(request: &VaultActionRequest) -> Result<String, String> {
    match request.action {
        VaultAction::Create => vmux_core::profile::vault::create_remote(
            &request.repository,
            if request.private {
                vmux_core::profile::vault::RepositoryVisibility::Private
            } else {
                vmux_core::profile::vault::RepositoryVisibility::Public
            },
        ),
        VaultAction::Connect => vmux_core::profile::vault::connect_remote(&request.repository),
        VaultAction::Sync => vmux_core::profile::vault::sync(),
    }
}

fn import_provider(provider: ToolProvider, path: &str) -> Result<String, String> {
    match provider {
        ToolProvider::HomebrewFormula | ToolProvider::HomebrewCask => {
            if !path.is_empty() {
                let (formulae, casks) = manifest_store::import_brewfile(Path::new(path))?;
                Ok(format!("imported {formulae} formulae and {casks} casks"))
            } else {
                let formulae = scan_homebrew(false, false)?;
                let casks = scan_homebrew(true, false)?;
                let mut manifest = manifest_store::load_manifest()?;
                let formulae =
                    import_inventory(&mut manifest, ToolProvider::HomebrewFormula, formulae);
                let casks = import_inventory(&mut manifest, ToolProvider::HomebrewCask, casks);
                manifest_store::write_manifest(&manifest)?;
                Ok(format!("imported {formulae} formulae and {casks} casks"))
            }
        }
        ToolProvider::Npm => {
            if !path.is_empty() {
                let imported = manifest_store::import_npm_manifest(Path::new(path))?;
                Ok(format!("imported {imported} npm package(s)"))
            } else {
                import_scanned_inventory(provider, scan_npm(false)?)
            }
        }
        ToolProvider::Acp => import_scanned_inventory(provider, scan_acp(false)?),
        ToolProvider::Lsp => import_scanned_inventory(provider, scan_lsp(false)?),
        ToolProvider::Mcp => {
            let imported = if path.is_empty() {
                manifest_store::import_default_mcp_configs()?
            } else {
                manifest_store::import_mcp_config(Path::new(path))?
            };
            Ok(format!("imported {imported} MCP server(s)"))
        }
        ToolProvider::Dotfiles => {
            if path.is_empty() {
                let packages = manifest_store::dotfile_packages()?;
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
    provider: ToolProvider,
    inventory: Vec<InventoryItem>,
) -> Result<String, String> {
    let mut manifest = manifest_store::load_manifest()?;
    let imported = import_inventory(&mut manifest, provider, inventory);
    manifest_store::write_manifest(&manifest)?;
    Ok(format!("imported {imported} {} item(s)", provider.id()))
}

fn import_inventory(
    manifest: &mut ToolsManifest,
    provider: ToolProvider,
    inventory: Vec<InventoryItem>,
) -> usize {
    let mut imported = 0;
    for item in inventory
        .into_iter()
        .filter(|item| matches!(item.status, ToolStatus::Installed | ToolStatus::Outdated))
    {
        imported += usize::from(!manifest.contains(provider.id(), &item.id));
        manifest.set_package(provider.id(), &item.id, true);
    }
    imported
}

fn apply_manifest() -> Result<String, String> {
    let manifest = manifest_store::load_manifest()?;
    let snapshot = scan_tools(false, false, VaultSnapshot::default());
    let mut installed = 0;
    for item in snapshot
        .categories
        .iter()
        .flat_map(|category| &category.items)
        .filter(|item| item.managed && item.status == ToolStatus::Missing)
        .filter(|item| !matches!(item.provider, ToolProvider::Dotfiles | ToolProvider::Mcp))
    {
        install_provider(item.provider, &item.id)?;
        installed += 1;
    }
    let linked = manifest_store::apply_enabled_dotfiles(&manifest)?;
    Ok(format!(
        "installed {installed} package(s), linked {linked} file(s)"
    ))
}

fn set_manifest_entry(provider: ToolProvider, id: &str, enabled: bool) -> Result<(), String> {
    let mut manifest = manifest_store::load_manifest()?;
    if provider == ToolProvider::Dotfiles {
        manifest.set_dotfile_package(id, enabled);
    } else if provider == ToolProvider::Mcp {
        if enabled {
            return Err("MCP servers must be imported from a config".to_string());
        }
        manifest.mcp.servers.remove(id);
    } else {
        manifest.set_package(provider.id(), id, enabled);
    }
    manifest_store::write_manifest(&manifest)
}

fn install_provider(provider: ToolProvider, id: &str) -> Result<(), String> {
    match provider {
        ToolProvider::HomebrewFormula => {
            command_output("brew", &["install", id], true)?;
        }
        ToolProvider::HomebrewCask => {
            command_output("brew", &["install", "--cask", id], true)?;
        }
        ToolProvider::Npm => {
            command_output("npm", &["install", "--global", id], true)?;
        }
        ToolProvider::Acp => {
            vmux_agent::acp_install::resolve_from_registry(id, |_, _, _| {})?;
        }
        ToolProvider::Lsp => {
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
        ToolProvider::Dotfiles => {
            manifest_store::apply_dotfile_package(id)?;
        }
        ToolProvider::Mcp => return Err("MCP servers are configuration, not packages".into()),
    }
    Ok(())
}

fn uninstall_provider(provider: ToolProvider, id: &str) -> Result<(), String> {
    match provider {
        ToolProvider::HomebrewFormula => {
            command_output("brew", &["uninstall", id], true)?;
        }
        ToolProvider::HomebrewCask => {
            command_output("brew", &["uninstall", "--cask", id], true)?;
        }
        ToolProvider::Npm => {
            command_output("npm", &["uninstall", "--global", id], true)?;
        }
        ToolProvider::Acp => vmux_agent::acp_install::uninstall(id)?,
        ToolProvider::Lsp => {
            vmux_editor::lsp::store::remove(&vmux_editor::lsp::store::default_root(), id)
                .map_err(|error| error.to_string())?;
        }
        ToolProvider::Dotfiles => {
            manifest_store::unlink_dotfile_package(id)?;
        }
        ToolProvider::Mcp => return Err("forget the MCP server instead".to_string()),
    }
    Ok(())
}

fn update_provider(provider: ToolProvider, id: &str) -> Result<(), String> {
    match provider {
        ToolProvider::HomebrewFormula => {
            command_output("brew", &["upgrade", id], true)?;
        }
        ToolProvider::HomebrewCask => {
            command_output("brew", &["upgrade", "--cask", id], true)?;
        }
        ToolProvider::Npm => {
            command_output("npm", &["update", "--global", id], true)?;
        }
        ToolProvider::Acp | ToolProvider::Lsp | ToolProvider::Dotfiles => {
            install_provider(provider, id)?;
        }
        ToolProvider::Mcp => return Err("MCP servers do not update through Tools".into()),
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
        let mut manifest = ToolsManifest::default();
        manifest.set_package(ToolProvider::Npm.id(), "typescript", true);
        let category = build_category(ToolProvider::Npm, Vec::new(), &manifest);
        assert_eq!(category.items.len(), 1);
        assert_eq!(category.items[0].status, ToolStatus::Missing);
        assert!(category.items[0].managed);
        assert_eq!(
            category.items[0].actions,
            [ToolAction::Install, ToolAction::Forget]
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
        assert_eq!(scoped.status, ToolStatus::Outdated);
    }

    #[test]
    fn bulk_import_adopts_only_installed_inventory() {
        let mut manifest = ToolsManifest::default();
        let imported = import_inventory(
            &mut manifest,
            ToolProvider::Npm,
            vec![
                InventoryItem {
                    id: "installed".to_string(),
                    name: "installed".to_string(),
                    version: Some("1".to_string()),
                    detail: String::new(),
                    status: ToolStatus::Installed,
                    removable: true,
                },
                InventoryItem {
                    id: "missing".to_string(),
                    name: "missing".to_string(),
                    version: None,
                    detail: String::new(),
                    status: ToolStatus::Missing,
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
            package_actions(ToolStatus::Installed, false, true),
            [ToolAction::Adopt, ToolAction::Uninstall]
        );
        assert_eq!(
            package_actions(ToolStatus::Outdated, true, true),
            [ToolAction::Update, ToolAction::Uninstall]
        );
    }
}
