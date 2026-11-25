use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use bevy_app::{App, Plugin as BevyPlugin, Startup, Update};
use bevy_ecs::entity::Entity;
use bevy_ecs::message::MessageReader;
use bevy_ecs::query::With;
use bevy_ecs::system::{Commands, Query};
use bevy_window::AppLifecycle;
use vmux_api::command_bar::{CommandBarUiState, SwitchTabRequest};
use vmux_api::session::{
    RemoteAgent, RemoteApproval, RemoteEvent, RemoteMediaEntry, RemoteModelState, RemoteSession,
    RemoteStatus, SessionId,
};
use vmux_api::{BinEvent, UiState, UiStateFrame};
use vmux_ecs::time::Clock;
use vmux_ecs::{OperationRetained, OperationState};
use vmux_session::event::{ChatDraftChanged, ChatSubmit};
use vmux_session::host::{
    Agents, Attach, Attachments, Browsed, ChatRuntime, Conversation, LiveTurn, Log, Models,
    PublishComposerEffect, RemoveAttachment, Reported, RepublishChatUiState, Submitted,
};
use vmux_session::state::ChatUiState;
use vmux_transport::{ClientCredential, DeviceId};
use vmux_ui::hooks::transport::HostBinding;

use crate::MobileDomainPlugin;
use crate::actions::MountInterestAdded;
use crate::connection::{
    ActiveScan, AuthStage, CancelScan, CatalogRefresh, ConnectionNotice, ConnectionReachable,
    CredentialPersistence, DisconnectRequest, PairRequest, ScanCompleted, ScanRequest, SourceEpoch,
};
use crate::credentials::Credentials;
use crate::host::{MobileHost, MountLease};
use crate::io::{
    CloseConnectionJob, CloseConnectionReceipt, ConnectionHandle, CreateChatJob, CreateChatReceipt,
    LoadCredentialsJob, LoadCredentialsReceipt, OpenConnectionJob, OpenConnectionReceipt,
    OpenStreamJob, OpenStreamReceipt, PersistCredentialsJob, PersistCredentialsReceipt,
    ReadAgentsJob, ReadAgentsReceipt, ReadMediaJob, ReadMediaReceipt, ReadModelsJob,
    ReadModelsReceipt, ReadSessionsJob, ReadSessionsReceipt, ReadStreamEventJob,
    ReadStreamEventReceipt, ReadUpgradedCredentialJob, ReadUpgradedCredentialReceipt,
    RemoveCredentialsJob, ResetConnectionJob, ResetConnectionReceipt, SendPromptJob,
    SendPromptReceipt, StreamCancel, StreamHandle, StreamReadOutcome,
};
use crate::presentation::{
    MobileAuthStage, MobileConnectionUiState, MobileScreenRoute, MobileSessionUiState,
};
use crate::remote::ApiError;
use crate::runtime::RuntimeHandle;
use crate::session::{
    LeaveSession, OpenSession, RestartSession, SelectedSession, StartChatRequest, StreamConnected,
};
use crate::stream::{
    StreamReconnect, StreamReconnectDeadline, StreamSession, StreamTerminalRefusal,
};

struct MobileChatSinkPlugin;

impl BevyPlugin for MobileChatSinkPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Reported>()
            .add_message::<Submitted>()
            .add_message::<Agents>()
            .add_message::<Models>()
            .add_message::<Browsed>()
            .add_message::<Attach>()
            .add_message::<RemoveAttachment>()
            .add_message::<PublishComposerEffect>()
            .add_message::<RepublishChatUiState>()
            .add_systems(Startup, spawn_chat_sink)
            .add_systems(Update, fold_chat_sink);
    }
}

fn spawn_chat_sink(mut commands: Commands) {
    commands.spawn((
        ChatRuntime,
        Conversation::default(),
        Log::default(),
        LiveTurn::default(),
        Attachments::default(),
        Models::default(),
        Browsed::default(),
    ));
}

