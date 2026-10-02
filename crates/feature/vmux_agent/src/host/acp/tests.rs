use super::*;

fn install_test_app() -> App {
    let mut app = App::new();
    app.add_plugins(AcpToolPlugin);
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

fn acp_session(agent_id: &str, sid: &str) -> AcpSession {
    AcpSession {
        agent_id: agent_id.to_string(),
        sid: sid.to_string(),
        cwd: PathBuf::from("/workspace"),
        anchor: vmux_ecs::ProcessId::new(),
        resume: None,
    }
}

#[test]
fn completed_install_progress_describes_agent_startup() {
    let progress = AcpInstallProgress::from_phase(InstallPhase::Done, Some(100), "ready");
    assert_eq!(progress.pct, None);
    assert_eq!(progress.message, "Starting agent…");

    let progress =
        AcpInstallProgress::from_phase(InstallPhase::Downloading, Some(42), "downloading");
    assert_eq!(progress.pct, Some(42));
    assert_eq!(progress.message, "downloading");
    assert_eq!(AcpLaunchStarted::ready_message(None), "Starting agent…");
    assert_eq!(
        AcpLaunchStarted::ready_message(Some("session-1")),
        "Loading session history…"
    );
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
        .spawn((install_key("claude"), completed_job("stale failure")))
        .id();
    let stack = app
        .world_mut()
        .spawn((
            acp_session("codex", "new-session"),
            AcpLaunchStarted,
            AcpInstallWaiter {
                job,
                sid: "old-session".to_string(),
                agent_id: "claude".to_string(),
            },
            AgentRunState::Installing {
                pct: None,
                message: "Preparing agent…".to_string(),
            },
        ))
        .id();

    app.update();

    assert!(app.world().get::<AcpInstallWaiter>(stack).is_none());
    assert!(app.world().get::<AcpLaunchStarted>(stack).is_none());
    assert!(matches!(
        app.world().get::<AgentRunState>(stack),
        Some(AgentRunState::Installing { .. })
    ));
    assert!(app.world().get_entity(job).is_err());
}

#[test]
fn removing_acp_session_clears_install_waiter() {
    let mut app = install_test_app();
    let job = app
        .world_mut()
        .spawn((install_key("claude"), completed_job("stale failure")))
        .id();
    let stack = app
        .world_mut()
        .spawn((
            acp_session("claude", "old-session"),
            AcpLaunchStarted,
            AcpInstallWaiter {
                job,
                sid: "old-session".to_string(),
                agent_id: "claude".to_string(),
            },
            AgentRunState::Installing {
                pct: None,
                message: "Preparing agent…".to_string(),
            },
        ))
        .id();

    app.world_mut().entity_mut(stack).remove::<AcpSession>();
    app.update();

    assert!(app.world().get::<AcpInstallWaiter>(stack).is_none());
    assert!(app.world().get::<AcpLaunchStarted>(stack).is_none());
    assert!(matches!(
        app.world().get::<AgentRunState>(stack),
        Some(AgentRunState::Installing { .. })
    ));
    assert!(app.world().get_entity(job).is_err());
}
