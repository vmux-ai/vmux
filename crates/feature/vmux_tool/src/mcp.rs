use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::state::{
    ToolAdoptRequest, ToolCategory, ToolForgetRequest, ToolImportRequest, ToolItem,
    ToolOperationKind, ToolProvider, ToolStatus,
};
use bevy_app::{App, Plugin, Startup, Update};
use bevy_ecs::prelude::*;
use bevy_tasks::IoTaskPool;
use serde::{Deserialize, Serialize};

use crate::manifest::{ToolStore, ToolsManifest};
use crate::{
    ToolOperationFailed, ToolOperationFinished, ToolOperationRequest, ToolOperationRouteFlush,
    ToolOperationRouteSet, ToolOperationSucceeded, ToolOperationTask, ToolProviderBinding,
    ToolProviderSnapshot, ToolProviderTarget, ToolScanner, ToolStoreOperation, ToolStoreTarget,
};

pub(crate) struct McpToolPlugin;

#[derive(Component)]
struct McpProvider;

impl Plugin for McpToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn)
            .add_systems(
                Update,
                (request_import, request_adopt, request_forget).in_set(ToolOperationRouteSet),
            )
            .add_systems(
                Update,
                (
                    locate,
                    discover,
                    import_file,
                    import_all,
                    import_one,
                    forget,
                )
                    .after(ToolOperationRouteFlush),
            )
            .add_systems(Update, (finish_file, finish_import, finish_forget));
    }
}

fn spawn(mut commands: Commands) {
    commands.spawn((
        ToolProviderBinding::new::<crate::Feature>(3),
        McpProvider,
        ToolScanner::new(McpProvider::scan),
    ));
}

impl McpProvider {
    fn scan(
        provider: &ToolProvider,
        store: &ToolStore,
        manifest: &mut ToolsManifest,
        _refresh: bool,
    ) -> Result<ToolProviderSnapshot, String> {
        let (discovered, errors) = store.discover_mcp_servers();
        let errors = errors
            .into_iter()
            .map(|error| format!("MCP Servers: {error}"))
            .collect();
        for (name, server) in &discovered {
            if name != "vmux" && name != "linear" && !server.conflict {
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
                let operations = if managed {
                    vec![ToolOperationKind::Forget]
                } else if status == ToolStatus::Available {
                    vec![ToolOperationKind::Adopt]
                } else {
                    Vec::new()
                };
                ToolItem {
                    provider: provider.clone(),
                    id: name.clone(),
                    name,
                    icon: None,
                    version: None,
                    detail,
                    status,
                    managed,
                    operations,
                }
            })
            .collect();
        Ok(ToolProviderSnapshot {
            category: ToolCategory {
                provider: provider.clone(),
                items,
            },
            errors,
        })
    }
}

fn request_import(
    requests: Query<
        (
            Entity,
            &ToolOperationRequest<ToolImportRequest>,
            &ToolProviderTarget,
        ),
        Added<ToolStoreTarget>,
    >,
    providers: Query<(), With<McpProvider>>,
    mut commands: Commands,
) {
    for (entity, operation, provider) in &requests {
        if !providers.contains(provider.entity()) {
            continue;
        }
        let request = &operation.0;
        let value = request.value.trim();
        let operation = if value.is_empty() {
            commands.entity(entity).insert((
                ToolStoreOperation,
                DiscoverMcpServers,
                ImportDefaultMcpConfigs,
            ));
            continue;
        } else {
            ImportMcpConfig {
                path: PathBuf::from(value),
            }
        };
        commands
            .entity(entity)
            .insert((ToolStoreOperation, operation));
    }
}

fn request_adopt(
    requests: Query<
        (
            Entity,
            &ToolOperationRequest<ToolAdoptRequest>,
            &ToolProviderTarget,
        ),
        Added<ToolStoreTarget>,
    >,
    providers: Query<(), With<McpProvider>>,
    mut commands: Commands,
) {
    for (entity, operation, provider) in &requests {
        if !providers.contains(provider.entity()) {
            continue;
        }
        let request = &operation.0;
        if request.id.trim().is_empty() {
            continue;
        }
        commands.entity(entity).insert((
            ToolStoreOperation,
            DiscoverMcpServers,
            ImportMcpServer {
                name: request.id.trim().to_string(),
            },
        ));
    }
}

