use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use vmux_api::protocol::ManagedMcpServer;
use vmux_ecs::{
    BlockingCancellation, Job, JobPlugin, JobRequest, JobResult, JobSet, Operation,
    OperationRetained,
};

use crate::io::{ObservationIoPlugin, ReadFile, WriteFile};
use crate::mcp_config::{
    McpConfigFile, McpLayerInput, McpResolvedServer, McpServerOverride, McpTrustStore,
    McpWorkspaceRoot,
};
use crate::state::ToolProvider;
use crate::{ToolInventoryChanged, ToolsManifest};

pub struct McpRegistryPlugin;

impl Plugin for McpRegistryPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<ObservationIoPlugin>() {
            app.add_plugins(ObservationIoPlugin);
        }
        if !app.is_plugin_added::<JobPlugin<ResolveWorkspaceRoot>>() {
            app.add_plugins(JobPlugin::<ResolveWorkspaceRoot>::default());
        }
        app.add_message::<ToolInventoryChanged>().add_systems(
            Update,
            (
                start_resolve,
                workspace_resolved,
                spawn_layer_reads,
                collect_resolve,
                start_save,
                apply_save,
                start_delete,
                apply_delete,
                start_trust,
                apply_trust,
                finish_write,
            )
                .chain()
                .before(JobSet::Submit),
        );
    }
}

#[derive(Component, Clone, Debug)]
pub struct McpResolveRequest {
    pub config_dir: PathBuf,
    pub profile_dir: PathBuf,
    pub cwd: Option<PathBuf>,
    pub agent_id: Option<String>,
    pub env: Vec<(String, String)>,
}

impl McpResolveRequest {
    const FILE_LIMIT: usize = 1024 * 1024;

    fn layer_paths(
        &self,
        workspace: Option<&McpWorkspaceRoot>,
    ) -> (Option<String>, Vec<(LayerKind, PathBuf)>) {
        let mut paths = vec![
            (
                LayerKind::Legacy,
                self.config_dir.join("tools").join("tools.toml"),
            ),
            (LayerKind::Global, self.config_dir.join("mcp.ron")),
        ];
        if self.profile_dir != self.config_dir {
            paths.push((LayerKind::Profile, self.profile_dir.join("mcp.ron")));
        }
        paths.push((LayerKind::Trust, self.profile_dir.join("mcp-trust.ron")));
        let mut repo_root = None;
        if let Some(workspace) = workspace {
            repo_root = Some(workspace.primary_root.to_string_lossy().into_owned());
            paths.push((
                LayerKind::Project,
                workspace.checkout_root.join(".vmux").join("mcp.ron"),
            ));
            paths.push((
                LayerKind::PrimaryLocal,
                workspace.primary_root.join(".vmux").join("mcp.local.ron"),
            ));
            if workspace.checkout_root != workspace.primary_root {
                paths.push((
                    LayerKind::WorktreeLocal,
                    workspace.checkout_root.join(".vmux").join("mcp.local.ron"),
                ));
            }
        }
        (repo_root, paths)
    }
}

#[derive(Component)]
struct WorkspaceReady(Option<McpWorkspaceRoot>);

#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct McpResolveResult {
    #[allow(unknown_lints, vmux_ecs_ownership)]
    pub servers: Vec<McpResolvedServer>,
    pub managed: Vec<ManagedMcpServer>,
    pub diagnostics: Vec<String>,
    pub repo_root: Option<String>,
}

#[derive(Component, Clone, Debug)]
pub struct McpSaveEntry {
    pub path: PathBuf,
    pub id: String,
    pub entry: McpServerOverride,
    pub summary: String,
}

#[derive(Component, Clone, Debug)]
pub struct McpDeleteEntry {
    pub path: PathBuf,
    pub id: String,
    pub summary: String,
    pub ignore_missing: bool,
}

