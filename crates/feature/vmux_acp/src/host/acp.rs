use std::path::PathBuf;

use crate::AcpSessionId;
use bevy::prelude::*;
#[cfg(test)]
use vmux_ecs::event::InstallPhase;
use vmux_ecs::service::ServiceConnected;
use vmux_ecs::service::ServiceRequest;
use vmux_ecs::{Cwd, EntityTarget, ProcessAnchor};
use vmux_session::{AgentId, RunState, Session, SessionId};
use vmux_setting::{AcpAgentConfig, AppSettings};
use vmux_tool::ToolProviderBinding;

use self::environment_driver::AcpEnvironment;
pub(super) use config::ConfigPlugin;

mod config;
mod config_driver;
mod environment_driver;
mod install;
pub(super) mod installed;
pub(super) mod installed_launch;
mod installer_driver;
mod inventory;
mod package;
mod package_io;
pub mod registry;
mod registry_catalog;
mod registry_driver;
mod tool;

pub(super) struct AgentProviderPlugin;

impl Plugin for AgentProviderPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            install::InstallPlugin,
            installed::InstalledPlugin,
            installed_launch::InstalledLaunchPlugin,
            inventory::AcpInventoryPlugin,
            tool::AcpToolPlugin,
        ))
        .add_message::<ServiceRequest>()
        .add_message::<vmux_ecs::agent::SwapStackSession>()
        .add_systems(Startup, spawn_tool_provider);
    }
}

fn spawn_tool_provider(mut commands: Commands) {
    let provider = commands
        .spawn(ToolProviderBinding::new::<crate::Feature>(0))
        .id();
    commands.entity(provider).insert((
        vmux_tool::observation::Handler,
        vmux_tool::observation::PackageProvider,
    ));
}

#[derive(Component)]
pub(crate) struct AcpLaunchStarted;

#[derive(Component)]
pub(crate) struct AcpSpawnQueued;

#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub(crate) struct AcpPackageChanged {
    pub(crate) agent_id: String,
}

#[derive(Component)]
struct AcpInstallJob {
    package: Option<Entity>,
    task: Option<vmux_ecs::BlockingTask<AcpInstallResult>>,
    outcome: Option<AcpInstallOutcome>,
}

#[derive(Component)]
struct PackageStarted;

#[derive(Component)]
struct Retiring;

#[derive(Component)]
struct AgentReady(Result<installer_driver::ResolvedAgent, String>);

#[derive(Component)]
struct AcpInstallResult {
    agent: Result<installer_driver::ResolvedAgent, String>,
    login_env: Vec<(String, String)>,
}

#[derive(Component, Clone, Debug, PartialEq, Eq, Hash)]
struct AcpInstallKey {
    profile: Option<Entity>,
    agents: PathBuf,
    catalog: Option<Entity>,
    catalog_revision: u64,
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

#[derive(Component, Clone)]
struct AcpInstallRequest {
    profile: Option<Entity>,
    agents: PathBuf,
    catalog: Option<Entity>,
    catalog_revision: u64,
    selected_agent: Option<registry::RegistryAgent>,
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
    launch: Result<AcpLaunch, String>,
}

#[derive(Clone)]
pub(super) struct AcpLaunch {
    command: String,
    args: Vec<String>,
    env: Vec<(String, String)>,
}

struct CatalogCommand<'a>(&'a str);

impl CatalogCommand<'_> {
    fn allowed(&self) -> bool {
        let command = std::path::Path::new(self.0)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(self.0);
        !matches!(
            command,
            "npx" | "npm" | "pnpm" | "yarn" | "bunx" | "uvx" | "pipx"
        )
    }
}

impl AcpLaunch {
    pub(super) fn configured_catalog(config: &AcpAgentConfig, shell: &str) -> Result<Self, String> {
        if config.command.trim().is_empty() {
            return Err(format!("ACP command is unavailable: {}", config.id));
        }
        if !CatalogCommand(&config.command).allowed() {
            return Err(format!(
                "ACP catalog does not automatically run package manager commands: {}",
                config.id
            ));
        }
        let login_env = vmux_process::LoginShellEnvironment::get(shell);
        Ok(Self {
            command: config.command.clone(),
            args: config.args.clone(),
            env: AcpEnvironment::catalog(config.env.clone(), login_env, None).into_inner(),
        })
    }

