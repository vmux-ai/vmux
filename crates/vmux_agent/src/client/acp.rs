//! GUI-side ACP agent integration: the [`AcpSession`] component identifies an ACP agent
//! pane, and [`AcpAgentPlugin`] forwards spawn/input/close to the daemon's
//! `AcpSessionManager`. The streamed updates are consumed by the shared
//! `consume_page_agent_stream` system (ACP reuses the Page stream messages).

use bevy::prelude::*;
use bevy_cef::prelude::WebviewExtendStandardMaterial;
use crossbeam_channel::{Receiver, Sender};
use vmux_core::{LastActivatedAt, event::InstallPhase};
use vmux_layout::event::TERMINAL_PAGE_URL;
use vmux_layout::pane::{PlacementCtx, resolve_spiral_pane};
use vmux_layout::stack::stack_bundle;
use vmux_service::client::ServiceClient;
use vmux_service::protocol::ClientMessage;
use vmux_setting::AppSettings;
use vmux_terminal::reattach_terminal_bundle;

use crate::components::{AgentApprovalPolicy, PromptQueue};
use crate::events::{AgentApprovalReply, AgentApprovalRequest, ApprovalDecision};
use crate::handoff::{ImportedConversation, PendingHandoff};
use crate::run_state::AgentRunState;

const UNBOUND_WORKSPACE_CONTEXT: &str = "VMUX HOST POLICY (mandatory): This tab has no selected workspace. For development work, first call select_workspace. Pass a path when the conversation identifies a local directory; otherwise vmux opens the native folder picker immediately after approval. Any directory can be a workspace. If it has no .git, vmux asks whether to initialize Git; declining keeps the plain workspace usable. Do not search the user's home directory. General questions and self-contained terminal demonstrations may run in the temporary current directory.";
const PENDING_WORKTREE_CONTEXT: &str = "VMUX HOST POLICY (mandatory): Workspace activation is pending. Do not access project paths directly or run git worktree add yourself. Wait for vmux to finish preparing the selected workspace before inspecting, editing, testing, or running the project.";
const REPOSITORY_WORKTREE_CONTEXT: &str = "VMUX HOST POLICY (mandatory): The selected workspace is a Git repository, but this tab is not isolated. Reading and inspection are allowed. Immediately before the first edit, write, test, build, or other mutation, call create_worktree. It reuses a known linked worktree, automatically uses one unambiguous existing worktree, or creates one when none exists. If it reports multiple candidates, ask the user with request_user_choice to choose an existing path or Create new worktree, then call create_worktree again with path or create=true. Never run git worktree add yourself.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AcpWorkspaceState {
    Bound,
    Unbound,
    PendingWorktree,
    RepositoryNeedsWorktree,
}

fn ancestor_acp_workspace_state(
    entity: Entity,
    child_of: &Query<&ChildOf>,
    tabs: &Query<&vmux_layout::tab::Tab>,
    workspaces: &Query<(), With<vmux_layout::tab::TabWorkspace>>,
    pending_projects: &Query<(), With<crate::plugin::PendingAgentProject>>,
    repositories_needing_worktrees: &Query<(), With<crate::plugin::RepositoryNeedsWorktree>>,
) -> Option<AcpWorkspaceState> {
    let mut current = entity;
    loop {
        if let Ok(tab) = tabs.get(current) {
            let state = match tab.startup_dir.as_deref() {
                Some(_) if repositories_needing_worktrees.contains(current) => {
                    AcpWorkspaceState::RepositoryNeedsWorktree
                }
                Some(_) => AcpWorkspaceState::Bound,
                None if workspaces.contains(current) => AcpWorkspaceState::Bound,
                None if pending_projects.contains(current) => AcpWorkspaceState::PendingWorktree,
                None => AcpWorkspaceState::Unbound,
            };
            return Some(state);
        }
        current = child_of.get(current).ok()?.parent();
    }
}

fn acp_prompt_context(
    handoff: Option<String>,
    workspace_state: Option<AcpWorkspaceState>,
) -> Option<String> {
    let policy = match workspace_state {
        Some(AcpWorkspaceState::Unbound) => Some(UNBOUND_WORKSPACE_CONTEXT),
        Some(AcpWorkspaceState::PendingWorktree) => Some(PENDING_WORKTREE_CONTEXT),
        Some(AcpWorkspaceState::RepositoryNeedsWorktree) => Some(REPOSITORY_WORKTREE_CONTEXT),
        Some(AcpWorkspaceState::Bound) | None => None,
    };
    match (handoff, policy) {
        (Some(handoff), Some(policy)) => Some(format!("{handoff}\n\n{policy}")),
        (Some(handoff), None) => Some(handoff),
        (None, Some(policy)) => Some(policy.to_string()),
        (None, None) => None,
    }
}

/// Marks a stack entity as an ACP agent session. vmux is ACP-only, so this is the agent
/// identity (there is no `AgentVariant`/`AgentKind` for ACP).
#[derive(Component, Clone, Debug)]
pub struct AcpSession {
    pub agent_id: String,
    pub sid: String,
    pub cwd: std::path::PathBuf,
    /// Ties this agent's vmux_mcp tool calls back to its pane (also set as a `ProcessId`
    /// component on the chat webview, where the tool router resolves it).
    pub anchor: vmux_core::ProcessId,
    /// The agent-assigned ACP session id to resume via `session/load` (from a restored
    /// `vmux://agent/<id>/<acp-session-id>` url). `None` opens a fresh session.
    pub resume: Option<String>,
}

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct AcpModelState {
    pub config_id: String,
    pub current_model_id: String,
    pub(crate) pending: Option<PendingAcpModelSelection>,
    pub models: Vec<vmux_service::protocol::AcpModelOption>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PendingAcpModelSelection {
    pub request_id: u64,
    pub model_id: String,
}

impl AcpModelState {
    pub fn display_model_id(&self) -> &str {
        self.pending
            .as_ref()
            .map(|pending| pending.model_id.as_str())
            .unwrap_or(&self.current_model_id)
    }

    pub fn current_name(&self) -> &str {
        self.models
            .iter()
            .find(|model| model.id == self.display_model_id())
            .map(|model| model.name.as_str())
            .unwrap_or_else(|| self.display_model_id())
    }
}

/// Progress, resolved launch spec, or terminal failure of a background agent install, keyed by
/// session id. The resolved spec is turned into `SpawnAcpAgent` on the ECS side (which owns the
/// non-clonable `ServiceClient`).
enum InstallMsg {
    Progress {
        sid: String,
        pct: Option<u8>,
        message: String,
    },
    Ready {
        sid: String,
        command: String,
        args: Vec<String>,
        env: Vec<(String, String)>,
    },
    Failed {
        sid: String,
        message: String,
    },
}

/// Carries background-install updates from install threads back onto the Bevy schedule.
#[derive(Resource)]
struct AcpInstallChannel {
    tx: Sender<InstallMsg>,
    rx: Receiver<InstallMsg>,
}

fn display_install_progress(
    phase: InstallPhase,
    pct: Option<u8>,
    message: &str,
) -> (Option<u8>, String) {
    if matches!(phase, InstallPhase::Done) {
        (None, "Starting agent…".to_string())
    } else {
        (pct, message.to_string())
    }
}

fn ready_agent_message(resume: Option<&str>) -> &'static str {
    if resume.is_some() {
        "Loading session history…"
    } else {
        "Starting agent…"
    }
}

impl Default for AcpInstallChannel {
    fn default() -> Self {
        let (tx, rx) = crossbeam_channel::unbounded();
        Self { tx, rx }
    }
}