fn fold_chat_sink(
    mut reported: MessageReader<Reported>,
    mut models_reader: MessageReader<Models>,
    mut browsed_reader: MessageReader<Browsed>,
    mut runtimes: Query<(&mut Conversation, &mut Models, &mut Browsed), With<ChatRuntime>>,
) {
    let Ok((mut conversation, mut models, mut browsed)) = runtimes.single_mut() else {
        return;
    };
    for Reported(event) in reported.read() {
        match event {
            RemoteEvent::Session { session } => {
                conversation.status = session.status.clone();
                conversation.approval = session.approval.clone();
                conversation.session = Some(session.as_ref().clone());
            }
            RemoteEvent::Status { status } => {
                if !matches!(status, RemoteStatus::Streaming) {
                    conversation.approval = None;
                }
                conversation.status = status.clone();
            }
            RemoteEvent::Approval { approval } => {
                conversation.approval = approval.clone();
            }
            RemoteEvent::Snapshot { .. } | RemoteEvent::Delta { .. } => {}
        }
    }
    for update in models_reader.read() {
        *models = update.clone();
    }
    for update in browsed_reader.read() {
        *browsed = update.clone();
    }
}

struct MobileFixture {
    runtime: RuntimeHandle,
    clock: Clock,
}

impl MobileFixture {
    fn start() -> Self {
        vmux_ui::i18n::Locale::from("en-US").make_current();
        let clock = Clock::fake(1_000);
        let mut app = App::new();
        app.insert_resource(clock.clone()).add_plugins((
            vmux_app::VmuxMobilePlugin,
            MobileChatSinkPlugin,
            MobileDomainPlugin,
        ));
        let runtime = RuntimeHandle::from_app(app);
        runtime.report_lifecycle(AppLifecycle::Running);
        runtime.update();
        Self { runtime, clock }
    }

    fn advance(&self, duration: Duration) {
        self.clock.advance(duration);
    }

    fn mount(&self) -> (HostBinding, MountLease) {
        MobileHost::mount(self.runtime.clone())
    }

    fn tick(&self) {
        self.runtime.update();
    }

    fn complete_initial_load(&self, stored: Result<Option<Credentials>, String>) {
        let entity = self.pending_job::<LoadCredentialsJob>().unwrap();
        self.runtime.with_world_mut(|world| {
            world
                .entity_mut(entity)
                .remove::<LoadCredentialsJob>()
                .insert(LoadCredentialsReceipt(stored));
        });
        self.tick();
    }

    fn pair_and_connect(
        &self,
        handle_id: u64,
        sessions: Vec<RemoteSession>,
        agents: Vec<RemoteAgent>,
    ) {
        self.runtime.send(PairRequest(Self::valid_pair_link()));
        self.tick();
        let open_entity = self.pending_job::<OpenConnectionJob>().unwrap();
        self.runtime.with_world_mut(|world| {
            world
                .entity_mut(open_entity)
                .remove::<OpenConnectionJob>()
                .insert(OpenConnectionReceipt(Ok(ConnectionHandle::fake(handle_id))));
        });
        self.tick();
        let read_entity = self.pending_job::<ReadSessionsJob>().unwrap();
        self.runtime.with_world_mut(|world| {
            world
                .entity_mut(read_entity)
                .remove::<ReadSessionsJob>()
                .insert(ReadSessionsReceipt(Ok(sessions)));
        });
        self.tick();
        if let Some(agents_entity) = self.pending_job::<ReadAgentsJob>() {
            self.runtime.with_world_mut(|world| {
                world
                    .entity_mut(agents_entity)
                    .remove::<ReadAgentsJob>()
                    .insert(ReadAgentsReceipt(Ok(agents)));
            });
        }
        if let Some(upgrade_entity) = self.pending_job::<ReadUpgradedCredentialJob>() {
            self.runtime.with_world_mut(|world| {
                world
                    .entity_mut(upgrade_entity)
                    .remove::<ReadUpgradedCredentialJob>()
                    .insert(ReadUpgradedCredentialReceipt(None));
            });
        }
        self.tick();
    }

    fn pending_job<T: bevy_ecs::component::Component>(&self) -> Option<Entity> {
        self.runtime.with_world_mut(|world| {
            let mut query = world.query_filtered::<Entity, With<T>>();
            query.iter(world).next()
        })
    }

    fn pending_jobs<T: bevy_ecs::component::Component>(&self) -> Vec<Entity> {
        self.runtime.with_world_mut(|world| {
            let mut query = world.query_filtered::<Entity, With<T>>();
            query.iter(world).collect()
        })
    }

    fn valid_pair_link() -> String {
        "https://mac.example.ts.net/#relay_token=r1&pairing_token=p1&fp=abcdef&device=mac-1"
            .to_string()
    }