    pub(super) fn catalog_message(
        &self,
        request_id: u64,
        agent_id: &str,
        agent_name: &str,
        cwd: &std::path::Path,
    ) -> vmux_api::protocol::ClientMessage {
        vmux_api::protocol::ClientMessage::ListAcpSessions {
            request_id,
            agent_id: agent_id.to_string(),
            agent_name: agent_name.to_string(),
            command: self.command.clone(),
            args: self.args.clone(),
            env: self.env.clone(),
            cwd: cwd.to_string_lossy().into_owned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::{App, Name};
    use vmux_ecs::profile::{ActiveProfile, Profile, ProfileDirectories, ProfilePaths};

    #[test]
    fn catalog_command_rejects_package_runners() {
        assert!(!CatalogCommand("npx").allowed());
        assert!(!CatalogCommand("/usr/local/bin/uvx").allowed());
        assert!(CatalogCommand("/usr/local/bin/custom-acp").allowed());
    }

    struct InstallFixture {
        app: App,
    }

    impl InstallFixture {
        fn new() -> Self {
            let mut app = App::new();
            app.add_plugins(install::InstallPlugin)
                .add_message::<ServiceRequest>()
                .add_message::<vmux_ecs::agent::SwapStackSession>();
            app.world_mut().spawn((
                Name::new("Test"),
                ActiveProfile(Profile::named("test")),
                ProfileDirectories(ProfilePaths::current()),
            ));
            Self { app }
        }

        fn wake(&mut self) -> std::sync::Arc<RetirementWake> {
            let wake =
                std::sync::Arc::new(RetirementWake(std::sync::atomic::AtomicBool::new(false)));
            self.app
                .insert_resource(vmux_ecs::JobWake(std::task::Waker::from(wake.clone())));
            wake
        }
    }

    impl AcpInstallJob {
        fn completed(message: &str) -> Self {
            Self {
                package: None,
                task: None,
                outcome: Some(AcpInstallOutcome {
                    launch: Err(message.to_string()),
                }),
            }
        }
    }

    impl AcpInstallKey {
        fn fixture(agent_id: &str) -> Self {
            AcpInstallRequest {
                profile: None,
                agents: PathBuf::new(),
                catalog: None,
                catalog_revision: 0,
                selected_agent: None,
                agent_id: agent_id.to_string(),
                fallback: None,
                shell: String::new(),
            }
            .key()
        }
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

    struct RetirementWake(std::sync::atomic::AtomicBool);

    impl std::task::Wake for RetirementWake {
        fn wake(self: std::sync::Arc<Self>) {
            self.0.store(true, std::sync::atomic::Ordering::Release);
        }
        fn wake_by_ref(self: &std::sync::Arc<Self>) {
            self.0.store(true, std::sync::atomic::Ordering::Release);
        }
    }

    #[test]
    fn retiring_a_shared_install_wakes_the_pending_replacement() {
        let mut fixture = InstallFixture::new();
        let wake = fixture.wake();
        let app = &mut fixture.app;
        let job = app
            .world_mut()
            .spawn((
                AcpInstallKey::fixture("agent"),
                InstallState::preparing(),
                AcpInstallJob::completed("canceled"),
                Retiring,
            ))
            .id();
        let session = app
            .world_mut()
            .spawn((
                TestSession::bundle("agent", "next"),
                RunState::Idle,
                AcpInstallRequest {
                    profile: None,
                    agents: PathBuf::new(),
                    catalog: None,
                    catalog_revision: 0,
                    selected_agent: None,
                    agent_id: "agent".into(),
                    fallback: None,
                    shell: String::new(),
                },
            ))
            .id();
        app.update();
        assert!(app.world().get_entity(job).is_err());
        assert!(app.world().get::<AcpInstallRequest>(session).is_some());
        assert!(app.world().get::<AcpLaunchStarted>(session).is_none());
        assert!(wake.0.load(std::sync::atomic::Ordering::Acquire));
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
        let mut fixture = InstallFixture::new();
        let app = &mut fixture.app;
        let job = app
            .world_mut()
            .spawn((
                AcpInstallKey::fixture("claude"),
                InstallState::preparing(),
                AcpInstallJob::completed("stale failure"),
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
        let mut fixture = InstallFixture::new();
        let app = &mut fixture.app;
        let job = app
            .world_mut()
            .spawn((
                AcpInstallKey::fixture("claude"),
                InstallState::preparing(),
                AcpInstallJob::completed("stale failure"),
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

    #[test]
    fn deliver_injects_resolved_profile_and_project_mcp_servers_into_spawn_request() {
        let dir = tempfile::tempdir().unwrap();
        let paths = ProfilePaths::at(Profile::named("test"), dir.path().join("data"));
        let repo = dir.path().join("repo");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        std::fs::create_dir_all(repo.join(".vmux")).unwrap();
        std::fs::create_dir_all(paths.profile()).unwrap();
        std::fs::write(
            paths.profile().join("mcp.ron"),
            "(version: 1, servers: {\n    \"profile-http\": Define(Http(url: \"https://mcp.example.com/mcp\")),\n    \"disabled-here\": Define(Stdio(command: \"npx\")),\n})\n",
        )
        .unwrap();
        std::fs::write(
            repo.join(".vmux/mcp.ron"),
            "(version: 1, servers: {\n    \"disabled-here\": Disable,\n    \"untrusted-project\": Define(Stdio(command: \"./evil\")),\n})\n",
        )
        .unwrap();

        let mut app = App::new();
        app.add_plugins((bevy::app::TaskPoolPlugin::default(), install::InstallPlugin))
            .add_message::<ServiceRequest>()
            .add_message::<vmux_ecs::agent::SwapStackSession>();
        app.world_mut().spawn((
            Name::new("Test"),
            ActiveProfile(Profile::named("test")),
            ProfileDirectories(paths),
        ));
        app.world_mut().spawn(ServiceConnected);
        let job = app
            .world_mut()
            .spawn((
                AcpInstallKey::fixture("claude-acp"),
                InstallState::preparing(),
                AcpInstallJob {
                    package: None,
                    task: None,
                    outcome: Some(AcpInstallOutcome {
                        launch: Ok(AcpLaunch {
                            command: "/bin/claude-acp".into(),
                            args: Vec::new(),
                            env: Vec::new(),
                        }),
                    }),
                },
            ))
            .id();
        let stack = app
            .world_mut()
            .spawn((
                Session,
                SessionId("s-mcp".into()),
                AgentId("claude-acp".into()),
                Cwd(repo),
                ProcessAnchor(vmux_ecs::ProcessId::new()),
                AcpLaunchStarted,
                AcpInstallWaiter {
                    job,
                    sid: "s-mcp".into(),
                    agent_id: "claude-acp".into(),
                },
                RunState::Idle,
            ))
            .id();

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let request = loop {
            app.update();
            let mut requests = app
                .world_mut()
                .resource_mut::<Messages<ServiceRequest>>()
                .drain()
                .collect::<Vec<_>>();
            if let Some(request) = requests.pop() {
                break request;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "ACP deliver did not emit SpawnAcpAgent after MCP resolution"
            );
            std::thread::sleep(std::time::Duration::from_millis(1));
        };
        let vmux_api::protocol::ClientMessage::SpawnAcpAgentWithMcp(spawn) = request.0 else {
            panic!("expected SpawnAcpAgentWithMcp");
        };
        let managed_mcp_servers = spawn.managed_mcp_servers;
        assert!(
            managed_mcp_servers
                .iter()
                .any(|server| server.name == "profile-http")
        );
        assert!(
            !managed_mcp_servers
                .iter()
                .any(|server| server.name == "disabled-here")
        );
        assert!(
            !managed_mcp_servers
                .iter()
                .any(|server| server.name == "untrusted-project")
        );
        assert!(app.world().get::<AcpSpawnQueued>(stack).is_some());
        assert!(
            app.world()
                .get::<vmux_tool::McpResolveResult>(stack)
                .is_none()
        );
    }
}
