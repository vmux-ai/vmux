use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use vmux_api::protocol::AgentPromptEnvelope;
use vmux_api::session::Message;
use vmux_ecs::jobs::{Job, JobPlugin, JobRequest, JobResult, JobSet};
use vmux_ecs::{EntityTarget, Operation};
use vmux_session::host::{HandoffMessages, ImportedConversation};

use crate::AcpSessionId;

pub(super) struct HandoffPlugin;

impl Plugin for HandoffPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<vmux_ecs::OperationPlugin>() {
            app.add_plugins(vmux_ecs::OperationPlugin);
        }
        app.add_plugins((
            JobPlugin::<LoadHandoffJob>::default(),
            JobPlugin::<SaveHandoffJob>::default(),
        ))
        .add_systems(Startup, spawn)
        .add_systems(
            Update,
            (
                request_load.before(JobSet::Submit),
                request_save
                    .after(vmux_ecs::service::ServiceMessageSet)
                    .before(JobSet::Submit),
                apply_load.after(JobSet::Apply),
                apply_save.after(JobSet::Apply),
            ),
        );
    }
}

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct PendingHandoff {
    pub context: String,
    pub sent: bool,
}

#[derive(Component)]
struct HandoffRoot(PathBuf);

impl HandoffRoot {
    fn record_path(&self, agent_id: &str, session_id: &str) -> PathBuf {
        self.0
            .join(Self::hex_component(agent_id))
            .join(format!("{}.json", Self::hex_component(session_id)))
    }