    fn sample_credentials() -> Credentials {
        Credentials {
            base_url: "https://mac.example.ts.net".to_string(),
            relay_token: "relay-1".to_string(),
            credential: Some(ClientCredential::Device("device-1".to_string())),
            client_id: DeviceId::new("phone-1"),
            fingerprint: "abcdef".to_string(),
            device: "mac-1".to_string(),
        }
    }

    fn session(id: &str, name: &str) -> RemoteSession {
        RemoteSession {
            id: SessionId::new(id),
            url: format!("vmux://sessions/{id}"),
            title: name.to_string(),
            name: name.to_string(),
            runtime: "claude".to_string(),
            model: None,
            cwd: format!("/src/{name}"),
            status: RemoteStatus::Idle,
            approval: None,
            created_at_ms: 1,
        }
    }

    fn decode_state<T>(bytes: &[u8]) -> T
    where
        T: UiState + rkyv::Archive,
        T::Archived: rkyv::Deserialize<T, rkyv::api::high::HighDeserializer<rkyv::rancor::Error>>
            + for<'a> rkyv::bytecheck::CheckBytes<
                rkyv::api::high::HighValidator<'a, rkyv::rancor::Error>,
            >,
        UiStateFrame<T, T::Update>: rkyv::Archive,
        <UiStateFrame<T, T::Update> as rkyv::Archive>::Archived:
            rkyv::Deserialize<
                    UiStateFrame<T, T::Update>,
                    rkyv::api::high::HighDeserializer<rkyv::rancor::Error>,
                > + for<'a> rkyv::bytecheck::CheckBytes<
                    rkyv::api::high::HighValidator<'a, rkyv::rancor::Error>,
                >,
    {
        let frame = rkyv::from_bytes::<UiStateFrame<T, T::Update>, rkyv::rancor::Error>(bytes)
            .expect("valid ui state frame");
        match frame {
            UiStateFrame::Snapshot(state) => state,
            UiStateFrame::Updates(_) => panic!("expected snapshot frame"),
        }
    }
}

#[test]
fn two_mounts_keep_independent_scopes_and_unmount_prunes_old_listeners() {
    let fixture = MobileFixture::start();
    fixture.complete_initial_load(Ok(None));

    let (host_a, lease_a) = fixture.mount();
    let (host_b, _lease_b) = fixture.mount();
    let seen_a = Rc::new(RefCell::new(Vec::<MobileConnectionUiState>::new()));
    let seen_b = Rc::new(RefCell::new(Vec::<MobileConnectionUiState>::new()));

    let sink_a = seen_a.clone();
    host_a
        .0
        .listen(
            MobileConnectionUiState::ID,
            Box::new(move |bytes| {
                sink_a.borrow_mut().push(MobileFixture::decode_state(bytes));
                true
            }),
        )
        .unwrap();
    let sink_b = seen_b.clone();
    host_b
        .0
        .listen(
            MobileConnectionUiState::ID,
            Box::new(move |bytes| {
                sink_b.borrow_mut().push(MobileFixture::decode_state(bytes));
                true
            }),
        )
        .unwrap();
    fixture.tick();

    assert_eq!(
        fixture.runtime.listener_count(MobileConnectionUiState::ID),
        2
    );
    assert_eq!(
        seen_a.borrow().last().unwrap().auth,
        MobileAuthStage::Unpaired
    );
    assert_eq!(
        seen_b.borrow().last().unwrap().auth,
        MobileAuthStage::Unpaired
    );

    drop(lease_a);
    assert_eq!(
        fixture.runtime.listener_count(MobileConnectionUiState::ID),
        1
    );

    let count_a_before = seen_a.borrow().len();
    fixture.pair_and_connect(1, vec![MobileFixture::session("s1", "alpha")], Vec::new());

    assert_eq!(seen_a.borrow().len(), count_a_before);
    assert_eq!(
        seen_b.borrow().last().unwrap().auth,
        MobileAuthStage::Paired
    );

    let (host_c, _lease_c) = fixture.mount();
    let seen_c = Rc::new(RefCell::new(Vec::<MobileConnectionUiState>::new()));
    let sink_c = seen_c.clone();
    host_c
        .0
        .listen(
            MobileConnectionUiState::ID,
            Box::new(move |bytes| {
                sink_c.borrow_mut().push(MobileFixture::decode_state(bytes));
                true
            }),
        )
        .unwrap();
    fixture.tick();
    assert_eq!(
        seen_c.borrow().last().unwrap().auth,
        MobileAuthStage::Paired
    );
}