#[derive(Component, Clone, Debug)]
pub struct McpApproveTrust {
    pub trust_path: PathBuf,
    pub repo_root: String,
    pub id: String,
    pub digest: String,
}

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct McpMutationResult(pub Result<String, String>);

#[derive(Clone)]
struct ResolveWorkspaceRoot {
    cwd: PathBuf,
}

impl Job for ResolveWorkspaceRoot {
    type Output = Result<Option<McpWorkspaceRoot>, String>;

    fn run(self) -> Self::Output {
        if BlockingCancellation::canceled() {
            return Err("workspace root lookup canceled".into());
        }
        Ok(McpWorkspaceRoot::resolve(&self.cwd))
    }
}

#[derive(Component)]
struct ResolveStarted;

#[derive(Component)]
struct ResolveWorkspaceJob;

#[derive(Component)]
struct ResolveLayers {
    repo_root: Option<String>,
    diagnostics: Vec<String>,
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
enum LayerKind {
    Legacy,
    Global,
    Profile,
    Project,
    PrimaryLocal,
    WorktreeLocal,
    Trust,
}

#[derive(Component)]
struct LayerRead {
    kind: LayerKind,
    path: PathBuf,
}

impl LayerRead {
    fn decode_into(&self, result: &JobResult<ReadFile>, input: &mut McpLayerInput) {
        let bytes = match &result.0 {
            Ok(Ok(file)) => match &file.0 {
                Some(bytes) => bytes,
                None => return,
            },
            Ok(Err(error)) | Err(error) => {
                input
                    .diagnostics
                    .push(format!("{}: {error}", self.path.display()));
                return;
            }
        };
        let text = match std::str::from_utf8(bytes) {
            Ok(text) => text,
            Err(error) => {
                input
                    .diagnostics
                    .push(format!("{}: {error}", self.path.display()));
                return;
            }
        };
        self.parse_text(text, &self.path, input);
    }