    fn hex_component(value: &str) -> String {
        let mut encoded = String::with_capacity(value.len() * 2);
        for byte in value.as_bytes() {
            encoded.push_str(&format!("{byte:02x}"));
        }
        encoded
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct HandoffRecord {
    source_agent: String,
    source_sid: String,
    messages: Vec<Message>,
    truncated: bool,
    first_prompt: Option<String>,
}

impl HandoffRecord {
    const LIMIT: usize = 16 * 1024 * 1024;

    fn decode(bytes: &[u8]) -> Option<Self> {
        let mut record = serde_json::from_slice::<Self>(bytes).ok()?;
        let mut fallback = record.first_prompt.as_deref();
        for message in &mut record.messages {
            let Message::User { text, .. } = message else {
                continue;
            };
            let prompt = AgentPromptEnvelope::new(text);
            if let Some(display_text) = prompt.display().map(str::to_string) {
                *text = display_text;
            } else if prompt.has_private_context()
                && let Some(display_text) = fallback.take()
            {
                *text = display_text.to_string();
            }
        }
        Some(record)
    }

    fn encode(&self) -> Result<Vec<u8>, String> {
        let bytes = serde_json::to_vec(self)
            .map_err(|error| format!("serialize handoff record: {error}"))?;
        if bytes.len() > Self::LIMIT {
            return Err("handoff record exceeds the 16 MiB limit".into());
        }
        Ok(bytes)
    }
}

#[derive(Clone)]
struct LoadHandoffJob(PathBuf);

impl Job for LoadHandoffJob {
    type Output = Option<HandoffRecord>;

    fn run(self) -> Self::Output {
        if vmux_ecs::BlockingCancellation::canceled() {
            return None;
        }
        let source = std::fs::File::open(&self.0).ok()?;
        let mut bytes = Vec::new();
        source
            .take(HandoffRecord::LIMIT as u64 + 1)
            .read_to_end(&mut bytes)
            .ok()?;
        if bytes.len() > HandoffRecord::LIMIT {
            return None;
        }
        HandoffRecord::decode(&bytes)
    }
}

#[derive(Clone)]
struct SaveHandoffJob {
    path: PathBuf,
    bytes: Arc<[u8]>,
}

impl Job for SaveHandoffJob {
    type Output = Result<(), String>;

    fn run(self) -> Self::Output {
        if vmux_ecs::BlockingCancellation::canceled() {
            return Err("handoff write canceled".into());
        }
        let parent = self
            .path
            .parent()
            .ok_or_else(|| format!("invalid handoff path {}", self.path.display()))?;
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("create handoff directory {}: {error}", parent.display()))?;
        vmux_path::AtomicFile::write(&self.path, &self.bytes)
            .map_err(|error| format!("write handoff record {}: {error}", self.path.display()))
    }
}

#[derive(Component)]
struct LoadHandoff {
    agent_id: String,
    session_id: String,
}

#[derive(Component)]
struct SaveHandoff;

fn spawn(existing: Query<(), With<HandoffRoot>>, mut commands: Commands) {
    if !existing.is_empty() {
        return;
    }
    commands.spawn((
        Name::new("Agent handoff directory"),
        HandoffRoot(
            vmux_ecs::profile::ProfilePaths::current()
                .profile()
                .join("handoffs"),
        ),
    ));
}

fn request_load(
    root: Single<&HandoffRoot>,
    sessions: Query<
        (Entity, &vmux_session::AgentId, &AcpSessionId),
        (Added<AcpSessionId>, Without<ImportedConversation>),
    >,
    mut commands: Commands,
) {
    for (session, agent_id, session_id) in &sessions {
        let path = root.record_path(&agent_id.0, &session_id.0);
        commands.spawn((
            Operation::bundle("acp.handoff.load").unwrap(),
            EntityTarget::<vmux_session::Session>::new(session),
            LoadHandoff {
                agent_id: agent_id.0.clone(),
                session_id: session_id.0.clone(),
            },
            JobRequest(LoadHandoffJob(path)),
        ));
    }
}

fn apply_load(
    completed: Query<(
        Entity,
        &EntityTarget<vmux_session::Session>,
        &LoadHandoff,
        &JobResult<LoadHandoffJob>,
    )>,
    sessions: Query<(&vmux_session::AgentId, &AcpSessionId), Without<ImportedConversation>>,
    mut commands: Commands,
) {
    for (operation, target, load, result) in &completed {
        let session = target.entity();
        if let Ok(Some(record)) = &result.0
            && let Ok((agent_id, session_id)) = sessions.get(session)
            && agent_id.0 == load.agent_id
            && session_id.0 == load.session_id
        {
            commands.entity(session).insert(ImportedConversation {
                source_agent: record.source_agent.clone(),
                source_sid: record.source_sid.clone(),
                truncated: record.truncated,
                first_prompt: record.first_prompt.clone(),
            });
            ImportedConversation::spawn_messages(&mut commands, session, record.messages.clone());
        }
        commands.entity(operation).despawn();
    }
}

fn request_save(
    root: Single<&HandoffRoot>,
    mut created: MessageReader<crate::host::event::AcpSessionCreated>,
    sessions: Query<(
        Entity,
        &vmux_session::SessionId,
        &vmux_session::AgentId,
        &ImportedConversation,
    )>,
    handoffs: HandoffMessages,
    mut commands: Commands,
) {
    for event in created.read() {
        for (session, session_id, agent_id, imported) in &sessions {
            if session_id.0 != event.sid || imported.first_prompt.is_none() {
                continue;
            }
            let record = HandoffRecord {
                source_agent: imported.source_agent.clone(),
                source_sid: imported.source_sid.clone(),
                messages: handoffs.messages(session),
                truncated: imported.truncated,
                first_prompt: imported.first_prompt.clone(),
            };
            let bytes = match record.encode() {
                Ok(bytes) => bytes,
                Err(error) => {
                    bevy::log::warn!("acp: failed to encode handoff metadata: {error}");
                    continue;
                }
            };
            let path = root.record_path(&agent_id.0, &event.acp_session_id);
            commands.spawn((
                Operation::bundle("acp.handoff.save").unwrap(),
                SaveHandoff,
                JobRequest(SaveHandoffJob {
                    path,
                    bytes: bytes.into(),
                }),
            ));
        }
    }
}

fn apply_save(
    completed: Query<(Entity, &JobResult<SaveHandoffJob>), With<SaveHandoff>>,
    mut commands: Commands,
) {
    for (operation, result) in &completed {
        if let Err(error) = &result.0 {
            bevy::log::warn!("acp: failed to persist handoff metadata: {error}");
        }
        commands.entity(operation).despawn();
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use bevy::ecs::system::RunSystemOnce;
    use vmux_api::session::{AssistantBlock, Message};

    use super::*;

    struct HandoffFixture {
        app: App,
        directory: tempfile::TempDir,
    }

    impl HandoffFixture {
        fn new() -> Self {
            let directory = tempfile::tempdir().unwrap();
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, HandoffPlugin))
                .add_message::<crate::host::event::AcpSessionCreated>();
            app.world_mut()
                .spawn(HandoffRoot(directory.path().to_path_buf()));
            app.update();
            Self { app, directory }
        }

        fn path(&self, agent_id: &str, session_id: &str) -> PathBuf {
            HandoffRoot(self.directory.path().to_path_buf()).record_path(agent_id, session_id)
        }

        fn save_and_wait(
            &mut self,
            sid: &str,
            agent_id: &str,
            acp_session_id: &str,
            imported: ImportedConversation,
            messages: Vec<Message>,
        ) {
            let session = self
                .app
                .world_mut()
                .spawn((
                    vmux_session::SessionId(sid.into()),
                    vmux_session::AgentId(agent_id.into()),
                    imported,
                ))
                .id();
            let mut pending_messages = Some(messages);
            self.app
                .world_mut()
                .run_system_once(move |mut commands: Commands| {
                    ImportedConversation::spawn_messages(
                        &mut commands,
                        session,
                        pending_messages.take().unwrap(),
                    );
                })
                .unwrap();
            self.app
                .world_mut()
                .write_message(crate::host::event::AcpSessionCreated {
                    sid: sid.into(),
                    acp_session_id: acp_session_id.into(),
                });
            let deadline = Instant::now() + Duration::from_secs(5);
            while Instant::now() < deadline {
                self.app.update();
                let pending = self
                    .app
                    .world_mut()
                    .query_filtered::<Entity, With<SaveHandoff>>()
                    .iter(self.app.world())
                    .count();
                if pending == 0 {
                    return;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            panic!("timed out waiting for handoff save");
        }

        fn load_and_wait(
            &mut self,
            agent_id: &str,
            acp_session_id: &str,
        ) -> (Option<ImportedConversation>, Vec<Message>) {
            let session = self
                .app
                .world_mut()
                .spawn((
                    vmux_session::AgentId(agent_id.into()),
                    AcpSessionId(acp_session_id.into()),
                ))
                .id();
            let deadline = Instant::now() + Duration::from_secs(5);
            while Instant::now() < deadline {
                self.app.update();
                let pending = self
                    .app
                    .world_mut()
                    .query_filtered::<Entity, With<LoadHandoff>>()
                    .iter(self.app.world())
                    .count();
                if pending == 0 {
                    let imported = self
                        .app
                        .world()
                        .get::<ImportedConversation>(session)
                        .cloned();
                    let messages = self
                        .app
                        .world_mut()
                        .run_system_once(move |handoffs: HandoffMessages| {
                            handoffs.messages(session)
                        })
                        .unwrap();
                    return (imported, messages);
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            panic!("timed out waiting for handoff load");
        }

        fn user(text: &str) -> Message {
            Message::user(text)
        }

        fn assistant(text: &str) -> Message {
            Message::Assistant {
                blocks: vec![AssistantBlock::Text(text.to_string())],
            }
        }
    }

    #[test]
    fn private_wire_prompt_keeps_display_prompt_separate() {
        let prompt = AgentPromptEnvelope::compose("continue here", Some("prior conversation"));

        assert!(AgentPromptEnvelope::new(&prompt).has_private_context());
        assert!(prompt.contains("prior conversation"));
        assert!(prompt.ends_with("continue here"));
    }

    #[test]
    fn replay_private_prompt_is_replaced_with_display_prompt() {
        let mut fixture = HandoffFixture::new();
        fixture.save_and_wait(
            "s1",
            "agent",
            "session",
            ImportedConversation {
                source_agent: String::new(),
                source_sid: String::new(),
                truncated: false,
                first_prompt: Some("continue here".into()),
            },
            vec![
                HandoffFixture::user(&AgentPromptEnvelope::compose(
                    "continue here",
                    Some("prior conversation"),
                )),
                HandoffFixture::assistant("done"),
            ],
        );
        let (_, messages) = fixture.load_and_wait("agent", "session");

        assert_eq!(messages[0], HandoffFixture::user("continue here"));
        assert_eq!(messages[1], HandoffFixture::assistant("done"));
    }

    #[test]
    fn replay_sanitizes_every_retried_private_prompt_from_its_own_payload() {
        let mut fixture = HandoffFixture::new();
        fixture.save_and_wait(
            "s2",
            "agent",
            "retry",
            ImportedConversation {
                source_agent: String::new(),
                source_sid: String::new(),
                truncated: false,
                first_prompt: Some("stale sidecar text".into()),
            },
            vec![
                HandoffFixture::user(&AgentPromptEnvelope::compose(
                    "first try",
                    Some("prior conversation"),
                )),
                HandoffFixture::user(&AgentPromptEnvelope::compose(
                    "second try",
                    Some("prior conversation"),
                )),
            ],
        );
        let (_, messages) = fixture.load_and_wait("agent", "retry");

        assert_eq!(
            messages,
            vec![
                HandoffFixture::user("first try"),
                HandoffFixture::user("second try"),
            ]
        );
    }

    #[test]
    fn replay_preserves_plain_prompt_starting_with_private_prefix() {
        let text = "<vmux_handoff_context> ordinary user text";
        let mut fixture = HandoffFixture::new();
        fixture.save_and_wait(
            "s3",
            "agent",
            "plain",
            ImportedConversation {
                source_agent: String::new(),
                source_sid: String::new(),
                truncated: false,
                first_prompt: Some("fallback".into()),
            },
            vec![HandoffFixture::user(text)],
        );
        let (_, messages) = fixture.load_and_wait("agent", "plain");

        assert_eq!(messages, vec![HandoffFixture::user(text)]);
    }

    #[test]
    fn imported_conversation_sidecar_round_trips() {
        let mut fixture = HandoffFixture::new();
        let imported = ImportedConversation {
            source_agent: "Codex".into(),
            source_sid: "cx/1".into(),
            truncated: true,
            first_prompt: Some("continue".into()),
        };
        let expected_messages = vec![
            HandoffFixture::user("fix auth"),
            HandoffFixture::assistant("working"),
        ];

        fixture.save_and_wait(
            "s4",
            "claude/custom",
            "target?1",
            imported.clone(),
            expected_messages.clone(),
        );
        let (loaded, messages) = fixture.load_and_wait("claude/custom", "target?1");

        assert_eq!(loaded, Some(imported));
        assert_eq!(messages, expected_messages);
        assert!(
            fixture
                .path("claude/custom", "target?1")
                .starts_with(fixture.directory.path())
        );
    }

    #[test]
    fn missing_or_malformed_sidecar_is_ignored() {
        let mut fixture = HandoffFixture::new();
        let (missing, missing_messages) = fixture.load_and_wait("claude", "missing");
        assert!(missing.is_none());
        assert!(missing_messages.is_empty());

        let path = fixture.path("claude", "bad");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "not json").unwrap();
        let (bad, bad_messages) = fixture.load_and_wait("claude", "bad");
        assert!(bad.is_none());
        assert!(bad_messages.is_empty());
    }
}