#[test]
fn canonical_catalog_drives_launcher_order_and_switch_tab_resolves_displayed_identity() {
    let fixture = MobileFixture::start();
    fixture.complete_initial_load(Ok(None));
    let (host, _lease) = fixture.mount();

    let launcher_states = Rc::new(RefCell::new(Vec::<CommandBarUiState>::new()));
    let sink = launcher_states.clone();
    host.0
        .listen(
            CommandBarUiState::ID,
            Box::new(move |bytes| {
                sink.borrow_mut().push(MobileFixture::decode_state(bytes));
                true
            }),
        )
        .unwrap();

    let session_states = Rc::new(RefCell::new(Vec::<MobileSessionUiState>::new()));
    let session_sink = session_states.clone();
    host.0
        .listen(
            MobileSessionUiState::ID,
            Box::new(move |bytes| {
                session_sink
                    .borrow_mut()
                    .push(MobileFixture::decode_state(bytes));
                true
            }),
        )
        .unwrap();

    fixture.pair_and_connect(
        1,
        vec![
            MobileFixture::session("sid-alpha", "alpha"),
            MobileFixture::session("sid-beta", "beta"),
        ],
        vec![RemoteAgent {
            id: "claude".to_string(),
            name: "Claude".to_string(),
            url: "vmux://sessions/claude".to_string(),
            icon: String::new(),
        }],
    );

    let latest = launcher_states.borrow().last().unwrap().clone();
    let titles: Vec<&str> = latest
        .snapshot
        .tabs
        .iter()
        .map(|tab| tab.title.as_str())
        .collect();
    assert_eq!(titles, ["alpha", "beta"]);
    assert_eq!(latest.snapshot.pages.len(), 1);

    let bytes =
        rkyv::to_bytes::<rkyv::rancor::Error>(&SwitchTabRequest { pane: 0, index: 1 }).unwrap();
    host.0.send(SwitchTabRequest::ID, &bytes).unwrap();
    fixture.tick();

    let session_state = session_states.borrow().last().unwrap().clone();
    assert_eq!(session_state.route, MobileScreenRoute::Session);
    assert_eq!(session_state.selected_sid, "sid-beta");
}

#[test]
fn invalid_pair_request_after_valid_attempt_refuses_old_connection_and_drains_handle() {
    let fixture = MobileFixture::start();
    fixture.complete_initial_load(Ok(None));

    fixture
        .runtime
        .send(PairRequest(MobileFixture::valid_pair_link()));
    fixture.tick();
    let stale_open = fixture.pending_job::<OpenConnectionJob>().unwrap();

    fixture
        .runtime
        .send(PairRequest("not-a-valid-link".to_string()));
    fixture.tick();

    fixture.runtime.with_world_mut(|world| {
        world
            .entity_mut(stale_open)
            .remove::<OpenConnectionJob>()
            .insert(OpenConnectionReceipt(Ok(ConnectionHandle::fake(77))));
    });
    fixture.tick();

    let auth = fixture
        .runtime
        .with_world_mut(|world| *world.query::<&AuthStage>().single(world).unwrap());
    assert_eq!(auth, AuthStage::Unpaired);

    let close_entity = fixture.pending_job::<CloseConnectionJob>().unwrap();
    let closed_id = fixture.runtime.with_world(|world| {
        world
            .get::<CloseConnectionJob>(close_entity)
            .unwrap()
            .handle
            .id()
    });
    assert_eq!(closed_id, 77);
    fixture.runtime.with_world_mut(|world| {
        world
            .entity_mut(close_entity)
            .remove::<CloseConnectionJob>()
            .insert(CloseConnectionReceipt);
    });
    fixture.tick();
    assert!(
        fixture
            .runtime
            .with_world(|world| { world.get::<OperationRetained>(close_entity).is_none() })
    );
}

