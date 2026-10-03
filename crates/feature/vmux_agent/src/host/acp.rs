use std::collections::BTreeMap;
#[cfg(test)]
use std::path::PathBuf;
use std::thread::JoinHandle;

use bevy::prelude::*;
use crossbeam_channel::{Receiver, Sender};
use vmux_api::protocol::ManagedMcpServer;
#[cfg(test)]
use vmux_ecs::event::InstallPhase;
use vmux_ecs::profile::{CurrentProfile, McpCredentials};
use vmux_ecs::service::ServiceConnected;
use vmux_ecs::service::ServiceRequest;
use vmux_ecs::{Cwd, EntityTarget, ProcessAnchor};
use vmux_editor::lsp::store::PackageStore;
use vmux_session::{AcpSessionId, AgentId, Session, SessionId};
use vmux_setting::{AcpAgentConfig, AppSettings};
use vmux_tool::state::{ToolOperationKey, ToolOperationKind, ToolProvider, ToolStatus};
use vmux_tool::{
    ToolInventory, ToolInventoryItem, ToolOperator, ToolProviderBinding, ToolProviderSnapshot,
    ToolScanner, ToolStore, ToolsManifest,
};

use self::installer_driver::{AgentInstaller, start_acp_install_job};
use self::registry::Registry;
pub(super) use config::add as add_config;
pub(super) use registry::add as add_registry;
use vmux_session::AgentRunState;

mod config;
mod config_driver;
mod environment_driver;
mod install;
mod installer_driver;
pub mod registry;
mod registry_driver;

pub(super) fn add(app: &mut App) {
    install::add(app);
    app.add_message::<ServiceRequest>()
        .add_message::<vmux_ecs::agent::SwapStackSession>()
        .add_systems(Startup, spawn_tool_provider);
}

fn spawn_tool_provider(mut commands: Commands) {
    commands.spawn((
        ToolProviderBinding::new::<crate::Feature>(0),
        ToolScanner::new(scan_tools),
        ToolOperator::new(operate_tool),
    ));
}

