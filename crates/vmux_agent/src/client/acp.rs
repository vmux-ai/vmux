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
use vmux_service::protocol::{ClientMessage, SharedMessage};
use vmux_setting::AppSettings;
use vmux_terminal::reattach_terminal_bundle;

use crate::components::{AgentApprovalPolicy, PromptQueue};
use crate::events::AgentApprovalRequest;
use crate::handoff::{ImportedConversation, PendingHandoff};
use crate::run_state::AgentRunState;

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

const CONVERSATION_TITLE_STEER_PROMPT: &str = "On the first user message, always call mcp__vmux__set_conversation_title as the first tool of the turn. The host immediately shows the raw first prompt as a provisional title; replace it with a concise 3 to 7 word summary with corrected spelling and grammar. On later user messages, call the tool only when the conversation topic materially changes; keep the current title for same-topic follow-ups. When needed, call it before reading skills, calling any other tool, or answering. Never copy the user's prompt verbatim. This tool never needs user permission.";

const UNBOUND_WORKSPACE_CONTEXT: &str = "VMUX HOST POLICY (mandatory): This tab has no selected project. Read-only inspection may use the current directory or a known path immediately. Never call select_project or create_worktree for requests that only read, show, search, or explain existing files. Before the first edit, write, test, build, or other mutation in an existing project, call select_project with its known path or without a path to open the project picker rooted at ~/.vmux/workspace. For a new project, do not ask the user to invent a folder location. First call request_user_choice with two concrete options: create the project at a suggested path under ~/.vmux/workspace, or choose an existing project. Use ~/.vmux/workspace/<remote-host>/<organization>/<repository> when a remote is known and ~/.vmux/workspace/local/<project> otherwise. If the user chooses creation, use run only to create the empty directory, then call select_project with that path. vmux will offer Git initialization and use the new project root directly; never call create_worktree for that new project. Do not search the user's home directory. General questions and self-contained terminal demonstrations may run in the temporary current directory.";
const PENDING_WORKTREE_CONTEXT: &str = "VMUX HOST POLICY (mandatory): Project activation is pending. Do not access project paths directly or run git worktree add yourself. Wait for vmux to finish preparing the selected project before inspecting, editing, testing, or running it.";
const REPOSITORY_WORKTREE_CONTEXT: &str = "VMUX HOST POLICY (mandatory): The selected project is a Git repository, but this tab is not isolated. Reading and inspection are allowed without a worktree. Never call create_worktree for requests that only read, show, search, or explain existing files. Immediately before the first edit, write, test, build, or other mutation, call create_worktree. It reuses a known linked worktree, automatically uses one unambiguous existing worktree, or creates one when none exists. If it reports multiple candidates, ask the user with request_user_choice to choose an existing path or Create new worktree, then call create_worktree again with path or create=true. Never run git worktree add yourself.";

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

pub(crate) fn apply_acp_model_info(
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

pub(crate) fn apply_acp_model_selection_result(
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
/// if the tool name is already in this session's auto-policy, reply `AllowAlways` without
/// prompting.
fn acp_auto_approval_message(
    session: &AcpSession,
    policy: &AgentApprovalPolicy,
    request: &AgentApprovalRequest,
) -> Option<ClientMessage> {
    policy.allows(&request.name).then(|| {
        ClientMessage::Shared(SharedMessage::agent(
            session.sid.clone(),
            vmux_service::protocol::AgentAction::Approve {
                call_id: request.call_id.clone(),
                decision: vmux_service::protocol::ApprovalDecision::AllowAlways,
            },
        ))
    })
}

fn auto_allow_acp_approval(
    trigger: On<AgentApprovalRequest>,
    sessions: Query<(&AcpSession, &AgentApprovalPolicy)>,
    service: Option<Res<ServiceClient>>,
) {
    let request = trigger.event();
    let Ok((session, policy)) = sessions.get(request.session) else {
        return;
    };
    let Some(message) = acp_auto_approval_message(session, policy, request) else {
        return;
    };
    let Some(service) = service else {
        warn!(sid = %session.sid, call_id = %request.call_id, "auto-approval waiting for service connection");
        return;
    };
    service.0.send(message);
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
        let pinned_version = fallback.as_ref().and_then(|config| config.version.clone());

        *state = AgentRunState::Installing {
            pct: None,
            message: "Preparing agent…".to_string(),
        };

        let sid = session.sid.clone();
        let agent_id = session.agent_id.clone();
        let progress = installs.tx.clone();
        let shell = shell.clone();

        std::thread::spawn(move || {
            let resolved = crate::acp_install::resolve_from_registry(
                &agent_id,
                pinned_version.as_deref(),
                |phase, pct, msg| {
                    let (pct, message) = display_install_progress(phase, pct, msg);
                    let _ = progress.send(InstallMsg::Progress {
                        sid: sid.clone(),
                        pct,
                        message,
                    });
                },
            );
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
                    Some(cfg) if !cfg.command.is_empty() => InstallMsg::Ready {
                        sid,
                        command: cfg.command,
                        args: cfg.args,
                        env: apply_agent_compatibility_env(
                            &agent_id,
                            build_agent_env(cfg.env, login_env, None),
                        ),
                    },
                    _ => InstallMsg::Failed {
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

    disable_codex_skills(
        &mut config,
        &crate::client::cli::codex::codex_disabled_skill_files(),
    );

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
    let instructions = if instructions.contains("mcp__vmux__set_conversation_title") {
        instructions
    } else {
        format!("{instructions}\n\n{CONVERSATION_TITLE_STEER_PROMPT}")
    };
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

fn disable_codex_skills(
    config: &mut serde_json::Map<String, serde_json::Value>,
    skill_files: &[std::path::PathBuf],
) {
    if skill_files.is_empty() {
        return;
    }
    let skills = config
        .entry("skills")
        .or_insert_with(|| serde_json::json!({}));
    if !skills.is_object() {
        *skills = serde_json::json!({});
    }
    let configured = skills
        .as_object_mut()
        .unwrap()
        .entry("config")
        .or_insert_with(|| serde_json::json!([]));
    if !configured.is_array() {
        *configured = serde_json::json!([]);
    }
    let configured = configured.as_array_mut().unwrap();
    for skill_file in skill_files {
        let path = skill_file.to_string_lossy();
        if let Some(existing) = configured.iter_mut().find(|entry| {
            entry
                .get("path")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|candidate| candidate == path)
        }) {
            existing
                .as_object_mut()
                .unwrap()
                .insert("enabled".to_string(), serde_json::Value::Bool(false));
        } else {
            configured.push(serde_json::json!({
                "path": path,
                "enabled": false,
            }));
        }
    }
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
    settings: Option<Res<AppSettings>>,
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
                        managed_mcp_servers: crate::managed_mcp::acp_servers(),
                        effort: settings
                            .as_ref()
                            .and_then(|settings| settings.agent.effort_for(&session.agent_id))
                            .map(str::to_string),
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
#[path = "acp.test.rs"]
mod tests;