#[test]
fn restore_offline_retries_while_pair_error_stays_unpaired_and_unauthorized_clears_exact_source() {
    let fixture = MobileFixture::start();
    fixture.complete_initial_load(Ok(Some(MobileFixture::sample_credentials())));

    let open_entity = fixture.pending_job::<OpenConnectionJob>().unwrap();
    fixture.runtime.with_world_mut(|world| {
        world
            .entity_mut(open_entity)
            .remove::<OpenConnectionJob>()
            .insert(OpenConnectionReceipt(Ok(ConnectionHandle::fake(10))));
    });
    fixture.tick();

    let read_entity = fixture.pending_job::<ReadSessionsJob>().unwrap();
    fixture.runtime.with_world_mut(|world| {
        world
            .entity_mut(read_entity)
            .remove::<ReadSessionsJob>()
            .insert(ReadSessionsReceipt(Err(ApiError::Message(
                "offline".to_string(),
            ))));
    });
    fixture.tick();

    let (auth, reachable) = fixture.runtime.with_world_mut(|world| {
        let mut q = world.query::<(&AuthStage, &ConnectionReachable)>();
        let (auth, reachable) = q.single(world).unwrap();
        (*auth, reachable.0)
    });
    assert_eq!(auth, AuthStage::Paired);
    assert!(!reachable);

    fixture.advance(CatalogRefresh::INTERVAL);
    fixture.tick();

    let refresh_entity = fixture.pending_job::<ReadSessionsJob>().unwrap();
    fixture.runtime.with_world_mut(|world| {
        world
            .entity_mut(refresh_entity)
            .remove::<ReadSessionsJob>()
            .insert(ReadSessionsReceipt(Err(ApiError::Unauthorized)));
    });
    fixture.tick();
    fixture.tick();

    let auth_after = fixture
        .runtime
        .with_world_mut(|world| *world.query::<&AuthStage>().single(world).unwrap());
    assert_eq!(auth_after, AuthStage::Unpaired);
    let close_entity = fixture.pending_job::<CloseConnectionJob>().unwrap();
    assert_eq!(
        fixture.runtime.with_world(|world| world
            .get::<CloseConnectionJob>(close_entity)
            .unwrap()
            .handle
            .id()),
        10
    );
    assert!(fixture.pending_job::<RemoveCredentialsJob>().is_some());

    fixture
        .runtime
        .send(PairRequest(MobileFixture::valid_pair_link()));
    fixture.tick();
    let pair_open = fixture.pending_job::<OpenConnectionJob>().unwrap();
    fixture.runtime.with_world_mut(|world| {
        world
            .entity_mut(pair_open)
            .remove::<OpenConnectionJob>()
            .insert(OpenConnectionReceipt(Ok(ConnectionHandle::fake(11))));
    });
    fixture.tick();
    let pair_read = fixture.pending_job::<ReadSessionsJob>().unwrap();
    fixture.runtime.with_world_mut(|world| {
        world
            .entity_mut(pair_read)
            .remove::<ReadSessionsJob>()
            .insert(ReadSessionsReceipt(Err(ApiError::Message(
                "unreachable".to_string(),
            ))));
    });
    fixture.tick();
    let pair_auth = fixture
        .runtime
        .with_world_mut(|world| *world.query::<&AuthStage>().single(world).unwrap());
    assert_eq!(pair_auth, AuthStage::Unpaired);
}

#[test]
fn re_pair_during_start_chat_refuses_old_created_session_and_never_guesses_unknown_sid() {
    let fixture = MobileFixture::start();
    fixture.complete_initial_load(Ok(None));
    fixture.pair_and_connect(1, vec![MobileFixture::session("s1", "one")], Vec::new());

    fixture.runtime.send(StartChatRequest {
        text: "hello".to_string(),
        agent_url: None,
    });
    fixture.tick();
    let create_op = fixture.pending_job::<CreateChatJob>().unwrap();

    fixture.pair_and_connect(2, vec![MobileFixture::session("s2", "two")], Vec::new());

    fixture.runtime.with_world_mut(|world| {
        world
            .entity_mut(create_op)
            .remove::<CreateChatJob>()
            .insert(CreateChatReceipt(Ok(SessionId::new("s-old-created"))));
    });
    fixture.tick();

    let selected = fixture.runtime.with_world_mut(|world| {
        world
            .query::<&SelectedSession>()
            .iter(world)
            .next()
            .cloned()
    });
    assert!(selected.is_none());

    fixture.runtime.send(StartChatRequest {
        text: "fresh".to_string(),
        agent_url: None,
    });
    fixture.tick();
    let create_op2 = fixture.pending_job::<CreateChatJob>().unwrap();
    fixture.runtime.with_world_mut(|world| {
        world
            .entity_mut(create_op2)
            .remove::<CreateChatJob>()
            .insert(CreateChatReceipt(Ok(SessionId::new("s-exact"))));
    });
    fixture.tick();

    let poll_entity = fixture.pending_job::<ReadSessionsJob>().unwrap();
    fixture.runtime.with_world_mut(|world| {
        world
            .entity_mut(poll_entity)
            .remove::<ReadSessionsJob>()
            .insert(ReadSessionsReceipt(Ok(vec![
                MobileFixture::session("s2", "two"),
                MobileFixture::session("s-unrelated-new", "unrelated"),
            ])));
    });
    fixture.tick();

    let selected_after_wrong = fixture.runtime.with_world_mut(|world| {
        world
            .query::<&SelectedSession>()
            .iter(world)
            .next()
            .cloned()
    });
    assert!(
        selected_after_wrong.is_none(),
        "must never guess an unrelated newly appearing session ID"
    );
}