fn scan_tools(
    provider: &ToolProvider,
    _store: &ToolStore,
    manifest: &mut ToolsManifest,
    refresh: bool,
) -> Result<ToolProviderSnapshot, String> {
    let catalog = if refresh {
        let policy = crate::policy_driver::PolicyDriver::registry();
        Registry::fetch_blocking(&policy.url)
            .ok()
            .or_else(Registry::cached)
    } else {
        Registry::cached()
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
    let package_store = PackageStore::at(vmux_ecs::profile::ProfilePaths::current().agents());
    let receipts = package_store.installed();
    let inventory = receipts
        .into_values()
        .filter(|receipt| receipt.source_id.starts_with("acp:"))
        .map(|receipt| {
            let agent = catalog.get(receipt.name.as_str());
            let latest = agent.and_then(|agent| agent.version.clone());
            ToolInventoryItem {
                id: receipt.name.as_str().to_string(),
                name: agent
                    .map(|agent| agent.name.clone())
                    .unwrap_or_else(|| receipt.name.as_str().to_string()),
                icon: agent.and_then(|agent| agent.icon.clone()),
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
        .collect();
    Ok(ToolInventory::new(provider.clone(), inventory)
        .reconcile(manifest)
        .into())
}

fn operate_tool(
    store: &ToolStore,
    operation: &ToolOperationKey,
    _value: &str,
) -> Result<String, String> {
    let id = operation.item_id.trim();
    match operation.kind {
        ToolOperationKind::Install | ToolOperationKind::Update => {
            if id.is_empty() {
                return Err("package name is required".to_string());
            }
            AgentInstaller::current().resolve(id, None, |_, _, _| {})?;
            store.set_managed_package(&operation.provider, id, true)?;
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
            AgentInstaller::current().uninstall(id)?;
            store.set_managed_package(&operation.provider, id, false)?;
            Ok(format!("{id} removed"))
        }
        ToolOperationKind::Forget => {
            store.set_managed_package(&operation.provider, id, false)?;
            Ok(format!("{id} removed from tools.toml"))
        }
        ToolOperationKind::Adopt => {
            store.set_managed_package(&operation.provider, id, true)?;
            Ok(format!("{id} is now managed"))
        }
        ToolOperationKind::Import => {
            let mut manifest = store.load()?;
            let before = manifest.managed_packages(operation.provider.id()).len();
            let _ = scan_tools(&operation.provider, store, &mut manifest, false)?;
            let imported = manifest
                .managed_packages(operation.provider.id())
                .len()
                .saturating_sub(before);
            store.save(&manifest)?;
            Ok(format!("imported {imported} acp item(s)"))
        }
        _ => Err(format!("ACP does not support {:?}", operation.kind)),
    }
}

#[derive(Component)]
pub(crate) struct AcpLaunchStarted;

#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub(crate) struct AcpPackageChanged {
    pub(crate) agent_id: String,
}

#[derive(Component)]
struct AcpInstallJob {
    progress: Receiver<InstallState>,
    thread: Option<JoinHandle<AcpInstallOutcome>>,
    outcome: Option<AcpInstallOutcome>,
    package_reported: bool,
}

#[derive(Component, Clone, Debug, PartialEq, Eq, Hash)]
struct AcpInstallKey {
    agent_id: String,
    version: Option<String>,
    fallback_command: String,
    fallback_args: Vec<String>,
    fallback_env: Vec<(String, String)>,
    shell: String,
}

#[derive(Component)]
#[relationship(relationship_target = AcpInstallWaiters)]
struct AcpInstallWaiter {
    #[relationship]
    job: Entity,
    sid: String,
    agent_id: String,
}

#[derive(Component)]
#[relationship_target(relationship = AcpInstallWaiter)]
struct AcpInstallWaiters(Vec<Entity>);

struct AcpInstallRequest {
    agent_id: String,
    fallback: Option<AcpAgentConfig>,
    shell: String,
}

#[derive(Component, Clone, Debug, PartialEq, Eq)]
struct InstallState {
    pct: Option<u8>,
    message: String,
}

#[derive(Clone)]
struct AcpInstallOutcome {
    package_added: bool,
    launch: Result<AcpLaunch, String>,
}

#[derive(Clone)]
struct AcpLaunch {
    command: String,
    args: Vec<String>,
    env: Vec<(String, String)>,
    managed_mcp_servers: Vec<ManagedMcpServer>,
    mcp_revision: u64,
}

#[derive(Clone)]
struct AcpInstallProgressSink {
    pending: Sender<InstallState>,
    wake: Option<bevy::winit::EventLoopProxy<bevy::winit::WinitUserEvent>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_ecs::profile::mcp_credentials::{McpCredentialAccess, McpCredentialStorage};
    use vmux_ecs::profile::{ActiveProfile, Profile, ProfileDirectories, ProfilePaths};

    fn install_test_app() -> App {
        let mut app = App::new();
        add(&mut app);
        let profile = Profile::named("test");
        let paths = ProfilePaths::current();
        app.world_mut().spawn((
            Name::new("Test"),
            ActiveProfile(profile.clone()),
            ProfileDirectories(paths.clone()),
            McpCredentials {
                access: McpCredentialAccess::default(),
                storage: McpCredentialStorage::at(profile, paths),
            },
        ));
        app
    }

    fn completed_job(message: &str) -> AcpInstallJob {
        let (_progress_sender, progress) = crossbeam_channel::unbounded();
        AcpInstallJob {
            progress,
            thread: None,
            outcome: Some(AcpInstallOutcome {
                package_added: false,
                launch: Err(message.to_string()),
            }),
            package_reported: false,
        }
    }

    fn install_key(agent_id: &str) -> AcpInstallKey {
        AcpInstallRequest {
            agent_id: agent_id.to_string(),
            fallback: None,
            shell: String::new(),
        }
        .key()
    }

    struct TestSession;

    impl TestSession {
        fn bundle(agent_id: &str, sid: &str) -> impl Bundle {
            (
                Session,
                SessionId(sid.to_string()),
                AgentId(agent_id.to_string()),
                Cwd(PathBuf::from("/workspace")),
                ProcessAnchor(vmux_ecs::ProcessId::new()),
            )
        }
    }

    #[test]
    fn completed_install_progress_describes_agent_startup() {
        let progress = InstallState::from_phase(InstallPhase::Done, Some(100), "ready");
        assert_eq!(progress.pct, None);
        assert_eq!(progress.message, "Starting agent…");

        let progress = InstallState::from_phase(InstallPhase::Downloading, Some(42), "downloading");
        assert_eq!(progress.pct, Some(42));
        assert_eq!(progress.message, "downloading");
    }

    #[test]
    fn install_job_tracks_waiting_sessions_through_relationship() {
        let mut app = App::new();
        let job = app.world_mut().spawn_empty().id();
        let stack = app
            .world_mut()
            .spawn(AcpInstallWaiter {
                job,
                sid: "session".to_string(),
                agent_id: "agent".to_string(),
            })
            .id();

        let waiters = app.world().get::<AcpInstallWaiters>(job).unwrap();
        assert_eq!(waiters.iter().collect::<Vec<_>>(), vec![stack]);
    }

    #[test]
    fn replaced_acp_session_ignores_stale_install_outcome() {
        let mut app = install_test_app();
        let job = app
            .world_mut()
            .spawn((
                install_key("claude"),
                InstallState::preparing(),
                completed_job("stale failure"),
            ))
            .id();
        let stack = app
            .world_mut()
            .spawn((
                TestSession::bundle("codex", "new-session"),
                AcpLaunchStarted,
                AcpInstallWaiter {
                    job,
                    sid: "old-session".to_string(),
                    agent_id: "claude".to_string(),
                },
                RunState::Idle,
            ))
            .id();

        app.update();

        assert!(app.world().get::<AcpInstallWaiter>(stack).is_none());
        assert!(app.world().get::<AcpLaunchStarted>(stack).is_none());
        assert!(matches!(
            app.world().get::<RunState>(stack),
            Some(RunState::Idle)
        ));
        assert!(app.world().get_entity(job).is_err());
    }

    #[test]
    fn removing_process_anchor_clears_install_waiter() {
        let mut app = install_test_app();
        let job = app
            .world_mut()
            .spawn((
                install_key("claude"),
                InstallState::preparing(),
                completed_job("stale failure"),
            ))
            .id();
        let stack = app
            .world_mut()
            .spawn((
                TestSession::bundle("claude", "old-session"),
                AcpLaunchStarted,
                AcpInstallWaiter {
                    job,
                    sid: "old-session".to_string(),
                    agent_id: "claude".to_string(),
                },
                RunState::Idle,
            ))
            .id();

        app.world_mut().entity_mut(stack).remove::<ProcessAnchor>();
        app.update();

        assert!(app.world().get::<AcpInstallWaiter>(stack).is_none());
        assert!(app.world().get::<AcpLaunchStarted>(stack).is_none());
        assert!(matches!(
            app.world().get::<RunState>(stack),
            Some(RunState::Idle)
        ));
        assert!(app.world().get_entity(job).is_err());
    }
}