/// The ACP registry catalog, fetched once at startup and read by the launcher snapshot to show
/// each agent's registry name + icon.
#[derive(Resource, Default)]
pub struct AcpCatalog {
    pub agents: Vec<crate::acp_registry::RegistryAgent>,
}

#[derive(Resource, Default)]
pub(crate) struct AcpInstallGeneration(u64);

impl AcpInstallGeneration {
    pub(crate) fn bump(&mut self) {
        self.0 = self.0.wrapping_add(1);
    }
}

/// One-shot receiver for the startup catalog fetch.
#[derive(Resource)]
struct AcpCatalogChannel {
    rx: Receiver<Vec<crate::acp_registry::RegistryAgent>>,
}

/// Kick a background thread that refreshes the registry (network, else cache) at startup.
fn start_catalog_fetch(mut commands: Commands) {
    let (tx, rx) = crossbeam_channel::unbounded();
    std::thread::spawn(move || {
        let agents = crate::acp_registry::fetch_blocking()
            .ok()
            .or_else(crate::acp_registry::load_cached)
            .map(|r| r.agents)
            .unwrap_or_default();
        let _ = tx.send(agents);
    });
    commands.insert_resource(AcpCatalogChannel { rx });
}

/// Move fetched catalog agents into the [`AcpCatalog`] resource when they arrive.
fn receive_catalog(channel: Option<Res<AcpCatalogChannel>>, mut catalog: ResMut<AcpCatalog>) {
    let Some(channel) = channel else {
        return;
    };
    if let Ok(agents) = channel.rx.try_recv() {
        catalog.agents = agents;
    }
}

pub struct AcpAgentPlugin;

impl Plugin for AcpAgentPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AcpInstallChannel>()
            .init_resource::<AcpCatalog>()
            .init_resource::<AcpInstallGeneration>()
            .add_message::<vmux_service::agent_events::PageAgentInfo>()
            .add_message::<vmux_service::agent_events::PageAgentWorkspaceChanged>()
            .add_message::<vmux_service::agent_events::PageAgentModelInfo>()
            .add_message::<vmux_service::agent_events::PageAgentModelSelectionResult>()
            .add_message::<vmux_service::agent_events::PageAgentSessionCreated>()
            .add_message::<vmux_service::agent_events::PageAgentAcpTerminalCreated>()
            .add_systems(Startup, start_catalog_fetch)
            .add_systems(
                Update,
                (
                    install_acp_session_when_focused,
                    send_acp_input,
                    drain_acp_installs,
                    receive_catalog,
                    apply_acp_agent_info,
                    apply_acp_workspace_changed,
                    (apply_acp_model_info, apply_acp_model_selection_result).chain(),
                    apply_acp_session_created,
                    apply_acp_terminal_created,
                ),
            )
            .add_observer(close_acp_session_on_remove)
            .add_observer(auto_allow_acp_approval);
    }
}

fn apply_acp_agent_info(
    mut reader: MessageReader<vmux_service::agent_events::PageAgentInfo>,
    mut sessions: Query<(&AcpSession, &mut vmux_core::team::Profile)>,
) {
    for event in reader.read() {
        let name = event.name.trim();
        if name.is_empty() {
            continue;
        }
        for (session, mut profile) in &mut sessions {
            if session.sid == event.sid && profile.name != name {
                *profile = vmux_core::team::Profile::registry(name, &session.agent_id);
            }
        }
    }
}

fn validate_acp_workspace(
    event: &vmux_service::agent_events::PageAgentWorkspaceChanged,
) -> Result<vmux_layout::worktree::ValidatedLinkedWorkspace, String> {
    vmux_layout::worktree::validate_linked_workspace(
        std::path::Path::new(&event.cwd),
        std::path::Path::new(&event.workspace_cwd),
        &event.branch,
    )
}

fn ancestor_tab(
    entity: Entity,
    child_of: &Query<&ChildOf>,
    tabs: &Query<(), With<vmux_layout::tab::Tab>>,
) -> Option<Entity> {
    let mut current = entity;
    loop {
        if tabs.contains(current) {
            return Some(current);
        }
        current = child_of.get(current).ok()?.parent();
    }
}

fn apply_acp_workspace_changed(
    mut reader: MessageReader<vmux_service::agent_events::PageAgentWorkspaceChanged>,
    mut sessions: Query<(Entity, &mut AcpSession)>,
    child_of: Query<&ChildOf>,
    tab_entities: Query<(), With<vmux_layout::tab::Tab>>,
    mut tabs: Query<&mut vmux_layout::tab::Tab>,
    mut workspaces: Query<&mut vmux_layout::tab::TabWorkspace>,
    managed: Query<&vmux_layout::tab::TabWorktree>,
    mut commands: Commands,
) {
    for event in reader.read() {
        let Ok(validated) = validate_acp_workspace(event) else {
            bevy::log::warn!(sid = %event.sid, "ignored invalid ACP worktree metadata");
            continue;
        };
        let cwd = validated.cwd;
        let workspace_cwd = validated.workspace_cwd;
        let checkout = validated.checkout;
        for (session_entity, mut session) in &mut sessions {
            if session.sid != event.sid {
                continue;
            }
            let Some(tab_entity) = ancestor_tab(session_entity, &child_of, &tab_entities) else {
                continue;
            };
            session.cwd.clone_from(&cwd);
            if let Ok(mut tab) = tabs.get_mut(tab_entity) {
                tab.startup_dir = Some(cwd.to_string_lossy().into_owned());
            }
            let workspace_project_dir = workspace_cwd.to_string_lossy().into_owned();
            if let Ok(mut workspace) = workspaces.get_mut(tab_entity) {
                workspace.project_dir.clone_from(&workspace_project_dir);
            } else {
                commands
                    .entity(tab_entity)
                    .insert(vmux_layout::tab::TabWorkspace {
                        project_dir: workspace_project_dir.clone(),
                    });
            }
            let keeps_managed = managed.get(tab_entity).ok().is_some_and(|metadata| {
                metadata.branch == event.branch
                    && std::path::Path::new(&metadata.checkout_dir)
                        .canonicalize()
                        .ok()
                        .as_ref()
                        == Some(&checkout.root)
            });
            let mut entity = commands.entity(tab_entity);
            entity
                .insert(vmux_layout::tab::TabDirDecided)
                .remove::<vmux_layout::tab::TabWorktreeUnavailable>();
            if !keeps_managed {
                entity
                    .remove::<vmux_layout::tab::TabWorktree>()
                    .remove::<vmux_layout::worktree::TabWorktreeReady>();
            } else if let Ok(ready) = vmux_layout::worktree::TabWorktreeReady::new(
                &cwd,
                &workspace_project_dir,
                managed.get(tab_entity).unwrap(),
                &checkout,
            ) {
                entity.insert(ready);
            } else {
                entity.remove::<vmux_layout::worktree::TabWorktreeReady>();
            }
        }
    }
}

fn apply_acp_model_info(
    mut reader: MessageReader<vmux_service::agent_events::PageAgentModelInfo>,
    mut sessions: Query<(Entity, &AcpSession, Option<&mut AcpModelState>)>,
    mut commands: Commands,
) {
    for event in reader.read() {
        for (entity, session, current) in &mut sessions {
            if session.sid != event.sid {
                continue;
            }
            if event.config_id.is_empty() || event.models.is_empty() {
                if current.is_some() {
                    commands.entity(entity).remove::<AcpModelState>();
                }
                continue;
            }
            if let Some(mut current) = current {
                let pending = current.pending.take();
                *current = AcpModelState {
                    config_id: event.config_id.clone(),
                    current_model_id: event.current_model_id.clone(),
                    pending,
                    models: event.models.clone(),
                };
            } else {
                commands.entity(entity).insert(AcpModelState {
                    config_id: event.config_id.clone(),
                    current_model_id: event.current_model_id.clone(),
                    pending: None,
                    models: event.models.clone(),
                });
            }
        }
    }
}

