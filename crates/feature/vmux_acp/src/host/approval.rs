use bevy::prelude::*;
use bevy_cef::prelude::UiInput;
use vmux_api::protocol::{ClientMessage, SharedMessage};
use vmux_ecs::service::ServiceRequest;
use vmux_ecs::{Cwd, EntityTarget};
use vmux_session::event::ChatApproval;
use vmux_session::{AgentId, RunState, Session, SessionId};

use crate::approval::{Grants, Remember, StoreRequested};
use crate::host::event::{AgentApprovalReply, ApprovalDecision};

pub(super) struct Plugin;

impl bevy::prelude::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<crate::approval::Plugin>() {
            app.add_plugins(crate::approval::Plugin);
        }
        app.add_plugins(super::approval_io::Plugin)
            .add_message::<ServiceRequest>()
            .add_systems(Startup, spawn)
            .add_observer(receive)
            .add_observer(reply)
            .add_systems(
                Update,
                auto_allow
                    .after(crate::approval::Resolve)
                    .before(vmux_session::TranscriptReadySet),
            );
    }
}

fn spawn(mut stores: ResMut<Messages<StoreRequested>>) {
    if !stores.is_empty() {
        return;
    }
    stores.write(StoreRequested(
        vmux_ecs::profile::ProfilePaths::current()
            .profile()
            .join("agent-approvals.json"),
    ));
}

fn receive(
    trigger: On<UiInput<ChatApproval>>,
    child_of: Query<&ChildOf>,
    targets: Query<&EntityTarget<Session>>,
    mut commands: Commands,
) {
    let Ok(parent) = child_of.get(trigger.event().webview) else {
        return;
    };
    let Ok(target) = targets.get(parent.parent()) else {
        return;
    };
    commands.trigger(AgentApprovalReply {
        session: target.entity(),
        call_id: trigger.event().payload.call_id.clone(),
        decision: trigger.event().payload.decision,
    });
}

fn reply(
    trigger: On<AgentApprovalReply>,
    mut sessions: Query<(&mut RunState, &SessionId, &AgentId, &Cwd), With<Session>>,
    grants: Grants,
    mut remembered: MessageWriter<Remember>,
    mut service: MessageWriter<ServiceRequest>,
) {
    let reply = trigger.event();
    let Ok((mut state, id, agent, cwd)) = sessions.get_mut(reply.session) else {
        return;
    };
    let RunState::AwaitingApproval { call_id, name, .. } = &*state else {
        return;
    };
    if call_id != &reply.call_id {
        return;
    }
    if reply.decision == ApprovalDecision::AllowAlways
        && let Some(file) = grants.active()
    {
        remembered.write(Remember {
            file,
            session: reply.session,
            agent: agent.clone(),
            cwd: cwd.clone(),
            tool: name.clone(),
        });
    }
    service.write(ServiceRequest(ClientMessage::Shared(
        SharedMessage::AgentApprove {
            sid: id.0.clone(),
            call_id: reply.call_id.clone(),
            decision: reply.decision,
        },
    )));
    *state = RunState::Streaming;
}

