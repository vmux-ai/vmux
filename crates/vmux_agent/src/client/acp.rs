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
            .add_message::<vmux_service::agent_events::PageAgentInfo>()
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
struct AcpInstallStarted;

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
            .find(|c| c.id == session.agent_id)
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
    mut env: Vec<(String, String)>,
) -> Vec<(String, String)> {
    if agent_id != "codex" {
        return env;
    }

    let existing = env
        .iter()
        .rev()
        .find(|(key, _)| key == "CODEX_CONFIG")
        .map(|(_, value)| value.as_str());
    let (mut config, warning) = parse_codex_config(existing);
    if let Some(warning) = warning {
        bevy::log::warn!("{warning}");
    }

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
                let Some(service) = service.as_ref() else {
                    continue;
                };
                if let Some((session, _)) = q.iter().find(|(s, _)| s.sid == sid) {
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
        &AcpSession,
        &mut AgentRunState,
        &mut PromptQueue,
        Option<&mut PendingHandoff>,
        Option<&mut ImportedConversation>,
    )>,
    service: Option<Res<ServiceClient>>,
) {
    let Some(service) = service else {
        return;
    };
    for (session, mut state, mut queue, mut pending, mut imported) in &mut q {
        if !queue.ready(matches!(*state, AgentRunState::Idle)) {
            continue;
        }
        let Some(text) = queue.take_next() else {
            continue;
        };
        let context = pending
            .as_deref_mut()
            .and_then(PendingHandoff::context_for_send);
        if context.is_some()
            && let Some(imported) = imported.as_deref_mut()
            && imported.first_prompt.is_none()
        {
            imported.first_prompt = Some(text.clone());
        }
        service.0.send(ClientMessage::AgentInput {
            sid: session.sid.clone(),
            text,
            context,
        });
        *state = AgentRunState::Streaming;
    }
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
        let env = apply_agent_compatibility_env("codex", Vec::new());
        let config = env
            .iter()
            .find(|(key, _)| key == "CODEX_CONFIG")
            .map(|(_, value)| serde_json::from_str::<serde_json::Value>(value).unwrap())
            .expect("codex ACP compatibility config");

        assert_eq!(config["features"]["shell_tool"], false);
        assert_eq!(config["features"]["unified_exec"], false);
        assert_eq!(config["tools"]["web_search"], false);
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
