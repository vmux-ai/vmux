use super::*;
use bevy_cef::prelude::BinIpcEventRawBuffer;

#[test]
fn plugin_builds_without_panic() {
    let mut app = App::new();
    app.add_plugins(bevy::app::TaskPoolPlugin::default())
        .init_resource::<BinIpcEventRawBuffer>()
        .add_plugins(PageAgentPlugin);
    app.update();
}

#[test]
fn auto_approved_acp_request_without_service_falls_back_to_awaiting_state() {
    let mut app = App::new();
    app.add_message::<PageAgentDelta>()
        .add_message::<PageAgentRunStatus>()
        .add_message::<PageAgentAwaitingApproval>()
        .add_message::<PageAgentApprovalResolved>()
        .add_message::<PageAgentSnapshot>()
        .add_message::<vmux_core::notify::AgentAttention>()
        .add_systems(Update, consume_page_agent_stream);
    let mut policy = AgentApprovalPolicy::default();
    policy.allow("run");
    let entity = app
        .world_mut()
        .spawn((
            AcpSession {
                agent_id: "a".into(),
                sid: "s1".into(),
                cwd: std::path::PathBuf::from("/tmp"),
                anchor: vmux_core::ProcessId::new(),
                resume: None,
            },
            AgentMessages::default(),
            AgentRunState::Streaming,
            PromptQueue::default(),
            policy,
        ))
        .id();
    app.world_mut().write_message(PageAgentAwaitingApproval {
        sid: "s1".into(),
        call_id: "call-1".into(),
        name: "run".into(),
        args_json: "{}".into(),
    });

    app.update();

    assert!(matches!(
        app.world().get::<AgentRunState>(entity),
        Some(AgentRunState::AwaitingApproval { call_id, name, args })
            if call_id == "call-1" && name == "run" && args == &serde_json::json!({})
    ));
}

#[test]
fn interrupted_status_pauses_queue_and_idles() {
    use crate::client::acp::AcpSession;
    use crate::components::PromptQueue;
    use vmux_service::agent_events::{
        PageAgentAwaitingApproval, PageAgentDelta, PageAgentRunStatus, PageAgentSnapshot,
    };
    use vmux_service::protocol::AgentRunStatus;

    let mut app = App::new();
    app.add_plugins(bevy::app::TaskPoolPlugin::default())
        .add_message::<PageAgentDelta>()
        .add_message::<PageAgentRunStatus>()
        .add_message::<PageAgentAwaitingApproval>()
        .add_message::<PageAgentApprovalResolved>()
        .add_message::<PageAgentSnapshot>()
        .add_message::<vmux_core::notify::AgentAttention>()
        .add_systems(Update, consume_page_agent_stream);

    let mut queue = PromptQueue::default();
    queue.enqueue("next".into());
    let e = app
        .world_mut()
        .spawn((
            AcpSession {
                agent_id: "a".into(),
                sid: "s1".into(),
                cwd: std::path::PathBuf::from("/tmp"),
                anchor: vmux_core::ProcessId::new(),
                resume: None,
            },
            AgentMessages::default(),
            AgentRunState::Streaming,
            queue,
        ))
        .id();
    app.world_mut().write_message(PageAgentRunStatus {
        sid: "s1".into(),
        status: AgentRunStatus::Interrupted,
    });
    app.update();

    let world = app.world();
    assert!(matches!(
        world.get::<AgentRunState>(e),
        Some(AgentRunState::Idle)
    ));
    let q = world.get::<PromptQueue>(e).unwrap();
    assert!(q.paused, "queue must pause after interrupt");
    assert_eq!(q.items.len(), 1, "held item must not auto-advance");
}

#[test]
fn flush_pending_interrupt_does_not_pause() {
    use crate::client::acp::AcpSession;
    use crate::components::PromptQueue;
    use vmux_service::agent_events::{
        PageAgentAwaitingApproval, PageAgentDelta, PageAgentRunStatus, PageAgentSnapshot,
    };
    use vmux_service::protocol::AgentRunStatus;

    let mut app = App::new();
    app.add_plugins(bevy::app::TaskPoolPlugin::default())
        .add_message::<PageAgentDelta>()
        .add_message::<PageAgentRunStatus>()
        .add_message::<PageAgentAwaitingApproval>()
        .add_message::<PageAgentApprovalResolved>()
        .add_message::<PageAgentSnapshot>()
        .add_message::<vmux_core::notify::AgentAttention>()
        .add_systems(Update, consume_page_agent_stream);

    let mut queue = PromptQueue::default();
    queue.enqueue("a".into());
    queue.enqueue("b".into());
    assert!(queue.request_flush());
    let e = app
        .world_mut()
        .spawn((
            AcpSession {
                agent_id: "a".into(),
                sid: "s1".into(),
                cwd: std::path::PathBuf::from("/tmp"),
                anchor: vmux_core::ProcessId::new(),
                resume: None,
            },
            AgentMessages::default(),
            AgentRunState::Streaming,
            queue,
        ))
        .id();
    app.world_mut().write_message(PageAgentRunStatus {
        sid: "s1".into(),
        status: AgentRunStatus::Interrupted,
    });
    app.update();

    let world = app.world();
    assert!(matches!(
        world.get::<AgentRunState>(e),
        Some(AgentRunState::Idle)
    ));
    let q = world.get::<PromptQueue>(e).unwrap();
    assert!(
        !q.paused,
        "flush interrupt must leave the queue running to drain"
    );
    assert_eq!(
        q.items.len(),
        2,
        "items wait for the idle drain to batch them"
    );
}