fn apply_acp_model_selection_result(
    mut reader: MessageReader<vmux_service::agent_events::PageAgentModelSelectionResult>,
    mut sessions: Query<(&AcpSession, &mut AcpModelState)>,
) {
    for event in reader.read() {
        for (session, mut state) in &mut sessions {
            if session.sid == event.sid
                && state.pending.as_ref().is_some_and(|pending| {
                    pending.request_id == event.request_id && pending.model_id == event.model_id
                })
            {
                if event.succeeded {
                    state.current_model_id.clone_from(&event.model_id);
                }
                state.pending = None;
            }
        }
    }
}

/// ACP agents re-request permission every time, so "allow always" must be answered by the host:
/// if the tool name is already in this session's auto-policy, reply `Allow` without prompting.
fn auto_allow_acp_approval(
    trigger: On<AgentApprovalRequest>,
    policies: Query<&AgentApprovalPolicy, With<AcpSession>>,
    mut commands: Commands,
) {
    let request = trigger.event();
    let Ok(policy) = policies.get(request.session) else {
        return;
    };
    if policy.auto.contains(&request.name) {
        commands.trigger(AgentApprovalReply {
            session: request.session,
            call_id: request.call_id.clone(),
            decision: ApprovalDecision::Allow,
        });
    }
}

/// Marks an `AcpSession` whose install has already been kicked off, so
/// [`install_acp_session_when_focused`] starts it exactly once.
#[derive(Component)]
pub(crate) struct AcpInstallStarted;

/// Install (and spawn) an ACP agent only once its stack is actually focused — i.e. the user
/// opened it. Background or restored agent tabs stay idle until visited, so vmux never installs
/// an agent the user hasn't looked at.
fn install_acp_session_when_focused(
    mut commands: Commands,
    mut q: Query<(Entity, &AcpSession, &mut AgentRunState), Without<AcpInstallStarted>>,
    focused: Option<Res<vmux_layout::stack::FocusedStack>>,
    settings: Option<Res<AppSettings>>,
    installs: Res<AcpInstallChannel>,
) {
    let Some(settings) = settings else {
        return;
    };
    let Some(focused) = focused else {
        return;
    };
    let shell = crate::plugin::agent_terminal_shell(&settings);
    for (entity, session, mut state) in &mut q {
        if focused.stack != Some(entity) {
            continue;
        }
        commands.entity(entity).insert(AcpInstallStarted);
        // `settings.agent.acp` is the override / escape hatch: a matching entry runs as-is if the
        // agent is absent from the registry (or unresolvable).
        let fallback = settings
            .agent
            .acp
            .iter()
            .find(|config| crate::acp_install::agent_ids_match(&config.id, &session.agent_id))
            .cloned();

        *state = AgentRunState::Installing {
            pct: None,
            message: "Preparing agent…".to_string(),
        };

        let sid = session.sid.clone();
        let agent_id = session.agent_id.clone();
        let progress = installs.tx.clone();
        let shell = shell.clone();

        std::thread::spawn(move || {
            let resolved =
                crate::acp_install::resolve_from_registry(&agent_id, |phase, pct, msg| {
                    let (pct, message) = display_install_progress(phase, pct, msg);
                    let _ = progress.send(InstallMsg::Progress {
                        sid: sid.clone(),
                        pct,
                        message,
                    });
                });
            let login_env = vmux_terminal::shell_env::login_shell_env(&shell);
            let msg = match resolved {
                Ok(r) => InstallMsg::Ready {
                    sid,
                    command: r.command,
                    args: r.args,
                    env: apply_agent_compatibility_env(
                        &agent_id,
                        build_agent_env(r.env, login_env, r.path_prepend),
                    ),
                },
                Err(reg_err) => match fallback {
                    Some(cfg) => InstallMsg::Ready {
                        sid,
                        command: cfg.command,
                        args: cfg.args,
                        env: apply_agent_compatibility_env(
                            &agent_id,
                            build_agent_env(cfg.env, login_env, None),
                        ),
                    },
                    None => InstallMsg::Failed {
                        sid,
                        message: reg_err,
                    },
                },
            };
            let _ = progress.send(msg);
        });
    }
}

/// Prepend a managed runtime `bin/` to the child's `PATH` (so e.g. `npx` finds its `node`). Prefers
/// the `PATH` already assembled in `env` (the login-shell `PATH` merged by [`build_agent_env`]),
/// falling back to this process's `PATH` only when `env` has none.
fn apply_path_prepend(
    mut env: Vec<(String, String)>,
    prepend: Option<String>,
) -> Vec<(String, String)> {
    if let Some(dir) = prepend {
        let existing = env
            .iter()
            .find(|(k, _)| k == "PATH")
            .map(|(_, v)| v.clone())
            .or_else(|| std::env::var("PATH").ok())
            .filter(|s| !s.is_empty());
        let full = match existing {
            Some(existing) => format!("{dir}:{existing}"),
            None => dir,
        };
        env.retain(|(k, _)| k != "PATH");
        env.push(("PATH".to_string(), full));
    }
    env
}

/// Keep only the last occurrence of each key, preserving order — so the login-shell env (appended
/// last) wins over the registry/config base for any shared key.
fn dedup_env_keep_last(env: &mut Vec<(String, String)>) {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::with_capacity(env.len());
    for (key, value) in std::mem::take(env).into_iter().rev() {
        if seen.insert(key.clone()) {
            out.push((key, value));
        }
    }
    out.reverse();
    *env = out;
}

/// Assemble an ACP agent's spawn environment. The registry/config `base` is the floor; the captured
/// login-shell env is layered on top so the user's exported API keys and real `PATH` reach the
/// agent even when vmux was launched from Finder/launchd (which hands the daemon a minimal
/// environment) rather than from a shell; finally the managed runtime `bin/` is prepended to the
/// resulting `PATH`. Without this an ACP agent authenticating via an env-var API key reports
/// "Authentication required" in release builds while working under `make` (where the daemon
/// inherits the launching shell's environment). Mirrors the terminal's agent-spawn merge.
fn build_agent_env(
    mut base: Vec<(String, String)>,
    login_env: &[(String, String)],
    path_prepend: Option<String>,
) -> Vec<(String, String)> {
    base.extend(login_env.iter().cloned());
    dedup_env_keep_last(&mut base);
    apply_path_prepend(base, path_prepend)
}

fn apply_agent_compatibility_env(
    agent_id: &str,
    env: Vec<(String, String)>,
) -> Vec<(String, String)> {
    match crate::acp_install::registry_id_alias(agent_id) {
        "mistral-vibe" => apply_vibe_compatibility_env(env),
        "codex-acp" => apply_codex_compatibility_env(env),
        "claude-acp" => apply_claude_compatibility_env(env),
        _ => env,
    }
}

fn apply_claude_compatibility_env(mut env: Vec<(String, String)>) -> Vec<(String, String)> {
    env.retain(|(key, _)| key != "MCP_TOOL_TIMEOUT");
    env.push((
        "MCP_TOOL_TIMEOUT".to_string(),
        (crate::mcp::LONG_MCP_TOOL_TIMEOUT_SECS * 1_000).to_string(),
    ));
    env
}