#[test]
fn session_switch_during_blocked_read_cancels_old_stream_and_rejects_late_results() {
    let fixture = MobileFixture::start();
    fixture.complete_initial_load(Ok(None));
    let (host, _lease) = fixture.mount();
    host.0.listen(ChatUiState::ID, Box::new(|_| true)).unwrap();

    fixture.pair_and_connect(
        1,
        vec![
            MobileFixture::session("sid-a", "A"),
            MobileFixture::session("sid-b", "B"),
        ],
        Vec::new(),
    );

    fixture.runtime.send(OpenSession(SessionId::new("sid-a")));
    fixture.tick();

    let open_a = fixture.pending_job::<OpenStreamJob>().unwrap();
    fixture.runtime.with_world_mut(|world| {
        world
            .entity_mut(open_a)
            .remove::<OpenStreamJob>()
            .insert(OpenStreamReceipt(Ok(StreamHandle::fake())));
    });
    fixture.tick();

    let read_a = fixture.pending_job::<ReadStreamEventJob>().unwrap();
    let models_a = fixture.pending_job::<ReadModelsJob>().unwrap();

    let draft_bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&ChatDraftChanged {
        text: "inspect @img".to_string(),
    })
    .unwrap();
    host.0.send(ChatDraftChanged::ID, &draft_bytes).unwrap();
    fixture.tick();
    let media_a = fixture.pending_job::<ReadMediaJob>().unwrap();

    let submit_bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&ChatSubmit {
        text: "hello".to_string(),
    })
    .unwrap();
    host.0.send(ChatSubmit::ID, &submit_bytes).unwrap();
    fixture.tick();
    let prompt_a = fixture.pending_job::<SendPromptJob>().unwrap();

    let cancel_a = fixture
        .runtime
        .with_world(|world| world.get::<StreamCancel>(read_a).unwrap().clone());

    fixture.runtime.send(OpenSession(SessionId::new("sid-b")));
    fixture.tick();

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap();
    let notified = rt.block_on(async {
        tokio::time::timeout(Duration::from_millis(50), cancel_a.wait())
            .await
            .is_ok()
    });
    assert!(notified, "switching sessions must trigger old StreamCancel");

    fixture.runtime.with_world_mut(|world| {
        world
            .entity_mut(read_a)
            .remove::<ReadStreamEventJob>()
            .insert(ReadStreamEventReceipt {
                outcome: StreamReadOutcome::Event(RemoteEvent::Approval {
                    approval: Some(RemoteApproval {
                        call_id: "stale-call".to_string(),
                        name: "shell".to_string(),
                        args: vmux_api::json::JsonValue::Null,
                    }),
                }),
                stream: Some(StreamHandle::fake()),
            });
        world.entity_mut(models_a).remove::<ReadModelsJob>().insert(
            ReadModelsReceipt::from_result(Ok(RemoteModelState {
                selected_id: "stale-model".to_string(),
                ..RemoteModelState::default()
            })),
        );
        world
            .entity_mut(media_a)
            .remove::<ReadMediaJob>()
            .insert(ReadMediaReceipt(Ok(vec![RemoteMediaEntry {
                path: "/stale.png".to_string(),
                name: "stale.png".to_string(),
                parent: "/".to_string(),
                mime_type: "image/png".to_string(),
                size: 10,
                is_dir: false,
                preview_data_url: String::new(),
            }])));
        world
            .entity_mut(prompt_a)
            .remove::<SendPromptJob>()
            .insert(SendPromptReceipt(Err(ApiError::Message(
                "stale failure".to_string(),
            ))));
    });
    fixture.tick();

    fixture.runtime.with_world_mut(|world| {
        let mut q = world.query_filtered::<(&Conversation, &Models, &Browsed), With<ChatRuntime>>();
        let (conversation, models, browsed) = q.single(world).unwrap();
        assert_eq!(conversation.session.as_ref().unwrap().id.as_str(), "sid-b");
        assert!(conversation.approval.is_none());
        assert_eq!(conversation.status, RemoteStatus::Idle);
        assert!(models.0.selected_id.is_empty());
        assert!(browsed.entries.is_empty());
        assert!(world.get::<OperationRetained>(read_a).is_none());
    });
}