#[test]
fn flush_pending_error_rearms_queue() {
    use crate::client::acp::AcpSession;
    use crate::components::PromptQueue;
    use vmux_service::agent_events::{
        PageAgentAwaitingApproval, PageAgentDelta, PageAgentRunStatus, PageAgentSnapshot,
    };
    use vmux_service::protocol::AgentRunStatus;

    let mut app = App::new();
    app.add_plugins(bevy::app::TaskPoolPlugin::default())
        .add_message::<PageAgentDelta>()
        .add_message::<PageAgentRunStatus>()
        .add_message::<PageAgentAwaitingApproval>()
        .add_message::<PageAgentApprovalResolved>()
        .add_message::<PageAgentSnapshot>()
        .add_message::<vmux_core::notify::AgentAttention>()
        .add_systems(Update, consume_page_agent_stream);

    let mut queue = PromptQueue::default();
    queue.enqueue("retry".into());
    assert!(queue.request_flush());
    let entity = app
        .world_mut()
        .spawn((
            AcpSession {
                agent_id: "a".into(),
                sid: "s1".into(),
                cwd: std::path::PathBuf::from("/tmp"),
                anchor: vmux_core::ProcessId::new(),
                resume: None,
            },
            AgentMessages::default(),
            AgentRunState::Streaming,
            queue,
        ))
        .id();
    app.world_mut().write_message(PageAgentRunStatus {
        sid: "s1".into(),
        status: AgentRunStatus::Errored("cancel race".into()),
    });
    app.update();

    assert!(matches!(
        app.world().get::<AgentRunState>(entity),
        Some(AgentRunState::Idle)
    ));
    let queue = app.world().get::<PromptQueue>(entity).unwrap();
    assert!(queue.flush_pending());
    assert!(!queue.paused);
    assert_eq!(
        queue.items.front().map(|item| item.text.as_str()),
        Some("retry")
    );
}

#[test]
fn acp_streaming_to_idle_raises_attention() {
    use crate::components::PromptQueue;
    let mut app = App::new();
    app.add_message::<PageAgentDelta>()
        .add_message::<PageAgentRunStatus>()
        .add_message::<PageAgentAwaitingApproval>()
        .add_message::<PageAgentApprovalResolved>()
        .add_message::<PageAgentSnapshot>()
        .add_message::<vmux_core::notify::AgentAttention>()
        .add_systems(Update, consume_page_agent_stream);
    let entity = app
        .world_mut()
        .spawn((
            AcpSession {
                agent_id: "mistral-vibe".into(),
                sid: "s1".into(),
                cwd: std::path::PathBuf::from("/tmp"),
                anchor: vmux_core::ProcessId::new(),
                resume: None,
            },
            AgentMessages::default(),
            AgentRunState::Streaming,
            PromptQueue::default(),
        ))
        .id();

    app.world_mut()
        .resource_mut::<bevy::ecs::message::Messages<PageAgentRunStatus>>()
        .write(PageAgentRunStatus {
            sid: "s1".into(),
            status: AgentRunStatus::Idle,
        });
    app.update();

    let atts: Vec<_> = app
        .world_mut()
        .resource_mut::<bevy::ecs::message::Messages<vmux_core::notify::AgentAttention>>()
        .drain()
        .collect();
    assert_eq!(atts.len(), 1);
    assert_eq!(atts[0].entity, entity);
}

#[test]
fn idle_to_idle_does_not_raise_attention() {
    use crate::components::PromptQueue;
    let mut app = App::new();
    app.add_message::<PageAgentDelta>()
        .add_message::<PageAgentRunStatus>()
        .add_message::<PageAgentAwaitingApproval>()
        .add_message::<PageAgentApprovalResolved>()
        .add_message::<PageAgentSnapshot>()
        .add_message::<vmux_core::notify::AgentAttention>()
        .add_systems(Update, consume_page_agent_stream);
    app.world_mut().spawn((
        AcpSession {
            agent_id: "mistral-vibe".into(),
            sid: "s1".into(),
            cwd: std::path::PathBuf::from("/tmp"),
            anchor: vmux_core::ProcessId::new(),
            resume: None,
        },
        AgentMessages::default(),
        AgentRunState::Idle,
        PromptQueue::default(),
    ));

    app.world_mut()
        .resource_mut::<bevy::ecs::message::Messages<PageAgentRunStatus>>()
        .write(PageAgentRunStatus {
            sid: "s1".into(),
            status: AgentRunStatus::Idle,
        });
    app.update();

    let count = app
        .world_mut()
        .resource_mut::<bevy::ecs::message::Messages<vmux_core::notify::AgentAttention>>()
        .drain()
        .count();
    assert_eq!(count, 0);
}

#[test]
fn remote_approval_resolution_restores_streaming_state() {
    let mut app = App::new();
    app.add_message::<PageAgentDelta>()
        .add_message::<PageAgentRunStatus>()
        .add_message::<PageAgentAwaitingApproval>()
        .add_message::<PageAgentApprovalResolved>()
        .add_message::<PageAgentSnapshot>()
        .add_message::<vmux_core::notify::AgentAttention>()
        .add_systems(Update, consume_page_agent_stream);
    let entity = app
        .world_mut()
        .spawn((
            AcpSession {
                agent_id: "mistral-vibe".into(),
                sid: "s1".into(),
                cwd: std::path::PathBuf::from("/tmp"),
                anchor: vmux_core::ProcessId::new(),
                resume: None,
            },
            AgentMessages::default(),
            AgentRunState::AwaitingApproval {
                call_id: "call-1".into(),
                name: "run".into(),
                args: serde_json::json!({}),
            },
            PromptQueue::default(),
        ))
        .id();
    app.world_mut().write_message(PageAgentApprovalResolved {
        sid: "s1".into(),
        call_id: "call-1".into(),
    });

    app.update();

    assert!(matches!(
        app.world().get::<AgentRunState>(entity),
        Some(AgentRunState::Streaming)
    ));
}
