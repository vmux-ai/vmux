use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use bevy_app::{App, Plugin as BevyPlugin, Update};
use bevy_ecs::prelude::*;
use bevy_ecs::system::SystemParam;
use vmux_api::session::{AssistantBlock, Message};
use vmux_ecs::{CreatedAt, EntityTarget};

use crate::{
    ConversationEvent, EventIdentity, MessageContent, ProviderKind, ProviderSessionId,
    ProviderSessionKey, Session, SessionId, TranscriptReadySet,
};

pub struct Plugin;

impl BevyPlugin for Plugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(crate::conversation::ConversationPlugin)
            .configure_sets(Update, (Prepare, Ready, TranscriptReadySet).chain())
            .add_message::<Received>()
            .add_message::<Committed>()
            .add_systems(Update, cancel.before(Prepare));
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Prepare;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Ready;

#[derive(Component, Clone)]
pub struct Read {
    pub(super) session: SessionId,
    pub(super) source: ProviderSessionKey,
    pub(super) fingerprint: u64,
    pub(super) failed: bool,
}

#[derive(Component)]
pub struct Applied;

#[derive(Message, Clone)]
pub struct Received {
    pub operation: Entity,
    pub entries: Vec<(Option<String>, Message)>,
    pub tail: bool,
    pub error: Option<String>,
    pub updated_at: i64,
    pub file: Option<FileFacts>,
}

#[derive(Clone, Debug)]
pub struct FileFacts {
    pub identity: String,
    pub prefix: Vec<u8>,
    pub length: u64,
    pub cursor: u64,
    pub positions: Vec<(u64, usize)>,
}

impl Received {
    pub fn messages(messages: Vec<Message>, tail: bool) -> Self {
        let mut entries = Vec::with_capacity(messages.len());
        for message in messages {
            entries.push((None, message));
        }
        Self {
            operation: Entity::PLACEHOLDER,
            entries,
            tail,
            error: None,
            updated_at: 0,
            file: None,
        }
    }
}

#[derive(Message)]
pub struct Committed {
    pub operation: Entity,
    pub result: Result<bool, String>,
}

#[derive(SystemParam)]
pub struct Versions<'w, 's> {
    sessions: Query<
        'w,
        's,
        (
            &'static SessionId,
            &'static ProviderKind,
            &'static ProviderSessionId,
        ),
        With<Session>,
    >,
    children: Query<'w, 's, &'static Children, With<Session>>,
    files: Query<
        'w,
        's,
        (
            &'static ChildOf,
            &'static ProviderKind,
            &'static ProviderSessionId,
            &'static crate::conversation::FileHistory,
        ),
    >,
    events: Query<
        'w,
        's,
        (
            Ref<'static, EventIdentity>,
            Ref<'static, MessageContent>,
            Ref<'static, CreatedAt>,
        ),
        (
            With<ConversationEvent>,
            Without<crate::conversation::Retired>,
        ),
    >,
}

impl Versions<'_, '_> {
    pub fn file(&self, session: Entity) -> Option<FileFacts> {
        let (_, kind, provider) = self.sessions.get(session).ok()?;
        for (parent, file_kind, file_provider, file) in &self.files {
            if parent.parent() == session && file_kind == kind && file_provider == provider {
                return Some(file.facts());
            }
        }
        None
    }

    pub fn capture(&self, session: Entity, failed: bool) -> Option<Read> {
        let (sid, kind, provider) = self.sessions.get(session).ok()?;
        Some(Read {
            session: sid.clone(),
            source: ProviderSessionKey::new(kind.clone(), provider.clone()),
            fingerprint: self.fingerprint(session),
            failed,
        })
    }

    pub fn matches(&self, session: Entity, read: &Read) -> bool {
        self.sessions
            .get(session)
            .is_ok_and(|(sid, kind, provider)| {
                sid == &read.session && kind == &read.source.kind && provider == &read.source.id
            })
    }

    pub fn has_events(&self, session: Entity) -> bool {
        if let Ok(children) = self.children.get(session) {
            for child in children.iter() {
                if self.events.contains(child) {
                    return true;
                }
            }
        }
        false
    }

    pub(super) fn fingerprint(&self, session: Entity) -> u64 {
        let mut fingerprint = DefaultHasher::new();
        if let Ok(children) = self.children.get(session) {
            for child in children.iter() {
                let Ok((identity, content, created)) = self.events.get(child) else {
                    continue;
                };
                child.hash(&mut fingerprint);
                identity.last_changed().get().hash(&mut fingerprint);
                content.last_changed().get().hash(&mut fingerprint);
                created.last_changed().get().hash(&mut fingerprint);
            }
        }
        fingerprint.finish()
    }

    fn tail(&self, session: Entity) -> Vec<Message> {
        let mut selected = BinaryHeap::new();
        if let Ok(children) = self.children.get(session) {
            for (position, child) in children.iter().enumerate() {
                let Ok((identity, _, _)) = self.events.get(child) else {
                    continue;
                };
                let key = (identity.sequence, position, child);
                if selected.len() < 512 {
                    selected.push(Reverse(key));
                    continue;
                }
                if selected.peek().is_some_and(|Reverse(oldest)| *oldest < key) {
                    selected.pop();
                    selected.push(Reverse(key));
                }
            }
        }
        let mut ordered = Vec::with_capacity(selected.len());
        for Reverse(entry) in selected {
            ordered.push(entry);
        }
        ordered.sort_by_key(|(sequence, position, _)| (*sequence, *position));
        let mut messages = Vec::with_capacity(ordered.len());
        for (_, _, entity) in ordered {
            if let Ok((_, content, _)) = self.events.get(entity) {
                messages.push(content.0.clone());
            }
        }
        messages
    }

    fn units(messages: &[Message]) -> Vec<Message> {
        let mut units = Vec::new();
        for message in messages {
            match message {
                Message::Assistant { blocks } => {
                    for block in blocks {
                        units.push(Message::Assistant {
                            blocks: vec![block.clone()],
                        });
                    }
                }
                _ => units.push(message.clone()),
            }
        }
        units
    }