fn request_forget(
    requests: Query<
        (
            Entity,
            &ToolOperationRequest<ToolForgetRequest>,
            &ToolProviderTarget,
        ),
        Added<ToolStoreTarget>,
    >,
    providers: Query<(), With<McpProvider>>,
    mut commands: Commands,
) {
    for (entity, operation, provider) in &requests {
        if !providers.contains(provider.entity()) {
            continue;
        }
        let request = &operation.0;
        if request.id.trim().is_empty() {
            continue;
        }
        commands.entity(entity).insert((
            ToolStoreOperation,
            ForgetMcpServer {
                name: request.id.trim().to_string(),
            },
        ));
    }
}

fn finish_file(
    operations: Query<
        (Entity, &ImportedMcpConfig),
        (With<ToolStoreOperation>, Without<ToolOperationFinished>),
    >,
    mut commands: Commands,
) {
    for (entity, output) in &operations {
        commands.entity(entity).insert((
            ToolOperationFinished,
            ToolOperationSucceeded(format!("imported {} MCP server(s)", output.servers)),
        ));
    }
}

fn finish_import(
    operations: Query<
        (Entity, &ImportedMcpServer),
        (With<ToolStoreOperation>, Without<ToolOperationFinished>),
    >,
    mut commands: Commands,
) {
    for (entity, output) in &operations {
        commands.entity(entity).insert((
            ToolOperationFinished,
            ToolOperationSucceeded(format!("{} is now managed", output.name)),
        ));
    }
}

fn finish_forget(
    operations: Query<
        (Entity, &ForgottenMcpServer),
        (With<ToolStoreOperation>, Without<ToolOperationFinished>),
    >,
    mut commands: Commands,
) {
    for (entity, output) in &operations {
        commands.entity(entity).insert((
            ToolOperationFinished,
            ToolOperationSucceeded(format!("{} removed from tools.toml", output.name)),
        ));
    }
}

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
struct DiscoverMcpServers;

#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct DiscoveredMcpServers {
    servers: BTreeMap<String, DiscoveredMcpServer>,
    errors: Vec<String>,
}

#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
struct McpConfigSources(Vec<PathBuf>);

impl McpConfigSources {
    fn at(home: &Path) -> Self {
        Self(
            [
                home.join(".codex/config.toml"),
                home.join(".claude.json"),
                home.join(".vibe/config.toml"),
                home.join(".mcp.json"),
            ]
            .into_iter()
            .filter(|path| path.is_file())
            .collect(),
        )
    }

    fn discover(&self) -> DiscoveredMcpServers {
        let mut servers = BTreeMap::<String, DiscoveredMcpServer>::new();
        let mut errors = Vec::new();
        for path in &self.0 {
            match McpConfigDocument::from_file(path) {
                Ok(document) => {
                    for (name, definition) in document.into_servers() {
                        match servers.get_mut(&name) {
                            Some(existing) => {
                                existing.conflict |= existing.definition != definition;
                                existing.sources.push(path.clone());
                            }
                            None => {
                                servers.insert(
                                    name,
                                    DiscoveredMcpServer {
                                        definition,
                                        sources: vec![path.clone()],
                                        conflict: false,
                                    },
                                );
                            }
                        }
                    }
                }
                Err(error) => errors.push(format!("{}: {error}", path.display())),
            }
        }
        DiscoveredMcpServers { servers, errors }
    }
}

fn locate(
    operations: Query<(Entity, &ToolStoreTarget), Added<DiscoverMcpServers>>,
    stores: Query<&ToolStore>,
    mut commands: Commands,
) {
    for (entity, target) in &operations {
        let Ok(store) = stores.get(target.entity()) else {
            commands.entity(entity).insert((
                ToolOperationFinished,
                ToolOperationFailed("tool store entity is unavailable".to_string()),
            ));
            continue;
        };
        commands
            .entity(entity)
            .insert(McpConfigSources::at(store.home()));
    }
}

fn discover(
    operations: Query<
        (Entity, &McpConfigSources),
        (
            With<DiscoverMcpServers>,
            Without<ToolOperationTask<DiscoveredMcpServers>>,
            Without<DiscoveredMcpServers>,
            Without<ToolOperationFinished>,
        ),
    >,
    mut commands: Commands,
) {
    for (entity, sources) in &operations {
        let sources = sources.clone();
        commands.entity(entity).insert(ToolOperationTask(
            IoTaskPool::get().spawn(async move { Ok(sources.discover()) }),
        ));
    }
}