#[test]
fn stream_reconnect_waits_two_second_ecs_deadline_and_refuses_terminal_errors() {
    let fixture = MobileFixture::start();
    fixture.complete_initial_load(Ok(None));
    fixture.pair_and_connect(1, vec![MobileFixture::session("sid-1", "one")], Vec::new());

    fixture.runtime.send(OpenSession(SessionId::new("sid-1")));
    fixture.tick();

    let open_1 = fixture.pending_job::<OpenStreamJob>().unwrap();
    fixture.runtime.with_world_mut(|world| {
        world
            .entity_mut(open_1)
            .remove::<OpenStreamJob>()
            .insert(OpenStreamReceipt(Ok(StreamHandle::fake())));
    });
    fixture.tick();

    let read_1 = fixture.pending_job::<ReadStreamEventJob>().unwrap();
    fixture.runtime.with_world_mut(|world| {
        world
            .entity_mut(read_1)
            .remove::<ReadStreamEventJob>()
            .insert(ReadStreamEventReceipt {
                outcome: StreamReadOutcome::Ended,
                stream: None,
            });
    });
    fixture.tick();

    assert!(fixture.pending_job::<OpenStreamJob>().is_none());
    let has_deadline = fixture.runtime.with_world_mut(|world| {
        world
            .query::<&StreamReconnectDeadline>()
            .iter(world)
            .count()
            == 1
    });
    assert!(has_deadline);

    fixture.advance(StreamReconnect::DELAY);
    fixture.tick();

    let open_2 = fixture.pending_job::<OpenStreamJob>().unwrap();
    fixture.runtime.with_world_mut(|world| {
        world
            .entity_mut(open_2)
            .remove::<OpenStreamJob>()
            .insert(OpenStreamReceipt(Err(ApiError::NotFound)));
    });
    fixture.tick();
    fixture.tick();

    let terminal = fixture
        .runtime
        .with_world_mut(|world| world.query::<&StreamTerminalRefusal>().iter(world).count() == 1);
    assert!(terminal);
    assert!(fixture.pending_job::<OpenStreamJob>().is_none());
}

#[test]
fn credential_persistence_failure_is_surfaced_without_claiming_persisted() {
    let fixture = MobileFixture::start();
    fixture.complete_initial_load(Ok(None));

    fixture
        .runtime
        .send(PairRequest(MobileFixture::valid_pair_link()));
    fixture.tick();
    let open = fixture.pending_job::<OpenConnectionJob>().unwrap();
    fixture.runtime.with_world_mut(|world| {
        world
            .entity_mut(open)
            .remove::<OpenConnectionJob>()
            .insert(OpenConnectionReceipt(Ok(ConnectionHandle::fake(30))));
    });
    fixture.tick();
    let read = fixture.pending_job::<ReadSessionsJob>().unwrap();
    fixture.runtime.with_world_mut(|world| {
        world
            .entity_mut(read)
            .remove::<ReadSessionsJob>()
            .insert(ReadSessionsReceipt(Ok(Vec::new())));
    });
    fixture.tick();

    let upgrade = fixture.pending_job::<ReadUpgradedCredentialJob>().unwrap();
    fixture.runtime.with_world_mut(|world| {
        world
            .entity_mut(upgrade)
            .remove::<ReadUpgradedCredentialJob>()
            .insert(ReadUpgradedCredentialReceipt(Some(
                MobileFixture::sample_credentials(),
            )));
    });
    fixture.tick();

    let persist = fixture.pending_job::<PersistCredentialsJob>().unwrap();
    fixture.runtime.with_world_mut(|world| {
        world
            .entity_mut(persist)
            .remove::<PersistCredentialsJob>()
            .insert(PersistCredentialsReceipt(Err("disk full".to_string())));
    });
    fixture.tick();

    let (status, notice, op_state) = fixture.runtime.with_world_mut(|world| {
        let mut q = world.query::<(&CredentialPersistence, &ConnectionNotice)>();
        let (status, notice) = q.single(world).unwrap();
        let op_state = world.get::<OperationState>(persist).cloned();
        (*status, notice.0.clone(), op_state)
    });
    assert_eq!(status, CredentialPersistence::Failed);
    assert_eq!(notice, "disk full");
    assert_eq!(op_state, Some(OperationState::Failed));
}