    pub(super) fn same(left: &Message, right: &Message) -> bool {
        match (left, right) {
            (
                Message::User {
                    text: a,
                    attachments: x,
                },
                Message::User {
                    text: b,
                    attachments: y,
                },
            ) => {
                vmux_api::protocol::AgentPromptEnvelope::new(a)
                    .display()
                    .unwrap_or(a)
                    == vmux_api::protocol::AgentPromptEnvelope::new(b)
                        .display()
                        .unwrap_or(b)
                    && x == y
            }
            (Message::Assistant { blocks: a }, Message::Assistant { blocks: b }) => {
                match (a.as_slice(), b.as_slice()) {
                    (
                        [
                            AssistantBlock::ToolUse {
                                call_id: a,
                                args: x,
                                ..
                            },
                        ],
                        [
                            AssistantBlock::ToolUse {
                                call_id: b,
                                args: y,
                                ..
                            },
                        ],
                    ) => {
                        if a != b {
                            return false;
                        }
                        if x == y {
                            return true;
                        }
                        let Ok(x) = serde_json::from_str::<serde_json::Value>(x) else {
                            return false;
                        };
                        let Ok(y) = serde_json::from_str::<serde_json::Value>(y) else {
                            return false;
                        };
                        x == y
                    }
                    _ => a == b,
                }
            }
            _ => left == right,
        }
    }

    pub(super) fn unseen(&self, session: Entity, incoming: Vec<Message>) -> Vec<Message> {
        let known = Self::units(&self.tail(session));
        let incoming = Self::units(&incoming);
        if incoming.is_empty() {
            return incoming;
        }
        let mut prefix = vec![0usize; incoming.len()];
        for position in 1..incoming.len() {
            let mut matched = prefix[position - 1];
            while matched > 0 && !Self::same(&incoming[position], &incoming[matched]) {
                matched = prefix[matched - 1];
            }
            if Self::same(&incoming[position], &incoming[matched]) {
                matched += 1;
            }
            prefix[position] = matched;
        }
        let mut overlap = 0usize;
        for message in &known {
            while overlap > 0
                && (overlap == incoming.len() || !Self::same(message, &incoming[overlap]))
            {
                overlap = prefix[overlap - 1];
            }
            if overlap < incoming.len() && Self::same(message, &incoming[overlap]) {
                overlap += 1;
            }
        }
        incoming[overlap..].to_vec()
    }
}