    fn parse_text(&self, text: &str, path: &Path, input: &mut McpLayerInput) {
        match self.kind {
            LayerKind::Legacy => match toml::from_str::<ToolsManifest>(text) {
                Ok(manifest) => input.legacy = McpConfigFile::from_legacy(&manifest.mcp),
                Err(error) => input
                    .diagnostics
                    .push(format!("{}: {error}", path.display())),
            },
            LayerKind::Trust => match McpTrustStore::parse(text) {
                Ok(store) => input.trust = store,
                Err(error) => input
                    .diagnostics
                    .push(format!("{}: {error}", path.display())),
            },
            other => match McpConfigFile::parse(text) {
                Ok(config) => match other {
                    LayerKind::Global => input.global = config,
                    LayerKind::Profile => input.profile = config,
                    LayerKind::Project => input.project = config,
                    LayerKind::PrimaryLocal => input.primary_local = config,
                    LayerKind::WorktreeLocal => input.worktree_local = config,
                    LayerKind::Legacy | LayerKind::Trust => {}
                },
                Err(error) => input
                    .diagnostics
                    .push(format!("{}: {error}", path.display())),
            },
        }
    }
}

#[derive(Component)]
struct LayerReadDone;

#[derive(Component)]
struct MutationStarted;

#[derive(Component)]
struct SaveReadJob {
    path: PathBuf,
    id: String,
    entry: McpServerOverride,
    summary: String,
}

#[derive(Component)]
struct DeleteReadJob {
    path: PathBuf,
    id: String,
    summary: String,
    ignore_missing: bool,
}

#[derive(Component)]
struct TrustReadJob {
    path: PathBuf,
    repo_root: String,
    id: String,
    digest: String,
}

#[derive(Component)]
struct PendingWrite {
    summary: String,
}

fn start_resolve(
    requests: Query<(Entity, &McpResolveRequest), Without<ResolveStarted>>,
    mut commands: Commands,
) {
    for (entity, request) in &requests {
        commands.entity(entity).insert(ResolveStarted);
        if let Some(cwd) = &request.cwd {
            commands.spawn((
                Name::new("MCP workspace lookup"),
                Operation::bundle("tool.mcp.workspace").unwrap().running(),
                OperationRetained,
                ChildOf(entity),
                ResolveWorkspaceJob,
                JobRequest(ResolveWorkspaceRoot { cwd: cwd.clone() }),
            ));
        } else {
            commands.entity(entity).insert(WorkspaceReady(None));
        }
    }
}

fn workspace_resolved(
    jobs: Query<(Entity, &ChildOf, &JobResult<ResolveWorkspaceRoot>), With<ResolveWorkspaceJob>>,
    mut commands: Commands,
) {
    for (job, parent, result) in &jobs {
        commands.entity(job).despawn();
        let workspace = match &result.0 {
            Ok(Ok(root)) => root.clone(),
            Ok(Err(_)) | Err(_) => None,
        };
        if let Ok(mut owner) = commands.get_entity(parent.parent()) {
            owner.insert(WorkspaceReady(workspace));
        }
    }
}

fn spawn_layer_reads(
    requests: Query<(Entity, &McpResolveRequest, &WorkspaceReady)>,
    mut commands: Commands,
) {
    for (entity, request, ready) in &requests {
        let (repo_root, paths) = request.layer_paths(ready.0.as_ref());
        commands
            .entity(entity)
            .remove::<WorkspaceReady>()
            .insert(ResolveLayers {
                repo_root,
                diagnostics: Vec::new(),
            });
        for (kind, path) in paths {
            commands.spawn((
                Name::new("MCP layer read"),
                Operation::bundle("tool.mcp.read").unwrap().running(),
                OperationRetained,
                ChildOf(entity),
                LayerRead {
                    kind,
                    path: path.clone(),
                },
                JobRequest(ReadFile {
                    path,
                    limit: McpResolveRequest::FILE_LIMIT,
                }),
            ));
        }
    }
}

fn collect_resolve(
    requests: Query<(Entity, &McpResolveRequest, &ResolveLayers)>,
    reads: Query<
        (Entity, &ChildOf, &LayerRead, Option<&JobResult<ReadFile>>),
        Without<LayerReadDone>,
    >,
    mut commands: Commands,
) {
    for (entity, request, layers) in &requests {
        let mut children = Vec::new();
        let mut ready = true;
        for (child, parent, read, result) in &reads {
            if parent.parent() != entity {
                continue;
            }
            if result.is_none() {
                ready = false;
                break;
            }
            children.push((child, read, result.unwrap()));
        }
        if !ready {
            continue;
        }
        let mut input = McpLayerInput {
            repo_root: layers.repo_root.clone(),
            diagnostics: layers.diagnostics.clone(),
            ..Default::default()
        };
        for (child, read, result) in children {
            read.decode_into(result, &mut input);
            commands.entity(child).insert(LayerReadDone).despawn();
        }
        let (servers, managed) = if let Some(agent_id) = &request.agent_id {
            let env_map: BTreeMap<&str, &str> = request
                .env
                .iter()
                .map(|(key, value)| (key.as_str(), value.as_str()))
                .collect();
            input.resolve_managed(agent_id, |variable| {
                env_map
                    .get(variable)
                    .map(|value| (*value).to_string())
                    .or_else(|| std::env::var(variable).ok())
            })
        } else {
            (input.resolve(), Vec::new())
        };
        commands
            .entity(entity)
            .remove::<(McpResolveRequest, ResolveStarted, ResolveLayers)>()
            .insert(McpResolveResult {
                servers,
                managed,
                diagnostics: input.diagnostics,
                repo_root: input.repo_root,
            });
    }
}

fn start_save(
    requests: Query<(Entity, &McpSaveEntry), Without<MutationStarted>>,
    mut commands: Commands,
) {
    for (entity, request) in &requests {
        let id = match McpConfigFile::validate_server_id(&request.id) {
            Ok(id) => id,
            Err(error) => {
                commands
                    .entity(entity)
                    .remove::<McpSaveEntry>()
                    .insert(McpMutationResult(Err(error)));
                continue;
            }
        };
        if let McpServerOverride::Define(spec) = &request.entry
            && let Err(error) = spec.validate()
        {
            commands
                .entity(entity)
                .remove::<McpSaveEntry>()
                .insert(McpMutationResult(Err(error)));
            continue;
        }
        commands.entity(entity).insert(MutationStarted);
        commands.spawn((
            Name::new("Read MCP config for save"),
            Operation::bundle("tool.mcp.save.read").unwrap().running(),
            OperationRetained,
            ChildOf(entity),
            SaveReadJob {
                path: request.path.clone(),
                id,
                entry: request.entry.clone(),
                summary: request.summary.clone(),
            },
            JobRequest(ReadFile {
                path: request.path.clone(),
                limit: McpResolveRequest::FILE_LIMIT,
            }),
        ));
    }
}

fn apply_save(
    jobs: Query<(Entity, &ChildOf, &SaveReadJob, &JobResult<ReadFile>)>,
    mut commands: Commands,
) {
    for (job, parent, save, result) in &jobs {
        commands.entity(job).despawn();
        let owner = parent.parent();
        let Ok(mut owner_commands) = commands.get_entity(owner) else {
            continue;
        };
        let mut config = match &result.0 {
            Ok(Ok(file)) => match &file.0 {
                Some(bytes) => match std::str::from_utf8(bytes)
                    .map_err(|error| error.to_string())
                    .and_then(McpConfigFile::parse)
                {
                    Ok(config) => config,
                    Err(error) => {
                        owner_commands
                            .remove::<(McpSaveEntry, MutationStarted)>()
                            .insert(McpMutationResult(Err(error)));
                        continue;
                    }
                },
                None => McpConfigFile::default(),
            },
            Ok(Err(error)) | Err(error) => {
                owner_commands
                    .remove::<(McpSaveEntry, MutationStarted)>()
                    .insert(McpMutationResult(Err(error.clone())));
                continue;
            }
        };
        config.servers.insert(save.id.clone(), save.entry.clone());
        let encoded = match config.encode() {
            Ok(text) => text,
            Err(error) => {
                owner_commands
                    .remove::<(McpSaveEntry, MutationStarted)>()
                    .insert(McpMutationResult(Err(error)));
                continue;
            }
        };
        commands.spawn((
            Name::new("Write MCP config"),
            Operation::bundle("tool.mcp.save.write").unwrap().running(),
            OperationRetained,
            ChildOf(owner),
            PendingWrite {
                summary: save.summary.clone(),
            },
            JobRequest(WriteFile {
                path: save.path.clone(),
                bytes: encoded.into_bytes(),
            }),
        ));
    }
}

fn start_delete(
    requests: Query<(Entity, &McpDeleteEntry), Without<MutationStarted>>,
    mut commands: Commands,
) {
    for (entity, request) in &requests {
        let id = match McpConfigFile::validate_server_id(&request.id) {
            Ok(id) => id,
            Err(error) => {
                commands
                    .entity(entity)
                    .remove::<McpDeleteEntry>()
                    .insert(McpMutationResult(Err(error)));
                continue;
            }
        };
        commands.entity(entity).insert(MutationStarted);
        commands.spawn((
            Name::new("Read MCP config for delete"),
            Operation::bundle("tool.mcp.delete.read").unwrap().running(),
            OperationRetained,
            ChildOf(entity),
            DeleteReadJob {
                path: request.path.clone(),
                id,
                summary: request.summary.clone(),
                ignore_missing: request.ignore_missing,
            },
            JobRequest(ReadFile {
                path: request.path.clone(),
                limit: McpResolveRequest::FILE_LIMIT,
            }),
        ));
    }
}

fn apply_delete(
    jobs: Query<(Entity, &ChildOf, &DeleteReadJob, &JobResult<ReadFile>)>,
    mut commands: Commands,
) {
    for (job, parent, delete, result) in &jobs {
        commands.entity(job).despawn();
        let owner = parent.parent();
        let Ok(mut owner_commands) = commands.get_entity(owner) else {
            continue;
        };
        let mut config = match &result.0 {
            Ok(Ok(file)) => match &file.0 {
                Some(bytes) => match std::str::from_utf8(bytes)
                    .map_err(|error| error.to_string())
                    .and_then(McpConfigFile::parse)
                {
                    Ok(config) => config,
                    Err(error) => {
                        owner_commands
                            .remove::<(McpDeleteEntry, MutationStarted)>()
                            .insert(McpMutationResult(Err(error)));
                        continue;
                    }
                },
                None => McpConfigFile::default(),
            },
            Ok(Err(error)) | Err(error) => {
                owner_commands
                    .remove::<(McpDeleteEntry, MutationStarted)>()
                    .insert(McpMutationResult(Err(error.clone())));
                continue;
            }
        };
        if config.servers.remove(&delete.id).is_none() {
            let outcome = if delete.ignore_missing {
                Ok(delete.summary.clone())
            } else {
                Err(format!("MCP server not found: {}", delete.id))
            };
            owner_commands
                .remove::<(McpDeleteEntry, MutationStarted)>()
                .insert(McpMutationResult(outcome));
            continue;
        }
        let encoded = match config.encode() {
            Ok(text) => text,
            Err(error) => {
                owner_commands
                    .remove::<(McpDeleteEntry, MutationStarted)>()
                    .insert(McpMutationResult(Err(error)));
                continue;
            }
        };
        commands.spawn((
            Name::new("Write MCP config after delete"),
            Operation::bundle("tool.mcp.delete.write")
                .unwrap()
                .running(),
            OperationRetained,
            ChildOf(owner),
            PendingWrite {
                summary: delete.summary.clone(),
            },
            JobRequest(WriteFile {
                path: delete.path.clone(),
                bytes: encoded.into_bytes(),
            }),
        ));
    }
}

fn start_trust(
    requests: Query<(Entity, &McpApproveTrust), Without<MutationStarted>>,
    mut commands: Commands,
) {
    for (entity, request) in &requests {
        let id = match McpConfigFile::validate_server_id(&request.id) {
            Ok(id) => id,
            Err(error) => {
                commands
                    .entity(entity)
                    .remove::<McpApproveTrust>()
                    .insert(McpMutationResult(Err(error)));
                continue;
            }
        };
        if request.repo_root.trim().is_empty() || request.digest.trim().is_empty() {
            commands
                .entity(entity)
                .remove::<McpApproveTrust>()
                .insert(McpMutationResult(Err(
                    "repository root and digest are required".into(),
                )));
            continue;
        }
        commands.entity(entity).insert(MutationStarted);
        commands.spawn((
            Name::new("Read MCP trust store"),
            Operation::bundle("tool.mcp.trust.read").unwrap().running(),
            OperationRetained,
            ChildOf(entity),
            TrustReadJob {
                path: request.trust_path.clone(),
                repo_root: request.repo_root.clone(),
                id,
                digest: request.digest.clone(),
            },
            JobRequest(ReadFile {
                path: request.trust_path.clone(),
                limit: McpResolveRequest::FILE_LIMIT,
            }),
        ));
    }
}

fn apply_trust(
    jobs: Query<(Entity, &ChildOf, &TrustReadJob, &JobResult<ReadFile>)>,
    mut commands: Commands,
) {
    for (job, parent, trust, result) in &jobs {
        commands.entity(job).despawn();
        let owner = parent.parent();
        let Ok(mut owner_commands) = commands.get_entity(owner) else {
            continue;
        };
        let mut store = match &result.0 {
            Ok(Ok(file)) => match &file.0 {
                Some(bytes) => match std::str::from_utf8(bytes)
                    .map_err(|error| error.to_string())
                    .and_then(McpTrustStore::parse)
                {
                    Ok(store) => store,
                    Err(error) => {
                        owner_commands
                            .remove::<(McpApproveTrust, MutationStarted)>()
                            .insert(McpMutationResult(Err(error)));
                        continue;
                    }
                },
                None => McpTrustStore::default(),
            },
            Ok(Err(error)) | Err(error) => {
                owner_commands
                    .remove::<(McpApproveTrust, MutationStarted)>()
                    .insert(McpMutationResult(Err(error.clone())));
                continue;
            }
        };
        store.trust(
            trust.repo_root.clone(),
            trust.id.clone(),
            trust.digest.clone(),
        );
        let encoded = match store.encode() {
            Ok(text) => text,
            Err(error) => {
                owner_commands
                    .remove::<(McpApproveTrust, MutationStarted)>()
                    .insert(McpMutationResult(Err(error)));
                continue;
            }
        };
        commands.spawn((
            Name::new("Write MCP trust store"),
            Operation::bundle("tool.mcp.trust.write").unwrap().running(),
            OperationRetained,
            ChildOf(owner),
            PendingWrite {
                summary: format!("{} trusted", trust.id),
            },
            JobRequest(WriteFile {
                path: trust.path.clone(),
                bytes: encoded.into_bytes(),
            }),
        ));
    }
}

fn finish_write(
    jobs: Query<(Entity, &ChildOf, &PendingWrite, &JobResult<WriteFile>)>,
    mut changed: MessageWriter<ToolInventoryChanged>,
    mut commands: Commands,
) {
    for (job, parent, write, result) in &jobs {
        commands.entity(job).despawn();
        let owner = parent.parent();
        let outcome = match &result.0 {
            Ok(Ok(())) => {
                changed.write(ToolInventoryChanged(ToolProvider::new("mcp")));
                Ok(write.summary.clone())
            }
            Ok(Err(error)) | Err(error) => Err(error.clone()),
        };
        if let Ok(mut owner_commands) = commands.get_entity(owner) {
            owner_commands
                .remove::<(
                    McpSaveEntry,
                    McpDeleteEntry,
                    McpApproveTrust,
                    MutationStarted,
                )>()
                .insert(McpMutationResult(outcome));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp_config::{McpAuthSpec, McpConfigScope, McpServerSpec};
    use std::time::{Duration, Instant};

    struct RegistryFixture {
        app: App,
        _dir: tempfile::TempDir,
        config_dir: PathBuf,
        profile_dir: PathBuf,
        primary: PathBuf,
        worktree: PathBuf,
    }

    impl RegistryFixture {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let config_dir = dir.path().join("config");
            let profile_dir = dir.path().join("profile");
            let primary = dir.path().join("repo");
            let worktree = primary.join(".worktrees/feature");
            let git_worktree = primary.join(".git/worktrees/feature");
            std::fs::create_dir_all(&git_worktree).unwrap();
            std::fs::create_dir_all(&worktree).unwrap();
            std::fs::write(
                worktree.join(".git"),
                format!("gitdir: {}\n", git_worktree.display()),
            )
            .unwrap();
            std::fs::write(git_worktree.join("commondir"), "../..\n").unwrap();
            let mut app = App::new();
            app.add_plugins((bevy_app::TaskPoolPlugin::default(), McpRegistryPlugin));
            Self {
                app,
                _dir: dir,
                config_dir,
                profile_dir,
                primary,
                worktree,
            }
        }

        fn drain<T: Component + Clone>(&mut self, entity: Entity) -> T {
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                self.app.update();
                if let Some(value) = self.app.world().get::<T>(entity).cloned() {
                    return value;
                }
                assert!(Instant::now() < deadline, "MCP registry task did not drain");
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    }

    #[test]
    fn registry_saves_resolves_and_trusts_project_mcp_servers_across_worktrees() {
        let mut fixture = RegistryFixture::new();
        let save_global = fixture
            .app
            .world_mut()
            .spawn(McpSaveEntry {
                path: fixture.config_dir.join("mcp.ron"),
                id: "linear-mistral".into(),
                entry: McpServerOverride::Define(McpServerSpec::Http {
                    url: "https://mcp.linear.app/mcp".into(),
                    headers: BTreeMap::new(),
                    header_env: BTreeMap::new(),
                    bearer_token_env_var: Some("MISTRAL_LINEAR".into()),
                    auth: McpAuthSpec::None,
                    enabled: true,
                    agents: Vec::new(),
                }),
                summary: "saved linear-mistral".into(),
            })
            .id();
        assert!(fixture.drain::<McpMutationResult>(save_global).0.is_ok());

        let save_project = fixture
            .app
            .world_mut()
            .spawn(McpSaveEntry {
                path: fixture.worktree.join(".vmux/mcp.ron"),
                id: "repo-server".into(),
                entry: McpServerOverride::Define(McpServerSpec::Stdio {
                    command: "npx".into(),
                    args: vec!["-y".into(), "repo-mcp".into()],
                    env: BTreeMap::new(),
                    cwd: None,
                    enabled: true,
                    agents: Vec::new(),
                }),
                summary: "saved repo-server".into(),
            })
            .id();
        assert!(fixture.drain::<McpMutationResult>(save_project).0.is_ok());

        let save_local = fixture
            .app
            .world_mut()
            .spawn(McpSaveEntry {
                path: fixture.primary.join(".vmux/mcp.local.ron"),
                id: "linear-mistral".into(),
                entry: McpServerOverride::Disable,
                summary: "disabled linear-mistral".into(),
            })
            .id();
        assert!(fixture.drain::<McpMutationResult>(save_local).0.is_ok());

        let resolve_entity = fixture
            .app
            .world_mut()
            .spawn(McpResolveRequest {
                config_dir: fixture.config_dir.clone(),
                profile_dir: fixture.profile_dir.clone(),
                cwd: Some(fixture.worktree.clone()),
                agent_id: Some("claude-acp".into()),
                env: vec![("MISTRAL_LINEAR".into(), "secret".into())],
            })
            .id();
        let resolved = fixture.drain::<McpResolveResult>(resolve_entity);
        assert!(resolved.managed.is_empty());
        let repo_server = resolved
            .servers
            .iter()
            .find(|server| server.id == "repo-server")
            .unwrap();
        assert_eq!(repo_server.source, McpConfigScope::Project);
        assert!(!repo_server.trusted);
        let linear = resolved
            .servers
            .iter()
            .find(|server| server.id == "linear-mistral")
            .unwrap();
        assert!(!linear.enabled);
        assert_eq!(linear.source, McpConfigScope::ProjectLocal);

        let trust_entity = fixture
            .app
            .world_mut()
            .spawn(McpApproveTrust {
                trust_path: fixture.profile_dir.join("mcp-trust.ron"),
                repo_root: repo_server.repo_root.clone().unwrap(),
                id: "repo-server".into(),
                digest: repo_server.digest.clone().unwrap(),
            })
            .id();
        assert!(fixture.drain::<McpMutationResult>(trust_entity).0.is_ok());

        let resolve_after = fixture
            .app
            .world_mut()
            .spawn(McpResolveRequest {
                config_dir: fixture.config_dir.clone(),
                profile_dir: fixture.profile_dir.clone(),
                cwd: Some(fixture.worktree.clone()),
                agent_id: Some("claude-acp".into()),
                env: Vec::new(),
            })
            .id();
        let after = fixture.drain::<McpResolveResult>(resolve_after);
        assert_eq!(after.managed.len(), 1);
        assert_eq!(after.managed[0].name, "repo-server");
    }
}