fn apply_vibe_compatibility_env(mut env: Vec<(String, String)>) -> Vec<(String, String)> {
    let mut disabled = Vec::new();
    if let Some(value) = env
        .iter()
        .rev()
        .find(|(key, _)| key == "VIBE_DISABLED_TOOLS")
        .map(|(_, value)| value)
    {
        match serde_json::from_str::<Vec<String>>(value) {
            Ok(existing) => extend_unique(&mut disabled, existing),
            Err(err) => bevy::log::warn!(
                "acp: existing VIBE_DISABLED_TOOLS is invalid JSON ({err}); discarding it"
            ),
        }
    }
    extend_unique(&mut disabled, ["bash".to_string()]);
    env.retain(|(key, _)| key != "VIBE_DISABLED_TOOLS");
    env.push((
        "VIBE_DISABLED_TOOLS".to_string(),
        serde_json::to_string(&disabled).unwrap(),
    ));
    let mut mcp_servers: Vec<serde_json::Value> = Vec::new();
    if let Some(value) = env
        .iter()
        .rev()
        .find(|(key, _)| key == "VIBE_MCP_SERVERS")
        .map(|(_, value)| value)
    {
        match serde_json::from_str::<Vec<serde_json::Value>>(value) {
            Ok(existing) => {
                for server in existing {
                    if let Some(name) = server.get("name").and_then(serde_json::Value::as_str) {
                        mcp_servers.retain(|candidate| {
                            candidate.get("name").and_then(serde_json::Value::as_str) != Some(name)
                        });
                    }
                    mcp_servers.push(server);
                }
            }
            Err(err) => bevy::log::warn!(
                "acp: existing VIBE_MCP_SERVERS is invalid JSON ({err}); discarding it"
            ),
        }
    }
    env.retain(|(key, _)| key != "VIBE_MCP_SERVERS");
    if !mcp_servers.is_empty() {
        env.push((
            "VIBE_MCP_SERVERS".to_string(),
            serde_json::to_string(&mcp_servers).unwrap(),
        ));
    }
    env
}

fn extend_unique(out: &mut Vec<String>, values: impl IntoIterator<Item = String>) {
    for value in values {
        if !out.contains(&value) {
            out.push(value);
        }
    }
}

fn apply_codex_compatibility_env(mut env: Vec<(String, String)>) -> Vec<(String, String)> {
    let existing = env
        .iter()
        .rev()
        .find(|(key, _)| key == "CODEX_CONFIG")
        .map(|(_, value)| value.as_str());
    let (mut config, warning) = parse_codex_config(existing);
    if let Some(warning) = warning {
        bevy::log::warn!("{warning}");
    }

    config.insert(
        "approvals_reviewer".to_string(),
        serde_json::Value::String("user".to_string()),
    );

    let features = config
        .entry("features")
        .or_insert_with(|| serde_json::json!({}));
    if !features.is_object() {
        *features = serde_json::json!({});
    }
    let features = features.as_object_mut().unwrap();
    features.insert("shell_tool".to_string(), serde_json::Value::Bool(false));
    features.insert("unified_exec".to_string(), serde_json::Value::Bool(false));
    let code_mode = features
        .entry("code_mode")
        .or_insert_with(|| serde_json::json!({}));
    if !code_mode.is_object() {
        *code_mode = serde_json::json!({});
    }
    code_mode.as_object_mut().unwrap().insert(
        "direct_only_tool_namespaces".to_string(),
        serde_json::json!([crate::client::cli::codex::DIRECT_ONLY_NAMESPACE]),
    );

    let tools = config
        .entry("tools")
        .or_insert_with(|| serde_json::json!({}));
    if !tools.is_object() {
        *tools = serde_json::json!({});
    }
    tools
        .as_object_mut()
        .unwrap()
        .insert("web_search".to_string(), serde_json::Value::Bool(false));

    let mcp_servers = config
        .entry("mcp_servers")
        .or_insert_with(|| serde_json::json!({}));
    if !mcp_servers.is_object() {
        *mcp_servers = serde_json::json!({});
    }
    let vmux = mcp_servers
        .as_object_mut()
        .unwrap()
        .entry("vmux")
        .or_insert_with(|| serde_json::json!({}));
    if !vmux.is_object() {
        *vmux = serde_json::json!({});
    }
    vmux.as_object_mut().unwrap().insert(
        "tool_timeout_sec".to_string(),
        serde_json::json!(crate::mcp::LONG_MCP_TOOL_TIMEOUT_SECS),
    );

    let instructions = config
        .get("developer_instructions")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let instructions = if instructions.contains("mcp__vmux__run") {
        instructions.to_string()
    } else if instructions.is_empty() {
        crate::client::cli::codex::RUN_STEER_PROMPT.to_string()
    } else {
        format!(
            "{instructions}\n\n{}",
            crate::client::cli::codex::RUN_STEER_PROMPT
        )
    };
    let instructions = vmux_core::knowledge::append_agent_context(&instructions);
    config.insert(
        "developer_instructions".to_string(),
        serde_json::Value::String(instructions),
    );

    env.retain(|(key, _)| key != "CODEX_CONFIG");
    env.push((
        "CODEX_CONFIG".to_string(),
        serde_json::Value::Object(config).to_string(),
    ));
    env
}

fn parse_codex_config(
    value: Option<&str>,
) -> (serde_json::Map<String, serde_json::Value>, Option<String>) {
    let Some(value) = value else {
        return (serde_json::Map::new(), None);
    };
    match serde_json::from_str::<serde_json::Value>(value) {
        Ok(serde_json::Value::Object(config)) => (config, None),
        Ok(value) => {
            let kind = match value {
                serde_json::Value::Null => "null",
                serde_json::Value::Bool(_) => "boolean",
                serde_json::Value::Number(_) => "number",
                serde_json::Value::String(_) => "string",
                serde_json::Value::Array(_) => "array",
                serde_json::Value::Object(_) => unreachable!(),
            };
            (
                serde_json::Map::new(),
                Some(format!(
                    "acp: existing CODEX_CONFIG is not a JSON object ({kind}); discarding it"
                )),
            )
        }
        Err(err) => (
            serde_json::Map::new(),
            Some(format!(
                "acp: existing CODEX_CONFIG is invalid JSON ({err}); discarding it"
            )),
        ),
    }
}

/// Drain background-install updates: reflect progress/failure onto the session run-state, and on
/// a resolved spec send `SpawnAcpAgent` (success run-state is then driven by the daemon stream).
fn drain_acp_installs(
    installs: Res<AcpInstallChannel>,
    service: Option<Res<ServiceClient>>,
    mut install_generation: ResMut<AcpInstallGeneration>,
    mut q: Query<(&AcpSession, &mut AgentRunState)>,
) {
    while let Ok(msg) = installs.rx.try_recv() {
        match msg {
            InstallMsg::Progress { sid, pct, message } => {
                for (session, mut state) in &mut q {
                    if session.sid == sid && matches!(*state, AgentRunState::Installing { .. }) {
                        *state = AgentRunState::Installing {
                            pct,
                            message: message.clone(),
                        };
                    }
                }
            }
            InstallMsg::Failed { sid, message } => {
                for (session, mut state) in &mut q {
                    if session.sid == sid {
                        *state = AgentRunState::Errored(message.clone());
                    }
                }
            }
            InstallMsg::Ready {
                sid,
                command,
                args,
                env,
            } => {
                install_generation.bump();
                let Some(service) = service.as_ref() else {
                    continue;
                };
                if let Some((session, mut state)) = q.iter_mut().find(|(s, _)| s.sid == sid) {
                    *state = AgentRunState::Installing {
                        pct: None,
                        message: ready_agent_message(session.resume.as_deref()).to_string(),
                    };
                    let mcp = crate::mcp::resolve_acp(
                        &session.cwd,
                        session.anchor,
                        &session.agent_id,
                    )
                    .inspect_err(|err| {
                        bevy::log::warn!(
                            "acp: vmux_mcp sidecar unresolved; agent runs without vmux tools: {err}"
                        );
                    })
                    .ok();
                    service.0.send(ClientMessage::SpawnAcpAgent {
                        sid,
                        agent_id: session.agent_id.clone(),
                        command,
                        args,
                        env,
                        cwd: session.cwd.to_string_lossy().into_owned(),
                        anchor: session.anchor,
                        mcp_command: mcp.as_ref().map(|m| m.command.clone()),
                        mcp_args: mcp.map(|m| m.args).unwrap_or_default(),
                        resume_acp_session_id: session.resume.clone(),
                    });
                }
            }
        }
    }
}