#[derive(Component, Clone, Debug, PartialEq, Eq)]
struct ImportMcpConfig {
    path: PathBuf,
}

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
struct ImportDefaultMcpConfigs;

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ImportedMcpConfig {
    servers: usize,
}

fn import_file(
    operations: Query<
        (Entity, &ImportMcpConfig, &ToolStoreTarget),
        (
            Without<ToolOperationTask<ImportedMcpConfig>>,
            Without<ImportedMcpConfig>,
            Without<ToolOperationFinished>,
        ),
    >,
    stores: Query<&ToolStore>,
    mut commands: Commands,
) {
    for (entity, operation, target) in &operations {
        let Ok(store) = stores.get(target.entity()).cloned() else {
            commands.entity(entity).insert((
                ToolOperationFinished,
                ToolOperationFailed("tool store entity is unavailable".to_string()),
            ));
            continue;
        };
        let path = operation.path.clone();
        commands
            .entity(entity)
            .insert(ToolOperationTask(IoTaskPool::get().spawn(async move {
                let servers = store.import_mcp_config(&path)?;
                Ok(ImportedMcpConfig { servers })
            })));
    }
}

fn import_all(
    operations: Query<
        (Entity, &DiscoveredMcpServers, &ToolStoreTarget),
        (
            With<ImportDefaultMcpConfigs>,
            Without<ToolOperationTask<ImportedMcpConfig>>,
            Without<ImportedMcpConfig>,
            Without<ToolOperationFinished>,
        ),
    >,
    stores: Query<&ToolStore>,
    mut commands: Commands,
) {
    for (entity, discovered, target) in &operations {
        let Ok(store) = stores.get(target.entity()).cloned() else {
            commands.entity(entity).insert((
                ToolOperationFinished,
                ToolOperationFailed("tool store entity is unavailable".to_string()),
            ));
            continue;
        };
        let discovered = discovered.clone();
        commands
            .entity(entity)
            .insert(ToolOperationTask(IoTaskPool::get().spawn(async move {
                let servers = discovered.import_all(&store)?;
                Ok(ImportedMcpConfig { servers })
            })));
    }
}

#[derive(Component, Clone, Debug, PartialEq, Eq)]
struct ImportMcpServer {
    name: String,
}

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub(super) struct ImportedMcpServer {
    name: String,
}

fn import_one(
    operations: Query<
        (
            Entity,
            &ImportMcpServer,
            &DiscoveredMcpServers,
            &ToolStoreTarget,
        ),
        (
            Without<ToolOperationTask<ImportedMcpServer>>,
            Without<ImportedMcpServer>,
            Without<ToolOperationFinished>,
        ),
    >,
    stores: Query<&ToolStore>,
    mut commands: Commands,
) {
    for (entity, operation, discovered, target) in &operations {
        let Ok(store) = stores.get(target.entity()).cloned() else {
            commands.entity(entity).insert((
                ToolOperationFinished,
                ToolOperationFailed("tool store entity is unavailable".to_string()),
            ));
            continue;
        };
        let name = operation.name.clone();
        let discovered = discovered.clone();
        commands
            .entity(entity)
            .insert(ToolOperationTask(IoTaskPool::get().spawn(async move {
                discovered.import(&store, &name)?;
                Ok(ImportedMcpServer { name })
            })));
    }
}

#[derive(Component, Clone, Debug, PartialEq, Eq)]
struct ForgetMcpServer {
    name: String,
}

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub(super) struct ForgottenMcpServer {
    name: String,
}