fn auto_allow(
    mut sessions: Query<(Entity, &SessionId, &mut RunState), With<Session>>,
    grants: Grants,
    mut service: MessageWriter<ServiceRequest>,
) {
    for (session, id, mut state) in &mut sessions {
        let RunState::AwaitingApproval { call_id, name, .. } = &*state else {
            continue;
        };
        if !grants.allows(session, name) {
            continue;
        }
        service.write(ServiceRequest(ClientMessage::Shared(
            SharedMessage::AgentApprove {
                sid: id.0.clone(),
                call_id: call_id.clone(),
                decision: ApprovalDecision::AllowAlways,
            },
        )));
        *state = RunState::Streaming;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::approval::{Binding, File};
    use serde_json::json;
    use std::path::{Path, PathBuf};
    use std::time::{Duration, Instant};
    use vmux_ecs::Active;

    struct Fixture {
        app: App,
        directory: tempfile::TempDir,
        path: PathBuf,
        file: Entity,
        next: u64,
    }

    impl Fixture {
        fn new(content: Option<&str>) -> Self {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("approvals.json");
            if let Some(content) = content {
                std::fs::write(&path, content).unwrap();
            }
            let mut value = Self {
                app: App::new(),
                directory,
                path,
                file: Entity::PLACEHOLDER,
                next: 0,
            };
            value.install();
            value.wait(|fixture| {
                let file = fixture.app.world().get::<File>(fixture.file).unwrap();
                file.loaded || file.read_failed
            });
            value
        }

        fn install(&mut self) {
            self.app = App::new();
            self.app
                .add_plugins((bevy::app::TaskPoolPlugin::default(), super::Plugin));
            self.app
                .world_mut()
                .write_message(StoreRequested(self.path.clone()));
            self.app.update();
            self.file = self
                .app
                .world_mut()
                .query_filtered::<Entity, (With<File>, With<Active>)>()
                .single(self.app.world())
                .unwrap();
        }

        fn wait(&mut self, mut ready: impl FnMut(&mut Self) -> bool) {
            let deadline = Instant::now() + Duration::from_secs(8);
            loop {
                self.app.update();
                if ready(self) {
                    return;
                }
                assert!(
                    Instant::now() < deadline,
                    "approval production workflow did not settle"
                );
                std::thread::sleep(Duration::from_millis(1));
            }
        }

        fn session(&mut self, agent: &str, cwd: &Path, call: &str, tool: &str) -> Entity {
            self.next += 1;
            let session = self
                .app
                .world_mut()
                .spawn((
                    Session,
                    SessionId(format!("s{}", self.next)),
                    AgentId(agent.into()),
                    Cwd(cwd.into()),
                    RunState::AwaitingApproval {
                        call_id: call.into(),
                        name: tool.into(),
                        args: json!({}),
                    },
                ))
                .id();
            self.app.update();
            session
        }

        fn reply(&mut self, session: Entity, call: &str, decision: ApprovalDecision) {
            self.app.world_mut().trigger(AgentApprovalReply {
                session,
                call_id: call.into(),
                decision,
            });
            self.app.update();
        }

        fn requests(&mut self) -> Vec<ServiceRequest> {
            self.app
                .world_mut()
                .resource_mut::<Messages<ServiceRequest>>()
                .drain()
                .collect()
        }

        fn saved(&mut self, agent: &str, tool: &str) -> bool {
            let Ok(bytes) = std::fs::read(&self.path) else {
                return false;
            };
            let Ok(stored) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
                return false;
            };
            let Some(scopes) = stored["by_agent"][agent].as_object() else {
                return false;
            };
            scopes.values().any(|tools| {
                tools.as_array().is_some_and(|tools| {
                    tools
                        .iter()
                        .any(|candidate| candidate.as_str() == Some(tool))
                })
            })
        }
    }

    #[test]
    fn stale_call_cannot_send_a_reply_or_remember_a_grant() {
        let mut fixture = Fixture::new(None);
        let directory = fixture.directory.path().to_path_buf();
        let session = fixture.session("one", &directory, "current", "edit");
        fixture.reply(session, "stale", ApprovalDecision::AllowAlways);
        assert!(fixture.requests().is_empty());
        assert!(
            matches!(fixture.app.world().get::<RunState>(session), Some(RunState::AwaitingApproval { call_id, .. }) if call_id == "current")
        );
        assert!(!fixture.path.exists());
    }

    #[test]
    fn one_time_and_denied_decisions_preserve_wire_scope_and_do_not_persist() {
        for decision in [ApprovalDecision::Allow, ApprovalDecision::Deny] {
            let mut fixture = Fixture::new(None);
            let directory = fixture.directory.path().to_path_buf();
            let session = fixture.session("one", &directory, "call", "edit");
            fixture.reply(session, "call", decision);
            let requests = fixture.requests();
            assert!(
                matches!(requests.as_slice(), [ServiceRequest(ClientMessage::Shared(SharedMessage::AgentApprove { sid, call_id, decision: actual }))] if sid == "s1" && call_id == "call" && *actual == decision)
            );
            fixture.reply(session, "call", decision);
            assert!(fixture.requests().is_empty());
            assert!(!fixture.path.exists());
        }
    }

    #[test]
    fn persisted_grant_automatically_answers_only_the_matching_agent_and_call_once() {
        let mut fixture = Fixture::new(None);
        let directory = fixture.directory.path().to_path_buf();
        vmux_git::worktree::CheckoutInfo::initialize(&directory).unwrap();
        let session = fixture.session("codex-acp", &directory, "first", "mcp__vmux__run");
        fixture.reply(session, "first", ApprovalDecision::AllowAlways);
        fixture.wait(|fixture| fixture.saved("codex-acp", "mcp_vmux_run"));
        fixture.install();
        fixture.wait(|fixture| {
            fixture
                .app
                .world()
                .get::<File>(fixture.file)
                .unwrap()
                .loaded
        });
        let session = fixture.session("codex-acp", &directory, "next", "mcp.vmux.run");
        fixture.wait(|fixture| {
            matches!(
                fixture.app.world().get::<RunState>(session),
                Some(RunState::Streaming)
            )
        });
        let requests = fixture.requests();
        assert!(
            matches!(requests.as_slice(), [ServiceRequest(ClientMessage::Shared(SharedMessage::AgentApprove { call_id, decision: ApprovalDecision::AllowAlways, .. }))] if call_id == "next")
        );
        fixture.app.update();
        assert!(fixture.requests().is_empty());
        let alias = fixture.session("codex", &directory, "alias", "mcp.vmux.run");
        fixture.wait(|fixture| fixture.app.world().get::<Binding>(alias).is_some());
        assert!(fixture.requests().is_empty());
        assert!(matches!(
            fixture.app.world().get::<RunState>(alias),
            Some(RunState::AwaitingApproval { .. })
        ));
    }

    #[test]
    fn failed_initial_read_preserves_the_file_and_retry_merges_other_agents() {
        let mut fixture = Fixture::new(Some("{"));
        let directory = fixture.directory.path().to_path_buf();
        let session = fixture.session("one", &directory, "first", "edit");
        fixture.reply(session, "first", ApprovalDecision::AllowAlways);
        fixture.wait(|fixture| {
            fixture
                .app
                .world()
                .get::<File>(fixture.file)
                .unwrap()
                .read_failed
        });
        assert_eq!(std::fs::read_to_string(&fixture.path).unwrap(), "{");
        let namespace = std::fs::canonicalize(&directory)
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let repaired = json!({"by_agent": {"other": {namespace: ["other_tool"]}}});
        std::fs::write(&fixture.path, serde_json::to_vec(&repaired).unwrap()).unwrap();
        fixture
            .app
            .world_mut()
            .entity_mut(session)
            .insert(RunState::AwaitingApproval {
                call_id: "retry".into(),
                name: "edit".into(),
                args: json!({}),
            });
        fixture.reply(session, "retry", ApprovalDecision::AllowAlways);
        fixture.wait(|fixture| fixture.saved("one", "edit"));
        let stored: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&fixture.path).unwrap()).unwrap();
        assert!(stored["by_agent"].get("one").is_some());
        assert_eq!(stored["by_agent"]["other"], repaired["by_agent"]["other"]);
    }

    #[test]
    fn quitting_before_scope_and_initial_read_completion_saves_accepted_permission() {
        let mut fixture = Fixture::new(None);
        let directory = fixture.directory.path().to_path_buf();
        let session = fixture.session("one", &directory, "first", "edit");
        fixture.app.world_mut().trigger(AgentApprovalReply {
            session,
            call_id: "first".into(),
            decision: ApprovalDecision::AllowAlways,
        });
        fixture.app.world_mut().write_message(AppExit::Success);
        fixture.app.update();
        assert!(fixture.saved("one", "edit"));
    }

    #[test]
    fn quitting_after_a_pending_older_write_keeps_the_newest_grant() {
        let mut fixture = Fixture::new(None);
        let directory = fixture.directory.path().to_path_buf();
        let session = fixture.session("one", &directory, "first", "edit");
        fixture.reply(session, "first", ApprovalDecision::AllowAlways);
        fixture.wait(|fixture| {
            fixture
                .app
                .world()
                .get::<File>(fixture.file)
                .unwrap()
                .write
                .is_some()
                || fixture.saved("one", "edit")
        });
        fixture
            .app
            .world_mut()
            .entity_mut(session)
            .insert(RunState::AwaitingApproval {
                call_id: "second".into(),
                name: "run".into(),
                args: json!({}),
            });
        fixture.app.world_mut().trigger(AgentApprovalReply {
            session,
            call_id: "second".into(),
            decision: ApprovalDecision::AllowAlways,
        });
        fixture.app.world_mut().write_message(AppExit::Success);
        fixture.app.update();
        assert!(fixture.saved("one", "edit"));
        assert!(fixture.saved("one", "run"));
    }

    #[test]
    fn noncanonical_legacy_keys_do_not_broaden_authorization_during_loading() {
        let mut fixture = Fixture::new(None);
        let directory = fixture.directory.path().to_path_buf();
        let namespace = std::fs::canonicalize(&directory)
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let input = json!({"by_agent": {"ONE": {namespace.clone(): ["edit"]}, "one": {namespace: ["mcp.vmux.run"]}}});
        std::fs::write(&fixture.path, serde_json::to_vec(&input).unwrap()).unwrap();
        fixture.install();
        fixture.wait(|fixture| {
            fixture
                .app
                .world()
                .get::<File>(fixture.file)
                .unwrap()
                .loaded
        });
        let session = fixture.session("one", &directory, "current", "mcp__vmux__run");
        fixture.wait(|fixture| fixture.app.world().get::<Binding>(session).is_some());
        assert!(fixture.requests().is_empty());
        assert!(matches!(
            fixture.app.world().get::<RunState>(session),
            Some(RunState::AwaitingApproval { .. })
        ));
    }
}