fn cancel(
    reads: Query<(Entity, &ChildOf, &Read), Without<Applied>>,
    sessions: Query<
        (
            &SessionId,
            &ProviderKind,
            &ProviderSessionId,
            &EntityTarget<Read>,
        ),
        With<Session>,
    >,
    mut committed: MessageWriter<Committed>,
    mut commands: Commands,
) {
    for (operation, parent, read) in &reads {
        let matches = sessions.get(parent.parent()).is_ok_and(
            |(current_sid, current_kind, current_provider, pending)| {
                current_sid == &read.session
                    && current_kind == &read.source.kind
                    && current_provider == &read.source.id
                    && pending.entity() == operation
            },
        );
        if !matches {
            commands.entity(operation).remove::<Read>();
            committed.write(Committed {
                operation,
                result: Ok(false),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ConversationAppendReceived, RunState, SnapshotReceived, Transcripts};
    use bevy_ecs::system::RunSystemOnce;

    use bevy_app::TaskPoolPlugin;
    use bevy_asset::AssetPlugin;
    use moonshine_save::prelude::Save;
    use moonshine_save::prelude::{LoadWorld, SaveWorld, TriggerLoad, TriggerSave};
    use vmux_ecs::store::StoreAppExt;

    struct Fixture {
        app: App,
        session: Entity,
    }

    impl Fixture {
        fn new(messages: Vec<Message>) -> Self {
            let mut app = App::new();
            app.add_plugins(Plugin);
            let session = app
                .world_mut()
                .spawn((
                    Session,
                    SessionId::new("canonical"),
                    ProviderKind("acp:example".into()),
                    ProviderSessionId("provider".into()),
                    RunState::Errored("active writer".into()),
                ))
                .id();
            app.world_mut().write_message(SnapshotReceived {
                session: SessionId::new("canonical"),
                messages,
            });
            app.update();
            Self { app, session }
        }

        fn operation(&mut self, failed: bool) -> Entity {
            let session = self.session;
            let read = self
                .app
                .world_mut()
                .run_system_once(move |versions: Versions| versions.capture(session, failed))
                .unwrap()
                .unwrap();
            let operation = self.app.world_mut().spawn((ChildOf(session), read)).id();
            self.app
                .world_mut()
                .entity_mut(session)
                .insert(EntityTarget::<Read>::new(operation));
            operation
        }

        fn receive(
            &mut self,
            operation: Entity,
            entries: Vec<(Option<String>, Message)>,
            tail: bool,
            error: Option<String>,
        ) -> Result<bool, String> {
            self.receive_file(operation, entries, tail, error, None)
        }

        fn receive_file(
            &mut self,
            operation: Entity,
            entries: Vec<(Option<String>, Message)>,
            tail: bool,
            error: Option<String>,
            file: Option<FileFacts>,
        ) -> Result<bool, String> {
            self.app.world_mut().write_message(Received {
                operation,
                entries,
                tail,
                error,
                updated_at: 42,
                file,
            });
            self.app.update();
            let completed = self
                .app
                .world_mut()
                .resource_mut::<Messages<Committed>>()
                .drain()
                .find(|event| event.operation == operation)
                .unwrap();
            completed.result
        }

        fn messages(&mut self) -> Vec<Message> {
            let session = self.session;
            self.app
                .world_mut()
                .run_system_once(move |transcripts: Transcripts| transcripts.get(session).messages)
                .unwrap()
        }

        fn roundtrip(&mut self) {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("history.ron");
            self.app
                .add_plugins((
                    TaskPoolPlugin::default(),
                    AssetPlugin::default(),
                    vmux_ecs::PrimitivesPlugin,
                ))
                .register_stored::<Session>()
                .register_stored::<SessionId>()
                .register_stored::<CreatedAt>()
                .add_observer(moonshine_save::save::save_on_default_event);
            let registry = self
                .app
                .world()
                .resource::<bevy_ecs::reflect::AppTypeRegistry>()
                .read();
            let filter = vmux_ecs::store::WorkspaceStored::filter(&registry)
                .allow::<Save>()
                .allow::<ChildOf>()
                .allow::<Children>()
                .allow::<Name>();
            drop(registry);
            let mut save = SaveWorld::default_into_file(path.clone());
            save.components = filter;
            self.app.world_mut().commands().trigger_save(save);
            self.app.update();
            assert!(path.exists());
            let mut restored = App::new();
            restored
                .add_plugins((
                    TaskPoolPlugin::default(),
                    AssetPlugin::default(),
                    vmux_ecs::PrimitivesPlugin,
                    Plugin,
                ))
                .register_stored::<Session>()
                .register_stored::<SessionId>()
                .register_stored::<CreatedAt>()
                .add_observer(moonshine_save::load::load_on_default_event);
            restored.update();
            restored
                .world_mut()
                .commands()
                .trigger_load(LoadWorld::default_from_file(path));
            restored.update();
            self.session = restored
                .world_mut()
                .query_filtered::<Entity, With<Session>>()
                .single(restored.world())
                .unwrap();
            self.app = restored;
        }

        fn external(&mut self, id: &str, message: Message) -> Entity {
            let operation = self.operation(false);
            assert_eq!(
                self.receive(operation, vec![(Some(id.into()), message)], false, None),
                Ok(true)
            );
            let source = self
                .app
                .world()
                .get::<ProviderKind>(self.session)
                .unwrap()
                .clone();
            let provider = self
                .app
                .world()
                .get::<ProviderSessionId>(self.session)
                .unwrap()
                .clone();
            self.app.world_mut()
                .query_filtered::<(&ChildOf, &Name, &ProviderKind, &ProviderSessionId), With<crate::conversation::NativeAlias>>()
                .iter(self.app.world())
                .find(|(_, name, kind, key)| name.as_str() == id && **kind == source && **key == provider)
                .unwrap()
                .0.parent()
        }
    }

    #[test]
    fn stale_or_malformed_file_reads_cannot_advance_the_accepted_generation() {
        let mut fixture = Fixture::new(Vec::new());
        let facts = FileFacts {
            identity: "original".into(),
            prefix: b"prefix".to_vec(),
            length: 100,
            cursor: 20,
            positions: vec![(10, 0)],
        };
        let operation = fixture.operation(false);
        assert_eq!(
            fixture.receive_file(
                operation,
                vec![(None, Message::user("kept"))],
                true,
                None,
                Some(facts.clone())
            ),
            Ok(true)
        );
        let stale = fixture.operation(false);
        let current = fixture.operation(false);
        let replacement = FileFacts {
            identity: "replacement".into(),
            ..facts.clone()
        };
        assert_eq!(
            fixture.receive_file(
                stale,
                vec![(None, Message::user("stale"))],
                true,
                None,
                Some(replacement.clone())
            ),
            Ok(false)
        );
        assert_eq!(
            fixture.receive_file(
                current,
                vec![(None, Message::user("kept"))],
                true,
                None,
                Some(facts.clone())
            ),
            Ok(true)
        );
        assert_eq!(fixture.messages(), [Message::user("kept")]);
        for positions in [Vec::new(), vec![(20, 0)], vec![(10, 0), (10, 0)]] {
            let operation = fixture.operation(false);
            let entries = if positions.len() == 2 {
                vec![(None, Message::user("bad")); 2]
            } else {
                vec![(None, Message::user("bad"))]
            };
            assert_eq!(
                fixture.receive_file(
                    operation,
                    entries,
                    true,
                    None,
                    Some(FileFacts {
                        positions,
                        ..replacement.clone()
                    })
                ),
                Err("invalid native file positions".into())
            );
        }
        let operation = fixture.operation(false);
        assert_eq!(
            fixture.receive_file(
                operation,
                vec![(None, Message::user("kept"))],
                true,
                None,
                Some(facts)
            ),
            Ok(true)
        );
        assert_eq!(fixture.messages(), [Message::user("kept")]);
    }

    #[test]
    fn stored_aliases_restore_retired_events_and_preserve_operations_and_file_retry() {
        let a = AssistantBlock::Text("a".into());
        let b = AssistantBlock::Text("b".into());
        let c = AssistantBlock::Text("c".into());
        let mut fixture = Fixture::new(vec![
            Message::Assistant {
                blocks: vec![a.clone()],
            },
            Message::Assistant {
                blocks: vec![b.clone()],
            },
            Message::Assistant {
                blocks: vec![c.clone()],
            },
        ]);
        let operation = fixture.operation(false);
        assert_eq!(
            fixture.receive(
                operation,
                vec![(
                    Some("group".into()),
                    Message::Assistant {
                        blocks: vec![a.clone(), b.clone(), c.clone()]
                    }
                )],
                false,
                None
            ),
            Ok(true)
        );
        let identity = fixture
            .app
            .world_mut()
            .query::<(&EventIdentity, &MessageContent)>()
            .iter(fixture.app.world())
            .find(|(_, content)| {
                content.0
                    == Message::Assistant {
                        blocks: vec![c.clone()],
                    }
            })
            .unwrap()
            .0
            .clone();
        fixture
            .app
            .world_mut()
            .write_message(crate::OperationCommitted(
                vmux_api::session::ConversationEvent {
                    event_id: identity.event_id.clone(),
                    session_id: SessionId::new("canonical"),
                    actor_id: identity.actor_id.clone(),
                    client_op_id: identity.client_op_id.clone(),
                    server_seq: identity.sequence,
                    created_at_ms: 42,
                    reply_to: identity.reply_to.clone(),
                    message: Message::Assistant {
                        blocks: vec![c.clone()],
                    },
                },
            ));
        fixture.app.update();
        let operation = fixture.operation(false);
        assert_eq!(
            fixture.receive(
                operation,
                vec![(
                    Some("group".into()),
                    Message::Assistant {
                        blocks: vec![a.clone()]
                    }
                )],
                false,
                None
            ),
            Ok(true)
        );
        let facts = FileFacts {
            identity: "stable".into(),
            prefix: b"prefix".to_vec(),
            length: 100,
            cursor: 20,
            positions: vec![(10, 0)],
        };
        let operation = fixture.operation(false);
        assert_eq!(
            fixture.receive_file(
                operation,
                vec![(None, Message::user("file entry"))],
                true,
                None,
                Some(facts.clone())
            ),
            Ok(true)
        );
        let expected = vec![
            Message::Assistant {
                blocks: vec![a.clone()],
            },
            Message::Assistant {
                blocks: vec![c.clone()],
            },
            Message::user("file entry"),
        ];
        assert_eq!(fixture.messages(), expected);
        fixture.roundtrip();
        assert_eq!(fixture.messages(), expected);
        let operation = fixture.operation(false);
        assert_eq!(
            fixture.receive_file(
                operation,
                vec![(None, Message::user("file entry"))],
                true,
                None,
                Some(facts)
            ),
            Ok(true)
        );
        assert_eq!(fixture.messages(), expected);
        let operation = fixture.operation(false);
        assert_eq!(
            fixture.receive(
                operation,
                vec![(
                    Some("group".into()),
                    Message::Assistant {
                        blocks: vec![a.clone(), b.clone(), c.clone()]
                    }
                )],
                true,
                None
            ),
            Ok(true)
        );
        assert_eq!(
            fixture.messages(),
            vec![
                Message::Assistant {
                    blocks: vec![a.clone()]
                },
                Message::Assistant { blocks: vec![b] },
                Message::Assistant {
                    blocks: vec![c.clone()]
                },
                Message::user("file entry")
            ]
        );
        let operation = fixture.operation(false);
        assert_eq!(
            fixture.receive(
                operation,
                vec![(Some("group".into()), Message::Assistant { blocks: vec![a] })],
                true,
                None
            ),
            Ok(true)
        );
        assert_eq!(fixture.messages(), expected);
    }

    #[test]
    fn file_position_retry_reuses_events_outside_the_tail_and_keeps_new_repeated_text() {
        let mut fixture = Fixture::new(Vec::new());
        let mut entries = Vec::new();
        let mut positions = Vec::new();
        for number in 0..600 {
            entries.push((None, Message::user(format!("message {number}"))));
            positions.push((number * 10, 0));
        }
        let facts = FileFacts {
            identity: "inode:1".into(),
            prefix: b"prefix".to_vec(),
            length: 10000,
            cursor: 6000,
            positions,
        };
        let operation = fixture.operation(false);
        assert_eq!(
            fixture.receive_file(operation, entries.clone(), false, None, Some(facts.clone())),
            Ok(true)
        );
        let expected = fixture.messages();
        let operation = fixture.operation(false);
        assert_eq!(
            fixture.receive_file(operation, entries, false, None, Some(facts)),
            Ok(true)
        );
        assert_eq!(fixture.messages(), expected);
        let operation = fixture.operation(false);
        assert_eq!(
            fixture.receive_file(
                operation,
                vec![(None, Message::user("message 0"))],
                true,
                None,
                Some(FileFacts {
                    identity: "inode:1".into(),
                    prefix: b"prefix appended".to_vec(),
                    length: 11000,
                    cursor: 10010,
                    positions: vec![(10000, 0)],
                })
            ),
            Ok(true)
        );
        let mut expected = expected;
        expected.push(Message::user("message 0"));
        assert_eq!(fixture.messages(), expected);
        let operation = fixture.operation(false);
        assert_eq!(
            fixture.receive_file(
                operation,
                vec![(None, Message::user("message 0"))],
                false,
                None,
                Some(FileFacts {
                    identity: "inode:1".into(),
                    prefix: b"prefix appended".to_vec(),
                    length: 11000,
                    cursor: 10,
                    positions: vec![(0, 0)],
                })
            ),
            Ok(true)
        );
        assert_eq!(fixture.messages(), expected);
    }

    #[test]
    fn file_generations_and_decoder_subrecords_have_separate_aliases() {
        let mut fixture = Fixture::new(Vec::new());
        for (identity, prefix, length) in [
            ("first", "prefix", 100),
            ("second", "prefix", 100),
            ("second", "prefix", 20),
            ("second", "edited", 20),
        ] {
            let operation = fixture.operation(false);
            let facts = FileFacts {
                identity: identity.into(),
                prefix: prefix.as_bytes().to_vec(),
                length,
                cursor: 20,
                positions: vec![(10, 0), (10, 1)],
            };
            let entries = vec![(None, Message::user("same")), (None, Message::user("same"))];
            assert_eq!(
                fixture.receive_file(operation, entries.clone(), true, None, Some(facts.clone())),
                Ok(true)
            );
            let count = fixture.messages().len();
            let operation = fixture.operation(false);
            assert_eq!(
                fixture.receive_file(operation, entries, true, None, Some(facts)),
                Ok(true)
            );
            assert_eq!(fixture.messages().len(), count);
        }
        assert_eq!(fixture.messages(), vec![Message::user("same"); 8]);
        let operation = fixture.operation(false);
        assert_eq!(
            fixture.receive(
                operation,
                vec![(Some("file".into()), Message::user("external"))],
                true,
                None
            ),
            Ok(true)
        );
        assert_eq!(fixture.messages().len(), 9);
        let operation = fixture.operation(false);
        assert_eq!(
            fixture.receive(
                operation,
                vec![(Some("file".into()), Message::user("changed"))],
                true,
                None
            ),
            Ok(true)
        );
        let mut expected = vec![Message::user("same"); 8];
        expected.push(Message::user("changed"));
        assert_eq!(fixture.messages(), expected);
    }

    #[test]
    fn mutable_provider_entries_update_the_canonical_event_without_changing_identity() {
        let mut fixture = Fixture::new(Vec::new());
        let event = fixture.external("native", Message::user("first"));
        let identity = fixture
            .app
            .world()
            .get::<EventIdentity>(event)
            .unwrap()
            .clone();
        assert_eq!(fixture.external("native", Message::user("updated")), event);
        assert_eq!(
            fixture.app.world().get::<EventIdentity>(event).unwrap(),
            &identity
        );
        assert_eq!(fixture.messages(), [Message::user("updated")]);
        let changed = fixture
            .app
            .world()
            .entity(event)
            .get_ref::<MessageContent>()
            .unwrap()
            .last_changed();
        assert_eq!(fixture.external("native", Message::user("updated")), event);
        assert_eq!(
            fixture
                .app
                .world()
                .entity(event)
                .get_ref::<MessageContent>()
                .unwrap()
                .last_changed(),
            changed
        );
    }

    #[test]
    fn accepted_source_identity_survives_adoption_until_metadata_commit() {
        let mut fixture = Fixture::new(Vec::new());
        let operation = fixture.operation(false);
        assert_eq!(
            fixture.receive(
                operation,
                vec![(Some("native".into()), Message::user("accepted"))],
                false,
                None
            ),
            Ok(true)
        );
        assert!(fixture.app.world().get::<Read>(operation).is_some());
        assert!(fixture.app.world().get::<Applied>(operation).is_some());
        fixture.app.world_mut().write_message(Received {
            operation,
            entries: vec![(Some("native".into()), Message::user("duplicate"))],
            tail: false,
            error: None,
            updated_at: 42,
            file: None,
        });
        fixture.app.update();
        assert_eq!(fixture.messages(), [Message::user("accepted")]);
        assert!(
            fixture
                .app
                .world()
                .resource::<Messages<Committed>>()
                .is_empty()
        );
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.session)
            .insert(ProviderSessionId("replacement".into()));
        let session = fixture.session;
        let matched = fixture
            .app
            .world_mut()
            .run_system_once(move |versions: Versions, reads: Query<&Read>| {
                versions.matches(session, reads.get(operation).unwrap())
            })
            .unwrap();
        assert!(!matched);
    }

    #[test]
    fn provider_tail_reconciles_grouping_prompt_metadata_and_equivalent_tool_arguments() {
        let prompt =
            vmux_api::protocol::AgentPromptEnvelope::compose("first", Some("private context"));
        let tool = AssistantBlock::ToolUse {
            call_id: "call".into(),
            name: "Bash".into(),
            args: "{\"a\":1,\"b\":2}".into(),
            kind: Some(vmux_api::session::ToolKind::Execute),
            parent_call_id: None,
        };
        let mut fixture = Fixture::new(vec![
            Message::user(prompt),
            Message::Assistant {
                blocks: vec![AssistantBlock::Text("context".into()), tool],
            },
        ]);
        let operation = fixture.operation(false);
        assert_eq!(
            fixture.receive(
                operation,
                vec![
                    (None, Message::user("first")),
                    (
                        None,
                        Message::Assistant {
                            blocks: vec![AssistantBlock::Text("context".into())]
                        }
                    ),
                    (
                        None,
                        Message::Assistant {
                            blocks: vec![AssistantBlock::ToolUse {
                                call_id: "call".into(),
                                name: "Execute command".into(),
                                args: "{ \"b\":2, \"a\":1 }".into(),
                                kind: None,
                                parent_call_id: None
                            }]
                        }
                    ),
                    (None, Message::user("new")),
                ],
                true,
                None
            ),
            Ok(true)
        );
        let messages = fixture.messages();
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[2], Message::user("new"));
    }

    #[test]
    fn local_input_or_mutable_event_changes_reject_a_late_provider_read() {
        for mutable in [false, true] {
            let mut fixture = Fixture::new(vec![Message::user("cached")]);
            let operation = fixture.operation(false);
            if mutable {
                let event = fixture
                    .app
                    .world_mut()
                    .query_filtered::<Entity, With<ConversationEvent>>()
                    .single(fixture.app.world())
                    .unwrap();
                fixture
                    .app
                    .world_mut()
                    .entity_mut(event)
                    .insert(MessageContent(Message::user("changed")));
            } else {
                fixture
                    .app
                    .world_mut()
                    .write_message(ConversationAppendReceived {
                        session: SessionId::new("canonical"),
                        messages: vec![Message::user("local")],
                    });
            }
            assert_eq!(
                fixture.receive(
                    operation,
                    vec![(None, Message::user("obsolete"))],
                    true,
                    None
                ),
                Ok(false)
            );
            assert!(!fixture.messages().contains(&Message::user("obsolete")));
        }
    }

    #[test]
    fn source_replacement_operation_replacement_and_successful_retry_refuse_old_history() {
        for change in 0..3 {
            let mut fixture = Fixture::new(vec![Message::user("cached")]);
            let operation = fixture.operation(true);
            match change {
                0 => {
                    fixture
                        .app
                        .world_mut()
                        .entity_mut(fixture.session)
                        .insert(ProviderSessionId("replacement".into()));
                }
                1 => {
                    fixture.operation(false);
                }
                _ => {
                    fixture
                        .app
                        .world_mut()
                        .entity_mut(fixture.session)
                        .insert(RunState::Idle);
                }
            }
            assert_eq!(
                fixture.receive(
                    operation,
                    vec![(None, Message::user("obsolete"))],
                    true,
                    None
                ),
                Ok(false)
            );
            assert_eq!(fixture.messages(), [Message::user("cached")]);
        }
    }

    #[test]
    fn empty_and_failed_provider_results_preserve_cached_history() {
        for error in [None::<String>, Some("provider unavailable".into())] {
            let mut fixture = Fixture::new(vec![Message::user("cached")]);
            let operation = fixture.operation(false);
            let expected = if let Some(error) = &error {
                Err(error.clone())
            } else {
                Ok(true)
            };
            assert_eq!(
                fixture.receive(operation, Vec::new(), true, error),
                expected
            );
            assert_eq!(fixture.messages(), [Message::user("cached")]);
        }
    }

    #[test]
    fn replay_replaces_only_compatible_snapshot_events_without_duplicating_provider_history() {
        let mut fixture = Fixture::new(Vec::new());
        let operation = fixture.operation(false);
        let messages = vec![
            Message::user("history"),
            Message::Assistant {
                blocks: vec![AssistantBlock::Text("answer".into())],
            },
        ];
        assert_eq!(
            fixture.receive(
                operation,
                messages
                    .iter()
                    .cloned()
                    .map(|message| (None, message))
                    .collect(),
                true,
                None
            ),
            Ok(true)
        );
        assert_eq!(fixture.messages(), messages);
        fixture.app.world_mut().write_message(SnapshotReceived {
            session: SessionId::new("canonical"),
            messages: messages.clone(),
        });
        fixture.app.update();
        assert_eq!(fixture.messages(), messages);
        assert_eq!(
            fixture
                .app
                .world_mut()
                .query_filtered::<Entity, With<ConversationEvent>>()
                .iter(fixture.app.world())
                .count(),
            2
        );
    }

    #[test]
    fn native_history_and_protocol_replay_keep_one_event_identity_in_both_orders() {
        let messages = vec![
            Message::user("history"),
            Message::Assistant {
                blocks: vec![AssistantBlock::Text("answer".into())],
            },
        ];
        for native_first in [true, false] {
            let mut fixture = Fixture::new(if native_first {
                Vec::new()
            } else {
                messages.clone()
            });
            let before = if native_first {
                Vec::new()
            } else {
                fixture
                    .app
                    .world_mut()
                    .query_filtered::<(Entity, &EventIdentity), With<ConversationEvent>>()
                    .iter(fixture.app.world())
                    .map(|(entity, identity)| (entity, identity.event_id.clone()))
                    .collect::<Vec<_>>()
            };
            let operation = fixture.operation(false);
            assert_eq!(
                fixture.receive(
                    operation,
                    vec![
                        (Some("native-user".into()), messages[0].clone()),
                        (Some("native-answer".into()), messages[1].clone())
                    ],
                    false,
                    None
                ),
                Ok(true)
            );
            let before = if native_first {
                fixture
                    .app
                    .world_mut()
                    .query_filtered::<(Entity, &EventIdentity), With<ConversationEvent>>()
                    .iter(fixture.app.world())
                    .map(|(entity, identity)| (entity, identity.event_id.clone()))
                    .collect::<Vec<_>>()
            } else {
                before
            };
            fixture.app.world_mut().write_message(SnapshotReceived {
                session: SessionId::new("canonical"),
                messages: messages.clone(),
            });
            fixture.app.update();
            assert_eq!(fixture.messages(), messages);
            assert_eq!(
                fixture
                    .app
                    .world_mut()
                    .query_filtered::<Entity, With<ConversationEvent>>()
                    .iter(fixture.app.world())
                    .count(),
                2
            );
            for (entity, event_id) in before {
                assert_eq!(
                    fixture
                        .app
                        .world()
                        .get::<EventIdentity>(entity)
                        .unwrap()
                        .event_id,
                    event_id
                );
            }
        }
    }

    #[test]
    fn grouped_protocol_replay_and_native_fragments_keep_the_first_event_shapes() {
        let user = Message::user("question");
        let first = Message::Assistant {
            blocks: vec![AssistantBlock::Text("first".into())],
        };
        let second = Message::Assistant {
            blocks: vec![AssistantBlock::Text("second".into())],
        };
        let grouped = Message::Assistant {
            blocks: vec![
                AssistantBlock::Text("first".into()),
                AssistantBlock::Text("second".into()),
            ],
        };
        for native_first in [false, true] {
            let expected = if native_first {
                vec![user.clone(), first.clone(), second.clone()]
            } else {
                vec![user.clone(), grouped.clone()]
            };
            let mut fixture = Fixture::new(if native_first {
                Vec::new()
            } else {
                expected.clone()
            });
            let operation = fixture.operation(false);
            assert_eq!(
                fixture.receive(
                    operation,
                    vec![
                        (Some("user".into()), user.clone()),
                        (Some("first".into()), first.clone()),
                        (Some("second".into()), second.clone())
                    ],
                    false,
                    None
                ),
                Ok(true)
            );
            if native_first {
                fixture.app.world_mut().write_message(SnapshotReceived {
                    session: SessionId::new("canonical"),
                    messages: vec![user.clone(), grouped.clone()],
                });
                fixture.app.update();
            }
            assert_eq!(fixture.messages(), expected);
        }
    }

    #[test]
    fn aliased_assistant_growth_shrink_and_reordering_preserve_the_event_identity() {
        let first = Message::Assistant {
            blocks: vec![AssistantBlock::Thinking("thought".into())],
        };
        let expanded = Message::Assistant {
            blocks: vec![
                AssistantBlock::Thinking("thought".into()),
                AssistantBlock::Text("answer".into()),
            ],
        };
        let reordered = Message::Assistant {
            blocks: vec![
                AssistantBlock::Text("answer".into()),
                AssistantBlock::Thinking("thought".into()),
            ],
        };
        for native in [false, true] {
            let mut fixture = Fixture::new(if native {
                Vec::new()
            } else {
                vec![first.clone()]
            });
            let event = if native {
                fixture.external("whole", first.clone())
            } else {
                fixture
                    .app
                    .world_mut()
                    .query_filtered::<Entity, With<ConversationEvent>>()
                    .single(fixture.app.world())
                    .unwrap()
            };
            let identity = fixture
                .app
                .world()
                .get::<EventIdentity>(event)
                .unwrap()
                .clone();
            let created = *fixture.app.world().get::<CreatedAt>(event).unwrap();
            for message in [expanded.clone(), reordered.clone(), first.clone()] {
                if native {
                    let operation = fixture.operation(false);
                    assert_eq!(
                        fixture.receive(
                            operation,
                            vec![(Some("whole".into()), message.clone())],
                            false,
                            None
                        ),
                        Ok(true)
                    );
                } else {
                    fixture.app.world_mut().write_message(SnapshotReceived {
                        session: SessionId::new("canonical"),
                        messages: vec![message.clone()],
                    });
                    fixture.app.update();
                }
                assert_eq!(fixture.messages(), [message]);
                assert_eq!(
                    fixture.app.world().get::<EventIdentity>(event),
                    Some(&identity)
                );
                assert_eq!(
                    fixture.app.world().get::<CreatedAt>(event).unwrap().0,
                    created.0
                );
            }
        }
    }

    #[test]
    fn replay_reactivates_a_removed_native_block_under_the_same_event() {
        let full = Message::Assistant {
            blocks: vec![
                AssistantBlock::Thinking("thought".into()),
                AssistantBlock::Text("answer".into()),
            ],
        };
        let shortened = Message::Assistant {
            blocks: vec![AssistantBlock::Thinking("thought".into())],
        };
        let mut fixture = Fixture::new(vec![full.clone()]);
        let event = fixture.external("whole", full.clone());
        let identity = fixture
            .app
            .world()
            .get::<EventIdentity>(event)
            .unwrap()
            .clone();
        let operation = fixture.operation(false);
        assert_eq!(
            fixture.receive(
                operation,
                vec![(Some("whole".into()), shortened)],
                false,
                None
            ),
            Ok(true)
        );
        fixture.app.world_mut().write_message(SnapshotReceived {
            session: SessionId::new("canonical"),
            messages: vec![full.clone()],
        });
        fixture.app.update();
        assert_eq!(fixture.messages(), [full]);
        assert_eq!(
            fixture.app.world().get::<EventIdentity>(event),
            Some(&identity)
        );
    }

    #[test]
    fn replay_growth_keeps_existing_native_fragments_and_adopts_the_new_part() {
        let first = Message::Assistant {
            blocks: vec![AssistantBlock::Thinking("thought".into())],
        };
        let second = Message::Assistant {
            blocks: vec![AssistantBlock::Text("answer".into())],
        };
        let third = Message::Assistant {
            blocks: vec![AssistantBlock::Text("later".into())],
        };
        let mut fixture = Fixture::new(Vec::new());
        let first_entity = fixture.external("first", first.clone());
        let second_entity = fixture.external("second", second.clone());
        for extra in [false, true] {
            let mut blocks = vec![
                AssistantBlock::Thinking("thought".into()),
                AssistantBlock::Text("answer".into()),
            ];
            if extra {
                blocks.push(AssistantBlock::Text("later".into()));
            }
            fixture.app.world_mut().write_message(SnapshotReceived {
                session: SessionId::new("canonical"),
                messages: vec![Message::Assistant { blocks }],
            });
            fixture.app.update();
        }
        assert_eq!(fixture.messages(), [first.clone(), second.clone(), third]);
        assert_eq!(
            fixture
                .app
                .world()
                .get::<MessageContent>(first_entity)
                .unwrap()
                .0,
            first
        );
        assert_eq!(
            fixture
                .app
                .world()
                .get::<MessageContent>(second_entity)
                .unwrap()
                .0,
            second
        );
    }

    #[test]
    fn native_single_event_shrink_preserves_an_authoritative_operation() {
        let a = AssistantBlock::Text("a".into());
        let b = AssistantBlock::Text("b".into());
        let message = Message::Assistant {
            blocks: vec![a, b.clone()],
        };
        let mut fixture = Fixture::new(vec![message.clone()]);
        let operation = fixture.operation(false);
        assert_eq!(
            fixture.receive(
                operation,
                vec![(Some("group".into()), message.clone())],
                false,
                None
            ),
            Ok(true)
        );
        let identity = fixture
            .app
            .world_mut()
            .query::<&EventIdentity>()
            .single(fixture.app.world())
            .unwrap()
            .clone();
        fixture
            .app
            .world_mut()
            .write_message(crate::OperationCommitted(
                vmux_api::session::ConversationEvent {
                    event_id: identity.event_id,
                    session_id: SessionId::new("canonical"),
                    actor_id: identity.actor_id,
                    client_op_id: identity.client_op_id,
                    server_seq: identity.sequence,
                    created_at_ms: 42,
                    reply_to: identity.reply_to,
                    message: message.clone(),
                },
            ));
        fixture.app.update();
        let operation = fixture.operation(false);
        assert_eq!(
            fixture.receive(
                operation,
                vec![(Some("group".into()), Message::Assistant { blocks: vec![b] })],
                false,
                None
            ),
            Ok(true)
        );
        assert_eq!(fixture.messages(), [message]);
    }

    #[test]
    fn native_group_partial_shrink_preserves_original_multi_block_event_identity() {
        for protected in [false, true] {
            let a = AssistantBlock::Text("a".into());
            let b = AssistantBlock::Text("b".into());
            let c = AssistantBlock::Text("c".into());
            let d = AssistantBlock::Text("d".into());
            let mut fixture = Fixture::new(vec![
                Message::Assistant {
                    blocks: vec![a.clone(), b.clone()],
                },
                Message::Assistant {
                    blocks: vec![c.clone(), d.clone()],
                },
            ]);
            let before: Vec<_> = fixture
                .app
                .world_mut()
                .query::<&EventIdentity>()
                .iter(fixture.app.world())
                .cloned()
                .collect();
            let operation = fixture.operation(false);
            assert_eq!(
                fixture.receive(
                    operation,
                    vec![(
                        Some("group".into()),
                        Message::Assistant {
                            blocks: vec![a.clone(), b.clone(), c.clone(), d.clone()]
                        }
                    )],
                    false,
                    None
                ),
                Ok(true)
            );
            if protected {
                let identity = fixture
                    .app
                    .world_mut()
                    .query::<(&EventIdentity, &MessageContent)>()
                    .iter(fixture.app.world())
                    .find(|(_, content)| {
                        content.0
                            == Message::Assistant {
                                blocks: vec![c.clone(), d.clone()],
                            }
                    })
                    .unwrap()
                    .0
                    .clone();
                fixture
                    .app
                    .world_mut()
                    .write_message(crate::OperationCommitted(
                        vmux_api::session::ConversationEvent {
                            event_id: identity.event_id,
                            session_id: SessionId::new("canonical"),
                            actor_id: identity.actor_id,
                            client_op_id: identity.client_op_id,
                            server_seq: identity.sequence,
                            created_at_ms: 42,
                            reply_to: identity.reply_to,
                            message: Message::Assistant {
                                blocks: vec![c.clone(), d.clone()],
                            },
                        },
                    ));
                fixture.app.update();
            }
            for blocks in [
                vec![a.clone(), b.clone(), c.clone()],
                vec![a.clone(), b.clone(), c.clone(), d.clone()],
            ] {
                let operation = fixture.operation(false);
                let count = blocks.len();
                assert_eq!(
                    fixture.receive(
                        operation,
                        vec![(Some("group".into()), Message::Assistant { blocks })],
                        false,
                        None
                    ),
                    Ok(true)
                );
                let tail = if count == 3 && !protected {
                    vec![c.clone()]
                } else {
                    vec![c.clone(), d.clone()]
                };
                assert_eq!(
                    fixture.messages(),
                    vec![
                        Message::Assistant {
                            blocks: vec![a.clone(), b.clone()]
                        },
                        Message::Assistant { blocks: tail }
                    ]
                );
            }
            let after: Vec<_> = fixture
                .app
                .world_mut()
                .query::<&EventIdentity>()
                .iter(fixture.app.world())
                .cloned()
                .collect();
            assert_eq!(after, before);
        }
    }

    #[test]
    fn native_group_shrink_and_growth_reactivate_the_original_split_events() {
        let a = Message::Assistant {
            blocks: vec![AssistantBlock::Text("a".into())],
        };
        let b = Message::Assistant {
            blocks: vec![AssistantBlock::Text("b".into())],
        };
        let mut fixture = Fixture::new(vec![a.clone(), b.clone()]);
        let before: Vec<_> = fixture
            .app
            .world_mut()
            .query::<(Entity, &EventIdentity)>()
            .iter(fixture.app.world())
            .map(|(entity, id)| (entity, id.clone()))
            .collect();
        for blocks in [
            vec![
                AssistantBlock::Text("a".into()),
                AssistantBlock::Text("b".into()),
            ],
            vec![AssistantBlock::Text("a".into())],
            vec![
                AssistantBlock::Text("a".into()),
                AssistantBlock::Text("b".into()),
            ],
        ] {
            let operation = fixture.operation(false);
            assert_eq!(
                fixture.receive(
                    operation,
                    vec![(
                        Some("group".into()),
                        Message::Assistant {
                            blocks: blocks.clone()
                        }
                    )],
                    false,
                    None
                ),
                Ok(true)
            );
            let expected = if blocks.len() == 1 {
                vec![a.clone()]
            } else {
                vec![a.clone(), b.clone()]
            };
            assert_eq!(fixture.messages(), expected);
        }
        for (entity, id) in before {
            assert_eq!(fixture.app.world().get::<EventIdentity>(entity), Some(&id));
        }
    }

    #[test]
    fn native_group_shrink_preserves_an_authoritative_committed_operation() {
        let first = Message::Assistant {
            blocks: vec![AssistantBlock::Text("first".into())],
        };
        let second = Message::Assistant {
            blocks: vec![AssistantBlock::Text("second".into())],
        };
        let mut fixture = Fixture::new(vec![first.clone(), second.clone()]);
        let operation = fixture.operation(false);
        assert_eq!(
            fixture.receive(
                operation,
                vec![(
                    Some("group".into()),
                    Message::Assistant {
                        blocks: vec![
                            AssistantBlock::Text("first".into()),
                            AssistantBlock::Text("second".into())
                        ]
                    }
                )],
                false,
                None
            ),
            Ok(true)
        );
        let identity = fixture
            .app
            .world_mut()
            .query::<(&EventIdentity, &MessageContent)>()
            .iter(fixture.app.world())
            .find(|(_, content)| content.0 == second)
            .unwrap()
            .0
            .clone();
        fixture
            .app
            .world_mut()
            .write_message(crate::OperationCommitted(
                vmux_api::session::ConversationEvent {
                    event_id: identity.event_id.clone(),
                    session_id: SessionId::new("canonical"),
                    actor_id: identity.actor_id.clone(),
                    client_op_id: identity.client_op_id.clone(),
                    server_seq: identity.sequence,
                    created_at_ms: 42,
                    reply_to: identity.reply_to.clone(),
                    message: second.clone(),
                },
            ));
        fixture.app.update();
        let operation = fixture.operation(false);
        assert_eq!(
            fixture.receive(
                operation,
                vec![(Some("group".into()), first.clone())],
                false,
                None
            ),
            Ok(true)
        );
        assert_eq!(fixture.messages(), [first, second]);
    }

    #[test]
    fn duplicate_equal_blocks_keep_distinct_native_alias_targets_after_growth() {
        let mut fixture = Fixture::new(vec![Message::Assistant {
            blocks: vec![
                AssistantBlock::Text("same".into()),
                AssistantBlock::Text("same".into()),
            ],
        }]);
        let operation = fixture.operation(false);
        let unit = Message::Assistant {
            blocks: vec![AssistantBlock::Text("same".into())],
        };
        assert_eq!(
            fixture.receive(
                operation,
                vec![(Some("a".into()), unit.clone()), (Some("b".into()), unit)],
                false,
                None
            ),
            Ok(true)
        );
        fixture.app.world_mut().write_message(SnapshotReceived {
            session: SessionId::new("canonical"),
            messages: vec![Message::Assistant {
                blocks: vec![
                    AssistantBlock::Text("same".into()),
                    AssistantBlock::Text("same".into()),
                    AssistantBlock::Text("new".into()),
                ],
            }],
        });
        fixture.app.update();
        let operation = fixture.operation(false);
        assert_eq!(
            fixture.receive(
                operation,
                vec![(
                    Some("b".into()),
                    Message::Assistant {
                        blocks: vec![AssistantBlock::Text("changed".into())]
                    }
                )],
                false,
                None
            ),
            Ok(true)
        );
        assert_eq!(
            fixture.messages(),
            [Message::Assistant {
                blocks: vec![
                    AssistantBlock::Text("same".into()),
                    AssistantBlock::Text("changed".into()),
                    AssistantBlock::Text("new".into())
                ]
            }]
        );
    }

    #[test]
    fn consecutive_snapshots_can_trim_then_restore_the_latest_content() {
        let user = Message::user("question");
        let first = Message::Assistant {
            blocks: vec![AssistantBlock::Text("before".into())],
        };
        let last = Message::Assistant {
            blocks: vec![AssistantBlock::Text("after".into())],
        };
        let mut fixture = Fixture::new(vec![user.clone(), first]);
        fixture.app.world_mut().write_message(SnapshotReceived {
            session: SessionId::new("canonical"),
            messages: vec![user.clone()],
        });
        fixture.app.world_mut().write_message(SnapshotReceived {
            session: SessionId::new("canonical"),
            messages: vec![user.clone(), last.clone()],
        });
        fixture.app.update();
        assert_eq!(fixture.messages(), [user, last]);
    }

    #[test]
    fn source_replacement_replay_cannot_overwrite_an_old_native_event() {
        let mut fixture = Fixture::new(Vec::new());
        let old = fixture.external("record", Message::user("source A"));
        let identity = fixture
            .app
            .world()
            .get::<EventIdentity>(old)
            .unwrap()
            .clone();
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.session)
            .insert(ProviderSessionId("source-B".into()));
        fixture.app.world_mut().write_message(SnapshotReceived {
            session: SessionId::new("canonical"),
            messages: vec![Message::user("source B")],
        });
        fixture.app.update();
        assert_eq!(
            fixture.app.world().get::<EventIdentity>(old),
            Some(&identity)
        );
        assert_eq!(
            fixture.app.world().get::<MessageContent>(old).unwrap().0,
            Message::user("source A")
        );
        assert_eq!(
            fixture.messages(),
            [Message::user("source A"), Message::user("source B")]
        );
    }

    #[test]
    fn exhausted_sequence_refuses_the_whole_read_before_any_existing_entry_changes() {
        let mut fixture = Fixture::new(Vec::new());
        let event = fixture.external("native", Message::user("original"));
        let mut identity = fixture
            .app
            .world()
            .get::<EventIdentity>(event)
            .unwrap()
            .clone();
        identity.sequence = u64::MAX;
        fixture.app.world_mut().entity_mut(event).insert(identity);
        let operation = fixture.operation(false);
        assert_eq!(
            fixture.receive(
                operation,
                vec![
                    (Some("native".into()), Message::user("changed")),
                    (None, Message::user("new")),
                ],
                false,
                None
            ),
            Err("history sequence exhausted".into())
        );
        assert_eq!(fixture.messages(), [Message::user("original")]);
    }

    #[test]
    fn removing_the_owner_cannot_materialize_events_from_a_late_completion() {
        let mut fixture = Fixture::new(Vec::new());
        let operation = fixture.operation(false);
        fixture.app.world_mut().despawn(fixture.session);
        assert_eq!(
            fixture.receive(
                operation,
                vec![(None, Message::user("obsolete"))],
                true,
                None
            ),
            Ok(false)
        );
        assert_eq!(
            fixture
                .app
                .world_mut()
                .query_filtered::<Entity, With<ConversationEvent>>()
                .iter(fixture.app.world())
                .count(),
            0
        );
    }
}