fn forget(
    operations: Query<
        (Entity, &ForgetMcpServer, &ToolStoreTarget),
        (
            Without<ToolOperationTask<ForgottenMcpServer>>,
            Without<ForgottenMcpServer>,
            Without<ToolOperationFinished>,
        ),
    >,
    stores: Query<&ToolStore>,
    mut commands: Commands,
) {
    for (entity, operation, target) in &operations {
        let Ok(store) = stores.get(target.entity()).cloned() else {
            commands.entity(entity).insert((
                ToolOperationFinished,
                ToolOperationFailed("tool store entity is unavailable".to_string()),
            ));
            continue;
        };
        let name = operation.name.clone();
        commands
            .entity(entity)
            .insert(ToolOperationTask(IoTaskPool::get().spawn(async move {
                let mut manifest = store.load()?;
                manifest.mcp.servers.remove(&name);
                store.save(&manifest)?;
                Ok(ForgottenMcpServer { name })
            })));
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpManifest {
    #[serde(default)]
    pub servers: BTreeMap<String, McpServerManifest>,
}

impl McpManifest {
    pub(crate) fn is_empty(&self) -> bool {
        self.servers.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpServerManifest {
    pub transport: McpTransport,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub header_env: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bearer_token_env_var: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum McpTransport {
    #[default]
    Stdio,
    Http,
    Sse,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiscoveredMcpServer {
    pub definition: McpServerManifest,
    pub sources: Vec<PathBuf>,
    pub conflict: bool,
}

impl ToolStore {
    fn default_mcp_config_paths(&self) -> McpConfigSources {
        McpConfigSources::at(self.home())
    }

    fn discover_mcp_servers(&self) -> (BTreeMap<String, DiscoveredMcpServer>, Vec<String>) {
        let discovered = self.default_mcp_config_paths().discover();
        (discovered.servers, discovered.errors)
    }

    pub(crate) fn import_mcp_config(&self, path: &Path) -> Result<usize, String> {
        self.migrate_legacy_storage()?;
        self.import_mcp_config_to(path, &self.manifest_path())
    }

    pub(crate) fn import_default_mcp_configs(&self) -> Result<usize, String> {
        self.default_mcp_config_paths().discover().import_all(self)
    }

    pub(crate) fn import_mcp_config_to(
        &self,
        path: &Path,
        manifest_path: &Path,
    ) -> Result<usize, String> {
        let path = self.expand_user_path(path)?;
        let servers = McpConfigDocument::from_file(&path)?.into_servers();
        if servers.is_empty() {
            return Err(format!("no MCP servers found in {}", path.display()));
        }
        let mut manifest = ToolsManifest::read(manifest_path)?;
        let mut imported = 0;
        for (name, definition) in servers {
            if name == "vmux" {
                continue;
            }
            imported += usize::from(manifest.mcp.servers.get(&name) != Some(&definition));
            manifest.mcp.servers.insert(name, definition);
        }
        manifest.write_to(manifest_path)?;
        Ok(imported)
    }
}

impl DiscoveredMcpServers {
    fn import_all(&self, store: &ToolStore) -> Result<usize, String> {
        if !self.errors.is_empty() {
            return Err(self.errors.join("\n"));
        }
        let mut conflicts = Vec::new();
        for (name, server) in &self.servers {
            if server.conflict {
                conflicts.push(name.as_str());
            }
        }
        if !conflicts.is_empty() {
            return Err(format!(
                "conflicting MCP definitions: {}",
                conflicts.join(", ")
            ));
        }
        let mut manifest = store.load()?;
        let mut imported = 0;
        for (name, server) in &self.servers {
            if name == "vmux" {
                continue;
            }
            imported += usize::from(manifest.mcp.servers.get(name) != Some(&server.definition));
            manifest
                .mcp
                .servers
                .insert(name.clone(), server.definition.clone());
        }
        store.save(&manifest)?;
        Ok(imported)
    }

    fn import(&self, store: &ToolStore, name: &str) -> Result<(), String> {
        if !self.errors.is_empty() {
            return Err(self.errors.join("\n"));
        }
        let server = self
            .servers
            .get(name)
            .ok_or_else(|| format!("MCP server not found: {name}"))?;
        if server.conflict {
            return Err(format!(
                "MCP server {name} has conflicting definitions; import an explicit config path"
            ));
        }
        let mut manifest = store.load()?;
        manifest
            .mcp
            .servers
            .insert(name.to_string(), server.definition.clone());
        store.save(&manifest)
    }
}

pub struct McpConfigDocument(BTreeMap<String, McpServerManifest>);

impl McpConfigDocument {
    pub fn from_file(path: &Path) -> Result<Self, String> {
        let source = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
        Self::parse(&source)
    }

    pub fn parse(source: &str) -> Result<Self, String> {
        let servers = if let Ok(document) = serde_json::from_str::<serde_json::Value>(source) {
            Self::parse_json_document(&document)?
        } else {
            let document: toml::Value =
                toml::from_str(source).map_err(|error| error.to_string())?;
            Self::parse_toml_document(&document)?
        };
        Ok(Self(servers))
    }

    pub fn servers(&self) -> &BTreeMap<String, McpServerManifest> {
        &self.0
    }

    pub fn into_servers(self) -> BTreeMap<String, McpServerManifest> {
        self.0
    }

    fn parse_json_document(
        document: &serde_json::Value,
    ) -> Result<BTreeMap<String, McpServerManifest>, String> {
        let Some(servers) = document
            .get("mcpServers")
            .or_else(|| document.get("mcp_servers"))
            .and_then(serde_json::Value::as_object)
        else {
            return Ok(BTreeMap::new());
        };
        let mut parsed = BTreeMap::new();
        for (name, value) in servers {
            if name == "vmux"
                || value.get("enabled").and_then(serde_json::Value::as_bool) == Some(false)
            {
                continue;
            }
            parsed.insert(name.clone(), Self::parse_json_server(value)?);
        }
        Ok(parsed)
    }

    fn parse_toml_document(
        document: &toml::Value,
    ) -> Result<BTreeMap<String, McpServerManifest>, String> {
        let Some(servers) = document.get("mcp_servers") else {
            return Ok(BTreeMap::new());
        };
        let mut parsed = BTreeMap::new();
        match servers {
            toml::Value::Table(table) => {
                for (name, value) in table {
                    if name == "vmux"
                        || value.get("enabled").and_then(toml::Value::as_bool) == Some(false)
                    {
                        continue;
                    }
                    let value = serde_json::to_value(value).map_err(|error| error.to_string())?;
                    parsed.insert(name.clone(), Self::parse_json_server(&value)?);
                }
            }
            toml::Value::Array(entries) => {
                for entry in entries {
                    let name = entry
                        .get("name")
                        .and_then(toml::Value::as_str)
                        .ok_or("MCP server is missing name")?;
                    if name == "vmux"
                        || entry.get("enabled").and_then(toml::Value::as_bool) == Some(false)
                    {
                        continue;
                    }
                    let value = serde_json::to_value(entry).map_err(|error| error.to_string())?;
                    parsed.insert(name.to_string(), Self::parse_json_server(&value)?);
                }
            }
            _ => return Err("mcp_servers must be a table or array".to_string()),
        }
        Ok(parsed)
    }

    fn parse_json_server(value: &serde_json::Value) -> Result<McpServerManifest, String> {
        let object = value.as_object().ok_or("MCP server must be an object")?;
        let command = Self::string_field(object, "command");
        let url = Self::string_field(object, "url");
        let transport = Self::string_field(object, "transport")
            .or_else(|| Self::string_field(object, "type"))
            .map(|transport| match transport.as_str() {
                "sse" => McpTransport::Sse,
                "http" | "streamable-http" => McpTransport::Http,
                _ => McpTransport::Stdio,
            })
            .unwrap_or_else(|| {
                if url.is_some() {
                    McpTransport::Http
                } else {
                    McpTransport::Stdio
                }
            });
        match transport {
            McpTransport::Stdio if command.is_none() => {
                return Err("stdio MCP server is missing command".to_string());
            }
            McpTransport::Http | McpTransport::Sse if url.is_none() => {
                return Err("remote MCP server is missing url".to_string());
            }
            _ => {}
        }
        let args = object
            .get("args")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(serde_json::Value::as_str)
            .map(str::to_string)
            .collect();
        let headers = Self::string_map_field(object, "headers")
            .or_else(|| Self::string_map_field(object, "http_headers"))
            .unwrap_or_default();
        Ok(McpServerManifest {
            transport,
            command,
            args,
            env: Self::string_map_field(object, "env").unwrap_or_default(),
            cwd: Self::string_field(object, "cwd"),
            url,
            headers,
            header_env: Self::string_map_field(object, "env_http_headers").unwrap_or_default(),
            bearer_token_env_var: Self::string_field(object, "bearer_token_env_var"),
        })
    }

    fn string_field(
        object: &serde_json::Map<String, serde_json::Value>,
        field: &str,
    ) -> Option<String> {
        object
            .get(field)
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
    }

    fn string_map_field(
        object: &serde_json::Map<String, serde_json::Value>,
        field: &str,
    ) -> Option<BTreeMap<String, String>> {
        object
            .get(field)
            .and_then(serde_json::Value::as_object)
            .map(|values| {
                values
                    .iter()
                    .filter_map(|(name, value)| {
                        value
                            .as_str()
                            .map(|value| (name.clone(), value.to_string()))
                    })
                    .collect()
            })
    }
}