/// When the daemon reports the agent-assigned ACP session id, redirect the pane url to
/// `vmux://agent/<id>/<acp_session_id>` (the persisted resume handle) and record it on the session
/// so a later reopen resumes via `session/load`.
#[allow(clippy::type_complexity)]
fn apply_acp_session_created(
    mut reader: MessageReader<vmux_service::agent_events::PageAgentSessionCreated>,
    mut sessions: Query<
        (
            Entity,
            &mut AcpSession,
            &mut vmux_core::PageMetadata,
            Option<&ImportedConversation>,
        ),
        Without<vmux_layout::Browser>,
    >,
    children: Query<&Children>,
    mut browser_meta: Query<&mut vmux_core::PageMetadata, With<vmux_layout::Browser>>,
) {
    for ev in reader.read() {
        for (stack, mut session, mut stack_meta, imported) in &mut sessions {
            if session.sid != ev.sid {
                continue;
            }
            session.resume = Some(ev.acp_session_id.clone());
            if let Some(imported) = imported
                && imported.first_prompt.is_some()
                && let Err(err) =
                    crate::handoff::save(&session.agent_id, &ev.acp_session_id, imported)
            {
                bevy::log::warn!("acp: failed to persist handoff metadata: {err}");
            }
            let url = format!("vmux://agent/{}/{}", session.agent_id, ev.acp_session_id);
            // The stack's PageMetadata is what persists (space.ron) so a restart can resume.
            if stack_meta.url != url {
                stack_meta.url = url.clone();
            }
            // The child Browser's PageMetadata is what the tab strip + address bar read.
            if let Ok(kids) = children.get(stack) {
                for kid in kids.iter() {
                    if let Ok(mut meta) = browser_meta.get_mut(kid)
                        && meta.url != url
                    {
                        meta.url = url.clone();
                    }
                }
            }
        }
    }
}

/// An ACP agent created a terminal (`terminal/create`): the daemon already spawned the PTY, so open
/// a visible pane beside the agent and **attach** it to `process_id` (never create a second PTY).
/// Reuses an existing terminal region when present (stacks over splits) and keeps keyboard focus on
/// the agent.
#[allow(clippy::too_many_arguments)]
fn apply_acp_terminal_created(
    mut reader: MessageReader<vmux_service::agent_events::PageAgentAcpTerminalCreated>,
    sessions: Query<(Entity, &AcpSession)>,
    ctx: PlacementCtx,
    mut meshes: ResMut<Assets<Mesh>>,
    mut webview_mt: ResMut<Assets<WebviewExtendStandardMaterial>>,
    mut commands: Commands,
) {
    let mut split_batch = std::collections::HashSet::new();
    for ev in reader.read() {
        let Some(stack) = sessions
            .iter()
            .find(|(_, session)| session.sid == ev.sid)
            .map(|(entity, _)| entity)
        else {
            continue;
        };
        let Ok(agent_pane) = ctx.child_of_q.get(stack).map(|child_of| child_of.parent()) else {
            continue;
        };
        let target_pane = resolve_spiral_pane(
            &mut commands,
            agent_pane,
            TERMINAL_PAGE_URL,
            false,
            &mut split_batch,
            &ctx,
        );
        let tab = commands
            .spawn((stack_bundle(), LastActivatedAt(0), ChildOf(target_pane)))
            .id();
        commands.spawn((
            reattach_terminal_bundle(&mut meshes, &mut webview_mt, ev.process_id),
            vmux_terminal::RetainOnProcessExit,
            ChildOf(tab),
        ));
    }
}

fn send_acp_input(
    mut q: Query<(
        Entity,
        &AcpSession,
        &mut AgentRunState,
        &mut PromptQueue,
        Has<AcpInstallStarted>,
        Option<&mut PendingHandoff>,
        Option<&mut ImportedConversation>,
    )>,
    child_of: Query<&ChildOf>,
    tabs: Query<&vmux_layout::tab::Tab>,
    workspaces: Query<(), With<vmux_layout::tab::TabWorkspace>>,
    pending_projects: Query<(), With<crate::plugin::PendingAgentProject>>,
    repositories_needing_worktrees: Query<(), With<crate::plugin::RepositoryNeedsWorktree>>,
    service: Option<Res<ServiceClient>>,
) {
    let Some(service) = service else {
        return;
    };
    for (entity, session, mut state, mut queue, install_started, mut pending, mut imported) in
        &mut q
    {
        if !acp_prompt_dispatch_ready(&state, &queue, install_started) {
            continue;
        }
        let Some(prompt) = queue.take_next() else {
            continue;
        };
        let text = prompt.text;
        let handoff = pending
            .as_deref_mut()
            .and_then(PendingHandoff::context_for_send);
        if handoff.is_some()
            && let Some(imported) = imported.as_deref_mut()
            && imported.first_prompt.is_none()
        {
            imported.first_prompt = Some(text.clone());
        }
        let workspace_state = ancestor_acp_workspace_state(
            entity,
            &child_of,
            &tabs,
            &workspaces,
            &pending_projects,
            &repositories_needing_worktrees,
        );
        let context = acp_prompt_context(handoff, workspace_state);
        service.0.send(ClientMessage::agent_input(
            session.sid.clone(),
            text,
            context,
            prompt.attachments,
        ));
        *state = AgentRunState::Streaming;
    }
}

fn acp_prompt_dispatch_ready(
    state: &AgentRunState,
    queue: &PromptQueue,
    install_started: bool,
) -> bool {
    install_started && queue.ready(matches!(state, AgentRunState::Idle))
}

