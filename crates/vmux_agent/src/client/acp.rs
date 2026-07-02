//! GUI-side ACP agent integration: the [`AcpSession`] component identifies an ACP agent
//! pane, and [`AcpAgentPlugin`] forwards spawn/input/close to the daemon's
//! `AcpSessionManager`. The streamed updates are consumed by the shared
//! `consume_page_agent_stream` system (ACP reuses the Page stream messages).

use bevy::prelude::*;
use crossbeam_channel::{Receiver, Sender};
use vmux_service::client::ServiceClient;
use vmux_service::protocol::ClientMessage;
use vmux_setting::AppSettings;

use crate::components::{AgentApprovalPolicy, PendingUserInput};
use crate::events::{AgentApprovalReply, AgentApprovalRequest, ApprovalDecision};
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
            .add_systems(Startup, start_catalog_fetch)
            .add_systems(
                Update,
                (
                    install_acp_session_when_focused,
                    send_acp_input,
                    drain_acp_installs,
                    receive_catalog,
                ),
            )
            .add_observer(close_acp_session_on_remove)
            .add_observer(auto_allow_acp_approval);
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

        std::thread::spawn(move || {
            let resolved =
                crate::acp_install::resolve_from_registry(&agent_id, |_phase, pct, msg| {
                    let _ = progress.send(InstallMsg::Progress {
                        sid: sid.clone(),
                        pct,
                        message: msg.to_string(),
                    });
                });
            let msg = match resolved {
                Ok(r) => InstallMsg::Ready {
                    sid,
                    command: r.command,
                    args: r.args,
                    env: apply_path_prepend(r.env, r.path_prepend),
                },
                Err(reg_err) => match fallback {
                    Some(cfg) => InstallMsg::Ready {
                        sid,
                        command: cfg.command,
                        args: cfg.args,
                        env: cfg.env,
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

/// Prepend a managed runtime `bin/` to the child's `PATH` (so e.g. `npx` finds its `node`),
/// replacing any inherited `PATH` entry.
fn apply_path_prepend(
    mut env: Vec<(String, String)>,
    prepend: Option<String>,
) -> Vec<(String, String)> {
    if let Some(dir) = prepend {
        let full = match std::env::var("PATH") {
            Ok(existing) if !existing.is_empty() => format!("{dir}:{existing}"),
            _ => dir,
        };
        env.retain(|(k, _)| k != "PATH");
        env.push(("PATH".to_string(), full));
    }
    env
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
                    let mcp = crate::mcp::resolve(&session.cwd, session.anchor)
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
                    });
                }
            }
        }
    }
}

fn send_acp_input(
    mut commands: Commands,
    q: Query<(Entity, &AcpSession, &PendingUserInput)>,
    service: Option<Res<ServiceClient>>,
) {
    let Some(service) = service else {
        return;
    };
    for (entity, session, pending) in &q {
        service.0.send(ClientMessage::AgentInput {
            sid: session.sid.clone(),
            text: pending.0.clone(),
        });
        commands.entity(entity).remove::<PendingUserInput>();
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

    #[test]
    fn plugin_builds_and_runs_without_panic() {
        let mut app = App::new();
        app.add_plugins(bevy::app::TaskPoolPlugin::default())
            .add_plugins(AcpAgentPlugin);
        app.world_mut().spawn(AcpSession {
            agent_id: "vibe-acp".to_string(),
            sid: "s1".to_string(),
            cwd: std::path::PathBuf::from("/tmp"),
            anchor: vmux_core::ProcessId::new(),
        });
        app.update();
    }
}