#[test]
fn suspend_owes_one_frame_stops_delivery_and_resume_restarts_without_duplicate_stream() {
    let fixture = MobileFixture::start();
    fixture.complete_initial_load(Ok(None));
    fixture.pair_and_connect(1, vec![MobileFixture::session("sid-1", "one")], Vec::new());

    fixture.runtime.send(OpenSession(SessionId::new("sid-1")));
    fixture.tick();
    let open_1 = fixture.pending_job::<OpenStreamJob>().unwrap();
    fixture.runtime.with_world_mut(|world| {
        world
            .entity_mut(open_1)
            .remove::<OpenStreamJob>()
            .insert(OpenStreamReceipt(Ok(StreamHandle::fake())));
    });
    fixture.tick();

    fixture.runtime.report_lifecycle(AppLifecycle::WillSuspend);
    fixture.tick();
    fixture.runtime.send(LeaveSession);
    fixture.tick();

    let still_selected = fixture
        .runtime
        .with_world_mut(|world| world.query::<&SelectedSession>().single(world).is_ok());
    assert!(
        still_selected,
        "suspended world must not process messages until resumed"
    );

    fixture.runtime.with_world_mut(|world| {
        let mut q = world.query::<&mut StreamConnected>();
        q.single_mut(world).unwrap().0 = true;
    });
    fixture.runtime.report_lifecycle(AppLifecycle::Running);
    fixture.runtime.send(RestartSession);
    fixture.tick();

    let reset_jobs = fixture.pending_jobs::<ResetConnectionJob>();
    if let Some(reset_entity) = reset_jobs.first().copied() {
        fixture.runtime.with_world_mut(|world| {
            world
                .entity_mut(reset_entity)
                .remove::<ResetConnectionJob>()
                .insert(ResetConnectionReceipt);
        });
        fixture.tick();
    }
    let stream_count = fixture
        .runtime
        .with_world_mut(|world| world.query::<&StreamSession>().iter(world).count());
    assert!(stream_count <= 1);
}

#[test]
fn stale_or_canceled_scanner_callback_refuses_old_pair_and_failure_results() {
    let fixture = MobileFixture::start();
    fixture.complete_initial_load(Ok(None));

    let stale_ticket = fixture.runtime.with_world_mut(|world| {
        let epoch = world.query::<&SourceEpoch>().single(world).unwrap().0;
        let ticket = crate::connection::ScanTicket {
            source_epoch: epoch,
            scan_generation: 9,
        };
        world
            .query::<&mut ActiveScan>()
            .single_mut(world)
            .unwrap()
            .0 = Some(ticket);
        ticket
    });

    fixture.runtime.send(CancelScan);
    fixture.tick();

    fixture.runtime.send(ScanCompleted {
        ticket: stale_ticket,
        result: Ok(MobileFixture::valid_pair_link()),
    });
    fixture.runtime.send(ScanCompleted {
        ticket: stale_ticket,
        result: Err("stale camera error".to_string()),
    });
    fixture.tick();

    assert!(fixture.pending_job::<OpenConnectionJob>().is_none());
    let notice = fixture.runtime.with_world_mut(|world| {
        world
            .query::<&ConnectionNotice>()
            .single(world)
            .unwrap()
            .0
            .clone()
    });
    assert!(notice.is_empty());

    fixture.runtime.send(DisconnectRequest);
    fixture.tick();
    fixture.runtime.send(ScanRequest);
    fixture.tick();
    let _ = MountInterestAdded {
        mount: crate::runtime::MountId(0),
        kind: crate::actions::MountInterestKind::Chat,
    };
}