fn close_acp_session_on_remove(
    trigger: On<Remove, AcpSession>,
    sessions: Query<&AcpSession>,
    service: Option<Res<ServiceClient>>,
) {
    let Some(service) = service else {
        return;
    };
    let Ok(session) = sessions.get(trigger.event_target()) else {
        return;
    };
    service.0.send(ClientMessage::ClosePageAgent {
        sid: session.sid.clone(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(k: &str, v: &str) -> (String, String) {
        (k.to_string(), v.to_string())
    }

    #[test]
    fn queued_prompt_waits_for_acp_install_start() {
        let mut queue = PromptQueue::default();
        queue.enqueue("hello".to_string());

        assert!(!acp_prompt_dispatch_ready(
            &AgentRunState::Idle,
            &queue,
            false
        ));
        assert!(acp_prompt_dispatch_ready(
            &AgentRunState::Idle,
            &queue,
            true
        ));
        assert!(!acp_prompt_dispatch_ready(
            &AgentRunState::Installing {
                pct: None,
                message: "Preparing agent…".to_string(),
            },
            &queue,
            true
        ));
    }

    #[test]
    fn unbound_workspace_context_requires_picker_before_project_work() {
        let context = acp_prompt_context(None, Some(AcpWorkspaceState::Unbound)).unwrap();

        assert!(context.contains("select_workspace"));
        assert!(context.contains("Any directory can be a workspace"));
        assert!(context.contains("initialize Git"));
        assert!(context.contains("Do not search the user's home directory"));
        assert!(context.contains("folder picker"));
    }

    #[test]
    fn repository_context_defers_worktree_until_mutation() {
        let context =
            acp_prompt_context(None, Some(AcpWorkspaceState::RepositoryNeedsWorktree)).unwrap();

        assert!(context.contains("Reading and inspection are allowed"));
        assert!(context.contains("Immediately before the first edit"));
        assert!(context.contains("create_worktree"));
        assert!(context.contains("request_user_choice"));
        assert!(context.contains("Never run git worktree add"));
    }

    #[test]
    fn pending_worktree_context_requires_waiting_for_activation() {
        let context = acp_prompt_context(
            Some("prior conversation".into()),
            Some(AcpWorkspaceState::PendingWorktree),
        )
        .unwrap();

        assert!(context.starts_with("prior conversation\n\n"));
        assert!(context.contains("activation is pending"));
        assert!(context.contains("Wait for vmux"));
        assert!(context.contains("before inspecting"));
    }

    #[test]
    fn bound_workspace_keeps_only_handoff_context() {
        assert_eq!(
            acp_prompt_context(
                Some("prior conversation".into()),
                Some(AcpWorkspaceState::Bound),
            )
            .as_deref(),
            Some("prior conversation")
        );
    }

    #[test]
    fn ancestor_workspace_state_tracks_pending_and_bound_tab() {
        use bevy::ecs::system::RunSystemOnce;

        let mut app = App::new();
        let tab = app
            .world_mut()
            .spawn(vmux_layout::tab::Tab {
                name: "Tab 1".into(),
                startup_dir: None,
            })
            .id();
        let stack = app.world_mut().spawn(ChildOf(tab)).id();
        let state = |world: &mut World| {
            world
                .run_system_once(
                    move |child_of: Query<&ChildOf>,
                          tabs: Query<&vmux_layout::tab::Tab>,
                          workspaces: Query<(), With<vmux_layout::tab::TabWorkspace>>,
                          pending: Query<(), With<crate::plugin::PendingAgentProject>>,
                          needs_worktree: Query<
                        (),
                        With<crate::plugin::RepositoryNeedsWorktree>,
                    >| {
                        ancestor_acp_workspace_state(
                            stack,
                            &child_of,
                            &tabs,
                            &workspaces,
                            &pending,
                            &needs_worktree,
                        )
                    },
                )
                .unwrap()
        };

        assert_eq!(state(app.world_mut()), Some(AcpWorkspaceState::Unbound));
        app.world_mut()
            .entity_mut(tab)
            .insert(crate::plugin::PendingAgentProject("/repo".into()));
        assert_eq!(
            state(app.world_mut()),
            Some(AcpWorkspaceState::PendingWorktree)
        );
        app.world_mut().entity_mut(tab).insert((
            vmux_layout::tab::Tab {
                name: "Tab 1".into(),
                startup_dir: Some("/repo".into()),
            },
            crate::plugin::RepositoryNeedsWorktree,
        ));
        assert_eq!(
            state(app.world_mut()),
            Some(AcpWorkspaceState::RepositoryNeedsWorktree)
        );
        app.world_mut()
            .entity_mut(tab)
            .insert(vmux_layout::tab::TabWorkspace {
                project_dir: "/repo".into(),
            })
            .remove::<crate::plugin::RepositoryNeedsWorktree>();
        assert_eq!(state(app.world_mut()), Some(AcpWorkspaceState::Bound));
    }

    #[test]
    fn acp_workspace_update_rebinds_only_matching_tab() {
        let repo = tempfile::tempdir().unwrap();
        let git = |args: &[&str]| {
            let status = std::process::Command::new("git")
                .current_dir(repo.path())
                .args(args)
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .env("GIT_CONFIG_SYSTEM", "/dev/null")
                .env_remove("GIT_DIR")
                .env_remove("GIT_WORK_TREE")
                .status()
                .unwrap();
            assert!(status.success(), "git {args:?} failed");
        };
        git(&["init", "-q", "-b", "main"]);
        git(&["config", "user.email", "t@example.com"]);
        git(&["config", "user.name", "Test"]);
        git(&["config", "commit.gpgsign", "false"]);
        std::fs::write(repo.path().join("seed.txt"), "seed\n").unwrap();
        git(&["add", "seed.txt"]);
        git(&["commit", "-qm", "init"]);
        let worktree_parent = tempfile::tempdir().unwrap();
        let worktree = worktree_parent.path().join("quiet-amber-wolf");
        vmux_git::worktree::worktree_add(repo.path(), &worktree, "vibe/quiet-amber-wolf", "main")
            .unwrap();
        let project_dir = repo.path().canonicalize().unwrap();
        let worktree_dir = worktree.canonicalize().unwrap();
        let mut app = App::new();
        app.add_message::<vmux_service::agent_events::PageAgentWorkspaceChanged>()
            .add_systems(Update, apply_acp_workspace_changed);
        let tab = app
            .world_mut()
            .spawn((
                vmux_layout::tab::Tab {
                    name: "matching".into(),
                    startup_dir: Some(project_dir.to_string_lossy().into_owned()),
                },
                vmux_layout::tab::TabWorkspace {
                    project_dir: project_dir.to_string_lossy().into_owned(),
                },
            ))
            .id();
        let session = app
            .world_mut()
            .spawn((
                AcpSession {
                    agent_id: "mistral-vibe".into(),
                    sid: "matching-sid".into(),
                    cwd: project_dir.clone(),
                    anchor: vmux_core::ProcessId::new(),
                    resume: None,
                },
                ChildOf(tab),
            ))
            .id();
        let unrelated_tab = app
            .world_mut()
            .spawn(vmux_layout::tab::Tab {
                name: "unrelated".into(),
                startup_dir: Some(project_dir.to_string_lossy().into_owned()),
            })
            .id();
        app.world_mut()
            .resource_mut::<Messages<vmux_service::agent_events::PageAgentWorkspaceChanged>>()
            .write(vmux_service::agent_events::PageAgentWorkspaceChanged {
                sid: "matching-sid".into(),
                name: "quiet-amber-wolf".into(),
                branch: "vibe/quiet-amber-wolf".into(),
                cwd: worktree_dir.to_string_lossy().into_owned(),
                workspace_cwd: project_dir.to_string_lossy().into_owned(),
            });

        app.update();

        assert_eq!(
            app.world().get::<AcpSession>(session).unwrap().cwd,
            worktree_dir
        );
        assert_eq!(
            app.world()
                .get::<vmux_layout::tab::Tab>(tab)
                .unwrap()
                .startup_dir
                .as_deref(),
            Some(worktree_dir.to_string_lossy().as_ref())
        );
        assert_eq!(
            app.world()
                .get::<vmux_layout::tab::Tab>(unrelated_tab)
                .unwrap()
                .startup_dir
                .as_deref(),
            Some(project_dir.to_string_lossy().as_ref())
        );
    }

    #[test]
    fn login_env_reaches_agent_and_overrides_base() {
        let base = vec![s("MISTRAL_API_KEY", ""), s("KEEP", "1")];
        let login = vec![s("MISTRAL_API_KEY", "real-key"), s("PATH", "/login/bin")];
        let env = build_agent_env(base, &login, None);
        assert!(
            env.contains(&s("MISTRAL_API_KEY", "real-key")),
            "login-shell API key must win over the empty registry value: {env:?}"
        );
        assert!(env.contains(&s("KEEP", "1")));
        assert!(env.contains(&s("PATH", "/login/bin")));
    }

    #[test]
    fn managed_bin_prepends_to_login_path_not_process_path() {
        let login = vec![s("PATH", "/login/bin")];
        let env = build_agent_env(Vec::new(), &login, Some("/managed/node/bin".to_string()));
        let path = env
            .iter()
            .find(|(k, _)| k == "PATH")
            .map(|(_, v)| v.as_str());
        assert_eq!(path, Some("/managed/node/bin:/login/bin"));
    }

    #[test]
    fn apply_path_prepend_prefers_env_path_over_process() {
        let env = apply_path_prepend(vec![s("PATH", "/from/login")], Some("/managed".to_string()));
        assert_eq!(
            env.iter()
                .find(|(k, _)| k == "PATH")
                .map(|(_, v)| v.as_str()),
            Some("/managed:/from/login")
        );
    }

    #[test]
    fn completed_install_progress_describes_agent_startup() {
        assert_eq!(
            display_install_progress(InstallPhase::Done, Some(100), "ready"),
            (None, "Starting agent…".to_string())
        );
        assert_eq!(
            display_install_progress(InstallPhase::Downloading, Some(42), "downloading"),
            (Some(42), "downloading".to_string())
        );
        assert_eq!(ready_agent_message(None), "Starting agent…");
        assert_eq!(
            ready_agent_message(Some("session-1")),
            "Loading session history…"
        );
    }

    #[test]
    fn live_acp_identity_updates_only_matching_profile() {
        use vmux_core::team::Profile;
        use vmux_service::agent_events::PageAgentInfo;

        let mut app = App::new();
        app.add_plugins(bevy::app::TaskPoolPlugin::default())
            .add_plugins(AcpAgentPlugin)
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<WebviewExtendStandardMaterial>>();
        let matching = app
            .world_mut()
            .spawn((
                AcpSession {
                    agent_id: "antigravity".into(),
                    sid: "s1".into(),
                    cwd: "/tmp".into(),
                    anchor: vmux_core::ProcessId::new(),
                    resume: None,
                },
                Profile::registry("Configured", "antigravity"),
            ))
            .id();
        let unrelated = app
            .world_mut()
            .spawn((
                AcpSession {
                    agent_id: "claude".into(),
                    sid: "s2".into(),
                    cwd: "/tmp".into(),
                    anchor: vmux_core::ProcessId::new(),
                    resume: None,
                },
                Profile::registry("Claude", "claude"),
            ))
            .id();

        app.world_mut().write_message(PageAgentInfo {
            sid: "s1".into(),
            name: "Antigravity".into(),
        });
        app.update();

        assert_eq!(
            app.world().get::<Profile>(matching).unwrap().name,
            "Antigravity"
        );
        assert_eq!(
            app.world().get::<Profile>(unrelated).unwrap().name,
            "Claude"
        );

        app.world_mut().write_message(PageAgentInfo {
            sid: "s1".into(),
            name: "   ".into(),
        });
        app.update();

        assert_eq!(
            app.world().get::<Profile>(matching).unwrap().name,
            "Antigravity"
        );
    }

    #[test]
    fn live_acp_model_info_updates_only_matching_session() {
        use vmux_service::agent_events::PageAgentModelInfo;
        use vmux_service::protocol::AcpModelOption;

        let mut app = App::new();
        app.add_plugins(bevy::app::TaskPoolPlugin::default())
            .add_plugins(AcpAgentPlugin)
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<WebviewExtendStandardMaterial>>();
        let matching = app
            .world_mut()
            .spawn(AcpSession {
                agent_id: "claude".into(),
                sid: "s1".into(),
                cwd: "/tmp".into(),
                anchor: vmux_core::ProcessId::new(),
                resume: None,
            })
            .id();
        let unrelated = app
            .world_mut()
            .spawn(AcpSession {
                agent_id: "codex".into(),
                sid: "s2".into(),
                cwd: "/tmp".into(),
                anchor: vmux_core::ProcessId::new(),
                resume: None,
            })
            .id();

        app.world_mut().write_message(PageAgentModelInfo {
            sid: "s1".into(),
            config_id: "model".into(),
            current_model_id: "sonnet".into(),
            models: vec![AcpModelOption {
                id: "sonnet".into(),
                name: "Claude Sonnet".into(),
                description: None,
            }],
        });
        app.update();

        let state = app.world().get::<AcpModelState>(matching).unwrap();
        assert_eq!(state.current_name(), "Claude Sonnet");
        assert!(state.pending.is_none());
        assert!(app.world().get::<AcpModelState>(unrelated).is_none());
    }

    #[test]
    fn model_results_preserve_latest_pending_selection() {
        use vmux_service::agent_events::{PageAgentModelInfo, PageAgentModelSelectionResult};
        use vmux_service::protocol::AcpModelOption;

        let models = vec![
            AcpModelOption {
                id: "default".into(),
                name: "Default".into(),
                description: None,
            },
            AcpModelOption {
                id: "opus".into(),
                name: "Opus".into(),
                description: None,
            },
            AcpModelOption {
                id: "fable".into(),
                name: "Fable".into(),
                description: None,
            },
        ];
        let mut app = App::new();
        app.add_message::<PageAgentModelInfo>()
            .add_message::<PageAgentModelSelectionResult>()
            .add_systems(
                Update,
                (apply_acp_model_info, apply_acp_model_selection_result).chain(),
            );
        let entity = app
            .world_mut()
            .spawn((
                AcpSession {
                    agent_id: "claude".into(),
                    sid: "s1".into(),
                    cwd: "/tmp".into(),
                    anchor: vmux_core::ProcessId::new(),
                    resume: None,
                },
                AcpModelState {
                    config_id: "model".into(),
                    current_model_id: "default".into(),
                    pending: Some(PendingAcpModelSelection {
                        request_id: 2,
                        model_id: "fable".into(),
                    }),
                    models: models.clone(),
                },
            ))
            .id();

        app.world_mut().write_message(PageAgentModelInfo {
            sid: "s1".into(),
            config_id: "model".into(),
            current_model_id: "opus".into(),
            models: models.clone(),
        });
        app.update();

        let state = app.world().get::<AcpModelState>(entity).unwrap();
        assert_eq!(state.current_model_id, "opus");
        assert_eq!(
            state.pending.as_ref().map(|pending| pending.request_id),
            Some(2)
        );
        assert_eq!(state.current_name(), "Fable");

        app.world_mut()
            .write_message(PageAgentModelSelectionResult {
                sid: "s1".into(),
                request_id: 1,
                model_id: "fable".into(),
                succeeded: false,
            });
        app.update();
        assert_eq!(
            app.world()
                .get::<AcpModelState>(entity)
                .unwrap()
                .pending
                .as_ref()
                .map(|pending| pending.request_id),
            Some(2)
        );

        app.world_mut()
            .write_message(PageAgentModelSelectionResult {
                sid: "s1".into(),
                request_id: 2,
                model_id: "fable".into(),
                succeeded: false,
            });
        app.update();
        let state = app.world().get::<AcpModelState>(entity).unwrap();
        assert!(state.pending.is_none());
        assert_eq!(state.current_name(), "Opus");

        {
            let mut state = app.world_mut().get_mut::<AcpModelState>(entity).unwrap();
            state.pending = Some(PendingAcpModelSelection {
                request_id: 3,
                model_id: "fable".into(),
            });
        }
        app.world_mut()
            .write_message(PageAgentModelSelectionResult {
                sid: "s1".into(),
                request_id: 3,
                model_id: "fable".into(),
                succeeded: true,
            });
        app.update();
        let state = app.world().get::<AcpModelState>(entity).unwrap();
        assert_eq!(state.current_model_id, "fable");
        assert!(state.pending.is_none());
    }

    #[test]
    fn acp_terminal_stack_does_not_take_focus_from_agent() {
        use vmux_layout::pane::leaf_pane_bundle;
        use vmux_layout::stack::Stack;
        use vmux_layout::tab::tab_bundle;
        use vmux_service::agent_events::PageAgentAcpTerminalCreated;

        let mut app = App::new();
        app.add_message::<PageAgentAcpTerminalCreated>()
            .add_systems(Update, apply_acp_terminal_created)
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<WebviewExtendStandardMaterial>>();
        let tab = app.world_mut().spawn(tab_bundle()).id();
        let pane = app
            .world_mut()
            .spawn((leaf_pane_bundle(), ChildOf(tab)))
            .id();
        let agent = app
            .world_mut()
            .spawn((
                stack_bundle(),
                LastActivatedAt(10),
                ChildOf(pane),
                AcpSession {
                    agent_id: "claude".into(),
                    sid: "s1".into(),
                    cwd: "/tmp".into(),
                    anchor: vmux_core::ProcessId::new(),
                    resume: None,
                },
            ))
            .id();
        app.world_mut()
            .entity_mut(agent)
            .insert(vmux_core::PageMetadata {
                url: "vmux://agent/claude".into(),
                ..default()
            });
        app.world_mut().write_message(PageAgentAcpTerminalCreated {
            sid: "s1".into(),
            terminal_id: "terminal-1".into(),
            process_id: vmux_core::ProcessId::new(),
            command: "echo".into(),
            args: vec!["hi".into()],
            cwd: Some("/tmp".into()),
        });

        app.update();

        let stack_times = {
            let world = app.world_mut();
            let mut query = world.query_filtered::<(Entity, &LastActivatedAt), With<Stack>>();
            query
                .iter(world)
                .map(|(entity, activated)| (entity, activated.0))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            stack_times
                .iter()
                .find(|(entity, _)| *entity == agent)
                .map(|(_, activated)| *activated),
            Some(10)
        );
        assert_eq!(
            stack_times
                .iter()
                .find(|(entity, _)| *entity != agent)
                .map(|(_, activated)| *activated),
            Some(0)
        );
    }

    #[test]
    fn codex_acp_routes_shell_commands_through_vmux_run() {
        for agent_id in ["codex", "codex-acp"] {
            let env = apply_agent_compatibility_env(agent_id, Vec::new());
            let config = env
                .iter()
                .find(|(key, _)| key == "CODEX_CONFIG")
                .map(|(_, value)| serde_json::from_str::<serde_json::Value>(value).unwrap())
                .expect("codex ACP compatibility config");

            assert_eq!(config["features"]["shell_tool"], false);
            assert_eq!(config["features"]["unified_exec"], false);
            assert_eq!(config["tools"]["web_search"], false);
            assert_eq!(config["approvals_reviewer"], "user");
            assert_eq!(config["mcp_servers"]["vmux"]["tool_timeout_sec"], 660);
            assert_eq!(
                config["features"]["code_mode"]["direct_only_tool_namespaces"],
                serde_json::json!([crate::client::cli::codex::DIRECT_ONLY_NAMESPACE])
            );
            assert!(
                config["developer_instructions"]
                    .as_str()
                    .unwrap()
                    .contains("mcp__vmux__run")
            );
        }
    }

    #[test]
    fn claude_acp_extends_mcp_tool_timeout() {
        for agent_id in ["claude", "claude-acp"] {
            let env = apply_agent_compatibility_env(agent_id, vec![s("MCP_TOOL_TIMEOUT", "60000")]);
            assert_eq!(
                env.iter()
                    .find(|(key, _)| key == "MCP_TOOL_TIMEOUT")
                    .map(|(_, value)| value.as_str()),
                Some("660000")
            );
        }
    }

    #[test]
    fn vibe_acp_routes_shell_commands_through_vmux_run() {
        let env = apply_agent_compatibility_env(
            "mistral-vibe",
            vec![
                s("VIBE_DISABLED_TOOLS", r#"["from-env"]"#),
                s(
                    "VIBE_MCP_SERVERS",
                    r#"[{"name":"from-env","transport":"stdio","command":"env-command"}]"#,
                ),
            ],
        );
        let disabled = env
            .iter()
            .find(|(key, _)| key == "VIBE_DISABLED_TOOLS")
            .map(|(_, value)| serde_json::from_str::<Vec<String>>(value).unwrap())
            .expect("Vibe ACP disabled tools");

        assert_eq!(disabled, vec!["from-env", "bash"]);
        let mcp_servers = env
            .iter()
            .find(|(key, _)| key == "VIBE_MCP_SERVERS")
            .map(|(_, value)| serde_json::from_str::<serde_json::Value>(value).unwrap())
            .expect("Vibe ACP MCP servers");
        assert_eq!(mcp_servers[0]["name"], "from-env");
    }

    #[test]
    fn vibe_acp_discards_invalid_mcp_environment() {
        let env =
            apply_agent_compatibility_env("mistral-vibe", vec![s("VIBE_MCP_SERVERS", "not-json")]);

        assert!(env.iter().all(|(key, _)| key != "VIBE_MCP_SERVERS"));
    }

    #[test]
    fn codex_acp_preserves_existing_config() {
        let env = apply_agent_compatibility_env(
            "codex",
            vec![s(
                "CODEX_CONFIG",
                r#"{"model":"gpt-test","features":{"custom_feature":true,"code_mode":{"custom_setting":"keep"}}}"#,
            )],
        );
        let config = env
            .iter()
            .find(|(key, _)| key == "CODEX_CONFIG")
            .map(|(_, value)| serde_json::from_str::<serde_json::Value>(value).unwrap())
            .unwrap();

        assert_eq!(config["model"], "gpt-test");
        assert_eq!(config["features"]["custom_feature"], true);
        assert_eq!(config["features"]["code_mode"]["custom_setting"], "keep");
        assert_eq!(config["features"]["shell_tool"], false);
    }

    #[test]
    fn codex_acp_reports_discarded_invalid_config() {
        let (_, invalid_json) = parse_codex_config(Some("{not-json"));
        assert!(invalid_json.unwrap().contains("invalid JSON"));

        let (_, non_object) = parse_codex_config(Some("[]"));
        assert!(non_object.unwrap().contains("not a JSON object"));
    }

    #[test]
    fn plugin_builds_and_runs_without_panic() {
        let mut app = App::new();
        app.add_plugins(bevy::app::TaskPoolPlugin::default())
            .add_plugins(AcpAgentPlugin)
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<WebviewExtendStandardMaterial>>();
        app.world_mut().spawn(AcpSession {
            agent_id: "vibe-acp".to_string(),
            sid: "s1".to_string(),
            cwd: std::path::PathBuf::from("/tmp"),
            anchor: vmux_core::ProcessId::new(),
            resume: None,
        });
        app.update();
    }
}
