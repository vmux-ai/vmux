use bevy_app::{App, Update};
use bevy_ecs::prelude::*;
use vmux_ecs::JobSet;

pub struct AccountPlugin;

impl bevy_app::Plugin for AccountPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<vmux_ecs::record::RecordPlugin>() {
            app.add_plugins(vmux_ecs::record::RecordPlugin);
        }
        if !app.is_plugin_added::<vmux_ecs::OperationPlugin>() {
            app.add_plugins(vmux_ecs::OperationPlugin);
        }
        app.add_plugins((
            crate::catalog::CatalogPlugin,
            crate::mutations::MutationsPlugin,
            crate::connection::ConnectionPlugin,
        ))
        .configure_sets(
            Update,
            (
                Set::Receive.after(JobSet::Apply),
                Set::Seed,
                vmux_ecs::record::Set::Restore,
                Set::Adopt,
                Set::Prepare,
                vmux_ecs::record::Set::Detect,
                vmux_ecs::record::Set::Capture,
                Set::Write,
                Set::Dispatch,
                Set::Apply,
                vmux_ecs::record::Set::Confirm,
                Set::Reply,
                Set::Publish,
            )
                .chain(),
        );
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Set {
    Receive,
    Seed,
    Adopt,
    Prepare,
    Write,
    Dispatch,
    Apply,
    Reply,
    Publish,
}

#[cfg(all(test, feature = "local"))]
mod tests {
    use super::*;
    use crate::connection::{
        Authorization, Authorized, Challenge, DeviceKey, Grant, KeyPrepared, KeyRequest,
        OpenRequest, PollReady, PollRequest, Registered, RemoteAccount, SendRequest, SessionToken,
        TransportReady, Verified, Waiting,
    };
    use crate::wire::Request;
    use crate::{
        Binding, CancelConnectRequest, ConnectRequest, Connection, DisconnectRequest,
        Disconnecting, LinkRequest, Phase, Server, SyncEnabled, SyncRequest,
    };
    use crate::{CreateRequest, Error, Failure, MutationCompleted, RenameRequest, Root};
    use crate::{Owner, Profile};
    use ring::signature::KeyPair;
    use uuid::Uuid;
    use vmux_ecs::{ProfileId, Ready, Revision, Uuid as Id};

    struct Fixture {
        app: App,
        root: Entity,
    }

    impl Fixture {
        fn account(&self) -> (Entity, Uuid, u64) {
            for entity in self.app.world().get::<Children>(self.root).unwrap().iter() {
                if self.app.world().get::<crate::Account>(entity).is_some() {
                    return (
                        entity,
                        self.app
                            .world()
                            .get::<Id>(entity)
                            .unwrap()
                            .0
                            .parse()
                            .unwrap(),
                        self.app.world().get::<Revision>(entity).unwrap().0,
                    );
                }
            }
            panic!("account is missing")
        }

        fn connection(&self) -> Option<Entity> {
            self.app
                .world()
                .get::<Children>(self.root)?
                .iter()
                .find(|&entity| self.app.world().get::<Connection>(entity).is_some())
        }

        fn phase(&mut self, expected: Phase) -> Entity {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            loop {
                self.app.update();
                if let Some(entity) = self.connection()
                    && self.app.world().get::<Phase>(entity) == Some(&expected)
                {
                    return entity;
                }
                if let Some(error) = self.app.world().get::<crate::ConnectionError>(self.root) {
                    panic!("connection failed: {:?}", error.0);
                }
                assert!(std::time::Instant::now() < deadline, "connection timed out");
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
        }

        fn gateway(&mut self, account: Uuid, approved: bool) {
            self.app.add_plugins(GatewayPlugin);
            self.app.world_mut().entity_mut(self.root).insert((
                crate::CloudAvailable,
                Server {
                    host: "accounts.example".into(),
                    port: 8788,
                },
            ));
            let mut gateway =
                self.app
                    .world_mut()
                    .spawn((Gateway, Id(account.to_string()), ChildOf(self.root)));
            if approved {
                gateway.insert(Approved);
            }
        }

        fn connect(&mut self) {
            let account_id = self.account().1;
            self.app.world_mut().write_message(ConnectRequest {
                root: self.root,
                account_id,
            });
        }

        fn select(&mut self, profile_id: Uuid, enabled: bool) -> MutationCompleted {
            let (_, account_id, binding_revision) = self.account();
            let operation_id = Uuid::new_v4();
            self.app.world_mut().write_message(SyncRequest {
                root: self.root,
                operation_id,
                account_id,
                binding_revision,
                profile_id,
                enabled,
            });
            self.result(operation_id)
        }
        fn at(data: &std::path::Path) -> Self {
            let mut app = App::new();
            app.add_plugins((AccountPlugin, crate::local::LocalPlugin));
            let root = app.world_mut().spawn(Root::at(data, "personal")).id();
            let mut fixture = Self { app, root };
            fixture.ready();
            fixture
        }

        fn ready(&mut self) {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            loop {
                self.app.update();
                if self.app.world().get::<Ready>(self.root).is_some() {
                    return;
                }
                if let Some(error) = self.app.world().get::<Error>(self.root) {
                    panic!("account load failed: {:?}", error.0)
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "account load timed out"
                );
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
        }

        fn profile(&mut self, directory_id: &str) -> (Uuid, String, Uuid, u64) {
            let mut rows = self
                .app
                .world_mut()
                .query_filtered::<(&Id, &ProfileId, &Name, &Owner, &Revision), With<Profile>>();
            let (_, id, name, owner, revision) = {
                let row = rows
                    .iter(self.app.world())
                    .find(|(_, directory, _, _, _)| directory.0 == directory_id)
                    .expect("profile exists");
                (
                    row.1.0.clone(),
                    row.0.0.parse().unwrap(),
                    row.2.as_str().to_string(),
                    row.3.0,
                    row.4.0,
                )
            };
            let account = self
                .app
                .world()
                .get::<Id>(owner)
                .unwrap()
                .0
                .parse()
                .unwrap();
            (id, name, account, revision)
        }

        fn entity(&mut self, directory_id: &str) -> Entity {
            let mut profiles = self
                .app
                .world_mut()
                .query_filtered::<(Entity, &ProfileId), With<Profile>>();
            for (entity, id) in profiles.iter(self.app.world()) {
                if id.0 == directory_id {
                    return entity;
                }
            }
            panic!("profile does not exist: {directory_id}")
        }

        fn create(&mut self, id: Uuid, name: &str) -> MutationCompleted {
            self.app.world_mut().write_message(CreateRequest {
                root: self.root,
                operation_id: id,
                name: name.into(),
            });
            self.result(id)
        }

        fn rename(&mut self, id: Uuid, profile_id: Uuid, name: &str) -> MutationCompleted {
            self.app.world_mut().write_message(RenameRequest {
                root: self.root,
                operation_id: id,
                profile_id,
                name: name.into(),
            });
            self.result(id)
        }

        fn result(&mut self, id: Uuid) -> MutationCompleted {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            loop {
                self.app.update();
                let results: Vec<_> = self
                    .app
                    .world_mut()
                    .resource_mut::<Messages<MutationCompleted>>()
                    .drain()
                    .collect();
                for result in results {
                    if result.operation_id == id {
                        return result;
                    }
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "profile mutation timed out"
                );
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
        }
    }

    struct GatewayPlugin;

    impl bevy_app::Plugin for GatewayPlugin {
        fn build(&self, app: &mut App) {
            app.add_systems(Update, (open, key, respond, delay).in_set(Set::Dispatch));
        }
    }

    #[derive(Component)]
    struct Gateway;

    #[derive(Component)]
    struct Approved;

    fn open(
        mut requests: MessageReader<OpenRequest>,
        active: Query<(), With<Connection>>,
        mut commands: Commands,
    ) {
        for request in requests.read() {
            if active.contains(request.entity) {
                commands.entity(request.entity).insert(TransportReady);
            }
        }
    }

    fn key(
        mut requests: MessageReader<KeyRequest>,
        active: Query<(), With<Connection>>,
        mut commands: Commands,
    ) {
        for request in requests.read() {
            if active.contains(request.entity) {
                commands
                    .entity(request.entity)
                    .insert(KeyPrepared(Some(Ok(DeviceKey {
                        id: Uuid::from_u128(7),
                        seed: zeroize::Zeroizing::new([3; 32]),
                        reused: false,
                    }))));
            }
        }
    }

    fn respond(
        mut requests: MessageReader<SendRequest>,
        active: Query<&ChildOf, With<Connection>>,
        gateways: Query<(&ChildOf, &Id, Has<Approved>), With<Gateway>>,
        mut commands: Commands,
    ) {
        for request in requests.read() {
            let Ok(owner) = active.get(request.entity) else {
                continue;
            };
            let (_, remote_id, approved) = gateways
                .iter()
                .find(|(parent, _, _)| parent.parent() == owner.parent())
                .unwrap();
            let mut target = commands.entity(request.entity);
            match &request.request {
                Request::AuthorizeDevice => {
                    target.insert((
                        Authorized,
                        Authorization {
                            user_code: "ABCD-EFGH".into(),
                            verification_uri: "https://accounts.example/device".into(),
                        },
                        Grant {
                            code: "private-grant".into(),
                            expires: std::time::Instant::now() + std::time::Duration::from_secs(60),
                            interval: 5,
                        },
                    ));
                }
                Request::Token { device_code } => {
                    assert_eq!(device_code, "private-grant");
                    if approved {
                        target.insert(SessionToken("private-session".into()));
                    } else {
                        target.insert(Waiting);
                    }
                }
                Request::RegisterDevice { session_token, .. } => {
                    assert_eq!(session_token, "private-session");
                    target.insert(Registered);
                }
                Request::Challenge { .. } => {
                    target.insert(Challenge("05".repeat(32)));
                }
                Request::Authenticate { signature } => {
                    let key =
                        ring::signature::Ed25519KeyPair::from_seed_unchecked(&[3; 32]).unwrap();
                    let mut bytes = Vec::new();
                    for offset in (0..signature.len()).step_by(2) {
                        bytes.push(u8::from_str_radix(&signature[offset..offset + 2], 16).unwrap());
                    }
                    ring::signature::UnparsedPublicKey::new(
                        &ring::signature::ED25519,
                        key.public_key(),
                    )
                    .verify(&[5; 32], &bytes)
                    .unwrap();
                    target.insert(Verified);
                }
                Request::Account => {
                    target.insert(RemoteAccount(remote_id.0.parse().unwrap()));
                }
            }
        }
    }

    fn delay(
        mut requests: MessageReader<PollRequest>,
        active: Query<(), With<Connection>>,
        mut commands: Commands,
    ) {
        for request in requests.read() {
            if active.contains(request.entity) {
                commands.entity(request.entity).insert(PollReady);
            }
        }
    }

    #[test]
    fn authenticated_link_keeps_local_identity_and_requires_durable_explicit_profile_consent() {
        let data = tempfile::tempdir().unwrap();
        let mut fixture = Fixture::at(data.path());
        let local_id = fixture.account().1;
        let profile = fixture.profile("personal");
        let remote = Uuid::from_u128(99);
        fixture.gateway(remote, true);
        fixture.connect();
        fixture.phase(Phase::Connected);
        let account = fixture.account();
        assert_eq!(account.1, local_id);
        assert_eq!(fixture.profile("personal"), profile);
        assert_eq!(
            fixture
                .app
                .world()
                .get::<Binding>(account.0)
                .unwrap()
                .account_id
                .as_deref(),
            Some(remote.to_string().as_str())
        );
        let entity = fixture.entity("personal");
        assert!(
            !fixture
                .app
                .world()
                .get::<SyncEnabled>(entity)
                .is_some_and(|enabled| enabled.0)
        );
        fixture.select(profile.0, true).result.unwrap();
        let database = rusqlite::Connection::open(data.path().join("accounts.sqlite")).unwrap();
        let values: String = database
            .query_row("select group_concat(value) from records", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert!(!values.contains("private-grant") && !values.contains("private-session"));
        drop(fixture);
        let mut restored = Fixture::at(data.path());
        assert_eq!(restored.account().1, local_id);
        let restored_profile = restored.entity("personal");
        assert!(
            restored
                .app
                .world()
                .get::<SyncEnabled>(restored_profile)
                .unwrap()
                .0
        );
        restored.app.world_mut().write_message(DisconnectRequest {
            root: restored.root,
            account_id: local_id,
        });
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            restored.app.update();
            if !restored
                .app
                .world()
                .get::<Binding>(restored.account().0)
                .unwrap()
                .is_linked()
                && restored
                    .app
                    .world()
                    .get::<Disconnecting>(restored.root)
                    .is_none()
            {
                break;
            }
            assert!(std::time::Instant::now() < deadline, "disconnect timed out");
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        let entity = restored.entity("personal");
        assert!(!restored.app.world().get::<SyncEnabled>(entity).unwrap().0);
        assert_eq!(restored.profile("personal").0, profile.0);
        drop(restored);
        let mut reopened = Fixture::at(data.path());
        assert!(
            !reopened
                .app
                .world()
                .get::<Binding>(reopened.account().0)
                .unwrap()
                .is_linked()
        );
        let entity = reopened.entity("personal");
        assert!(!reopened.app.world().get::<SyncEnabled>(entity).unwrap().0);
    }

    #[test]
    fn canceled_authentication_and_stale_context_cannot_link_or_select_profiles() {
        let data = tempfile::tempdir().unwrap();
        let mut fixture = Fixture::at(data.path());
        fixture.gateway(Uuid::from_u128(99), false);
        let profile = fixture.profile("personal").0;
        let local_id = fixture.account().1;
        assert_eq!(
            fixture.select(profile, true).result.unwrap_err(),
            Failure::not_linked()
        );
        fixture.connect();
        let old = fixture.phase(Phase::AwaitingApproval);
        let connection_id = fixture
            .app
            .world()
            .get::<Id>(old)
            .unwrap()
            .0
            .parse()
            .unwrap();
        fixture.app.world_mut().write_message(CancelConnectRequest {
            root: fixture.root,
            account_id: local_id,
            connection_id,
        });
        fixture.app.update();
        assert!(fixture.app.world().get_entity(old).is_err());
        assert!(
            fixture
                .app
                .world()
                .get::<Binding>(fixture.account().0)
                .is_none()
        );
        fixture.connect();
        let replacement = fixture.phase(Phase::AwaitingApproval);
        fixture.app.world_mut().write_message(CancelConnectRequest {
            root: fixture.root,
            account_id: local_id,
            connection_id,
        });
        fixture.app.update();
        assert_eq!(fixture.connection(), Some(replacement));
        fixture
            .app
            .world_mut()
            .entity_mut(replacement)
            .insert((Phase::LookingUp, RemoteAccount(Uuid::from_u128(99))));
        fixture.phase(Phase::Connected);
        let (account, account_id, revision) = fixture.account();
        let operation_id = Uuid::new_v4();
        fixture.app.world_mut().write_message(SyncRequest {
            root: fixture.root,
            operation_id,
            account_id,
            binding_revision: revision - 1,
            profile_id: profile,
            enabled: true,
        });
        assert_eq!(
            fixture.result(operation_id).result.unwrap_err(),
            Failure::account_mismatch()
        );
        let operation_id = Uuid::new_v4();
        fixture.app.world_mut().write_message(LinkRequest {
            root: fixture.root,
            operation_id,
            account_id,
            binding: Binding {
                server: "accounts.example:8788".into(),
                account_id: Some(Uuid::from_u128(100).to_string()),
                credential_generation: revision,
            },
        });
        assert_eq!(
            fixture.result(operation_id).result.unwrap_err(),
            Failure::account_mismatch()
        );
        assert_eq!(
            fixture
                .app
                .world()
                .get::<Binding>(account)
                .unwrap()
                .account_id,
            Some(Uuid::from_u128(99).to_string())
        );
    }

    #[test]
    fn a_disabled_cloud_gate_refuses_sync_enable_without_rewriting_local_preferences() {
        let data = tempfile::tempdir().unwrap();
        let mut fixture = Fixture::at(data.path());
        let (account, _, generation) = fixture.account();
        fixture.app.world_mut().entity_mut(account).insert(Binding {
            server: "accounts.example:8788".into(),
            account_id: Some(Uuid::from_u128(99).to_string()),
            credential_generation: generation,
        });
        let profile = fixture.profile("personal").0;
        assert_eq!(
            fixture.select(profile, true).result.unwrap_err(),
            Failure::access_denied()
        );
        let entity = fixture.entity("personal");
        assert!(fixture.app.world().get::<SyncEnabled>(entity).is_none());
        fixture.select(profile, false).result.unwrap();
        assert!(!fixture.app.world().get::<SyncEnabled>(entity).unwrap().0);
    }

    #[cfg(feature = "cloud")]
    #[test]
    fn a_current_off_cancels_cloud_work_and_preserves_accepted_local_writes_after_restart() {
        let data = tempfile::tempdir().unwrap();
        let mut fixture = Fixture::at(data.path());
        fixture.gateway(Uuid::from_u128(99), true);
        fixture.app.add_plugins((
            vmux_feature::FeaturePlugin,
            crate::availability::AvailabilityPlugin,
        ));
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.root)
            .insert(crate::availability::SecureCredentials);
        let provider = fixture
            .app
            .world_mut()
            .spawn(vmux_feature::Provider::bundle("production"))
            .id();
        fixture
            .app
            .world_mut()
            .write_message(vmux_feature::Tick(1000));
        fixture
            .app
            .world_mut()
            .write_message(vmux_feature::Receive {
                provider,
                generation: 0,
                document: vmux_feature::snapshot::Document {
                    schema_version: 1,
                    environment: "production".into(),
                    revision: 1,
                    issued_at: 1000,
                    valid_until: 1100,
                    flags: std::collections::BTreeMap::from([("account.cloud".into(), true)]),
                },
            });
        fixture.app.update();
        fixture.connect();
        fixture.phase(Phase::Connected);
        let (account, account_id, _) = fixture.account();
        let profile_id = fixture.profile("personal").0;
        fixture.select(profile_id, true).result.unwrap();
        let rename = Uuid::new_v4();
        fixture.app.world_mut().write_message(RenameRequest {
            root: fixture.root,
            operation_id: rename,
            profile_id,
            name: "Still local".into(),
        });
        fixture.app.update();
        fixture
            .app
            .world_mut()
            .write_message(vmux_feature::Receive {
                provider,
                generation: 0,
                document: vmux_feature::snapshot::Document {
                    schema_version: 1,
                    environment: "production".into(),
                    revision: 2,
                    issued_at: 1000,
                    valid_until: 1100,
                    flags: std::collections::BTreeMap::from([("account.cloud".into(), false)]),
                },
            });
        fixture.result(rename).result.unwrap();
        assert!(fixture.connection().is_none());
        assert!(
            fixture
                .app
                .world()
                .get::<crate::CloudAvailable>(fixture.root)
                .is_none()
        );
        assert_eq!(
            fixture
                .app
                .world()
                .get::<Binding>(account)
                .unwrap()
                .account_id,
            Some(Uuid::from_u128(99).to_string())
        );
        assert_eq!(
            fixture.select(profile_id, true).result.unwrap_err(),
            Failure::access_denied()
        );
        drop(fixture);
        let mut restored = Fixture::at(data.path());
        assert_eq!(restored.account().1, account_id);
        assert_eq!(restored.profile("personal").0, profile_id);
        assert_eq!(restored.profile("personal").1, "Still local");
        let entity = restored.entity("personal");
        assert!(restored.app.world().get::<SyncEnabled>(entity).unwrap().0);
        let account = restored.account().0;
        assert_eq!(
            restored
                .app
                .world()
                .get::<Binding>(account)
                .unwrap()
                .account_id,
            Some(Uuid::from_u128(99).to_string())
        );
        assert!(
            restored
                .app
                .world()
                .get::<crate::CloudAvailable>(restored.root)
                .is_none()
        );
    }

    #[test]
    fn failed_binding_and_sync_writes_preserve_committed_settings_and_allow_retry() {
        let data = tempfile::tempdir().unwrap();
        let mut fixture = Fixture::at(data.path());
        let (account, account_id, generation) = fixture.account();
        let binding = Binding {
            server: "accounts.example:8788".into(),
            account_id: Some(Uuid::from_u128(99).to_string()),
            credential_generation: generation,
        };
        let held = rusqlite::Connection::open(data.path().join("accounts.sqlite")).unwrap();
        held.execute_batch("begin immediate").unwrap();
        let operation_id = Uuid::new_v4();
        fixture.app.world_mut().write_message(LinkRequest {
            root: fixture.root,
            operation_id,
            account_id,
            binding: binding.clone(),
        });
        assert!(matches!(
            fixture.result(operation_id).result,
            Err(Failure {
                kind: crate::FailureKind::Storage,
                ..
            })
        ));
        assert!(fixture.app.world().get::<Binding>(account).is_none());
        held.execute_batch("rollback").unwrap();
        fixture.app.world_mut().write_message(LinkRequest {
            root: fixture.root,
            operation_id,
            account_id,
            binding: binding.clone(),
        });
        fixture.result(operation_id).result.unwrap();
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.root)
            .insert(crate::CloudAvailable);
        let profile = fixture.profile("personal").0;
        held.execute_batch("begin immediate").unwrap();
        assert!(matches!(
            fixture.select(profile, true).result,
            Err(Failure {
                kind: crate::FailureKind::Storage,
                ..
            })
        ));
        let entity = fixture.entity("personal");
        assert!(
            !fixture
                .app
                .world()
                .get::<SyncEnabled>(entity)
                .is_some_and(|enabled| enabled.0)
        );
        held.execute_batch("rollback").unwrap();
        fixture.select(profile, true).result.unwrap();
        let (_, _, revision) = fixture.account();
        let refresh = Uuid::new_v4();
        fixture.app.world_mut().write_message(LinkRequest {
            root: fixture.root,
            operation_id: refresh,
            account_id,
            binding,
        });
        let selection = Uuid::new_v4();
        fixture.app.world_mut().write_message(SyncRequest {
            root: fixture.root,
            operation_id: selection,
            account_id,
            binding_revision: revision,
            profile_id: profile,
            enabled: false,
        });
        assert_eq!(
            fixture.result(selection).result.unwrap_err(),
            Failure::account_mismatch()
        );
        assert!(fixture.app.world().get::<SyncEnabled>(entity).unwrap().0);
    }

    #[test]
    fn legacy_profiles_and_offline_edits_restore_with_stable_identity_and_ownership() {
        let data = tempfile::tempdir().unwrap();
        let directory = data.path().join("profiles/personal");
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("display_name"), "Original").unwrap();
        std::fs::write(directory.join("history"), "preserved").unwrap();
        let mut fixture = Fixture::at(data.path());
        let before = fixture.profile("personal");
        assert_eq!(before.1, "Original");
        let result = fixture.rename(Uuid::new_v4(), before.0, "Renamed");
        assert_eq!(result.result.unwrap().revision, 2);
        let created = fixture.create(Uuid::new_v4(), "Work");
        let receipt = created.result.unwrap();
        assert_eq!(created.directory_id.as_deref(), Some("work"));
        assert_eq!(fixture.profile("work").2, before.2);
        drop(fixture);
        let mut restored = Fixture::at(data.path());
        assert_eq!(
            restored.profile("personal"),
            (before.0, "Renamed".into(), before.2, 2)
        );
        assert_eq!(
            restored.profile("work"),
            (receipt.entity_id, "Work".into(), before.2, 1)
        );
        assert_eq!(
            std::fs::read_to_string(directory.join("history")).unwrap(),
            "preserved"
        );
    }

    #[test]
    fn replay_returns_the_original_receipt_without_reverting_newer_profile_edits() {
        let data = tempfile::tempdir().unwrap();
        let mut fixture = Fixture::at(data.path());
        let operation = Uuid::new_v4();
        let first = fixture.create(operation, "Work").result.unwrap();
        fixture
            .rename(Uuid::new_v4(), first.entity_id, "Latest")
            .result
            .unwrap();
        let replay = fixture.create(operation, "Work");
        assert_eq!(replay.result.unwrap(), first);
        assert_eq!(replay.directory_id.as_deref(), Some("work"));
        assert_eq!(fixture.profile("work").1, "Latest");
        assert!(!data.path().join("profiles/work-2").exists());
    }

    #[test]
    fn concurrent_hosts_refuse_stale_writes_and_refresh_without_losing_the_draft() {
        let data = tempfile::tempdir().unwrap();
        let mut first = Fixture::at(data.path());
        let mut second = Fixture::at(data.path());
        let profile = first.profile("personal").0;
        assert_eq!(second.profile("personal").0, profile);
        first
            .rename(Uuid::new_v4(), profile, "Accepted")
            .result
            .unwrap();
        let refused = second.rename(Uuid::new_v4(), profile, "Unaccepted");
        assert_eq!(refused.result.unwrap_err(), Failure::conflict());
        second.ready();
        assert_eq!(second.profile("personal").1, "Accepted");
        second
            .rename(Uuid::new_v4(), profile, "Retried")
            .result
            .unwrap();
        drop(first);
        drop(second);
        assert_eq!(Fixture::at(data.path()).profile("personal").1, "Retried");
    }

    #[test]
    fn failed_writes_preserve_committed_names_and_can_retry_after_a_database_lock() {
        let data = tempfile::tempdir().unwrap();
        let mut fixture = Fixture::at(data.path());
        let before = fixture.profile("personal");
        let held = rusqlite::Connection::open(data.path().join("accounts.sqlite")).unwrap();
        held.execute_batch("begin immediate").unwrap();
        let rejected = fixture.rename(Uuid::new_v4(), before.0, "Blocked");
        assert!(matches!(
            rejected.result,
            Err(Failure {
                kind: crate::FailureKind::Storage,
                ..
            })
        ));
        assert_eq!(fixture.profile("personal"), before);
        let rejected = fixture.create(Uuid::new_v4(), "Blocked create");
        assert!(matches!(
            rejected.result,
            Err(Failure {
                kind: crate::FailureKind::Storage,
                ..
            })
        ));
        assert!(!data.path().join("profiles/blocked-create").exists());
        let mut profiles = fixture
            .app
            .world_mut()
            .query_filtered::<Entity, With<Profile>>();
        assert_eq!(profiles.iter(fixture.app.world()).count(), 1);
        held.execute_batch("rollback").unwrap();
        fixture
            .rename(Uuid::new_v4(), before.0, "Retry")
            .result
            .unwrap();
        assert_eq!(fixture.profile("personal").1, "Retry");
        fixture
            .create(Uuid::new_v4(), "Blocked create")
            .result
            .unwrap();
        drop(fixture);
        assert_eq!(
            Fixture::at(data.path()).profile("blocked-create").1,
            "Blocked create"
        );
    }

    #[test]
    fn failed_capture_returns_a_result_and_allows_a_later_valid_write() {
        let data = tempfile::tempdir().unwrap();
        let mut fixture = Fixture::at(data.path());
        let before = fixture.profile("personal");
        let entity = fixture.entity("personal");
        let scope = fixture
            .app
            .world()
            .get::<vmux_ecs::record::Scope>(entity)
            .unwrap()
            .0;
        fixture
            .app
            .world_mut()
            .entity_mut(entity)
            .insert(vmux_ecs::record::Scope(Uuid::new_v4()));
        let rejected = fixture.rename(Uuid::new_v4(), before.0, "Rejected");
        assert!(matches!(
            rejected.result,
            Err(Failure {
                kind: crate::FailureKind::Storage,
                ..
            })
        ));
        assert_eq!(fixture.profile("personal"), before);
        fixture
            .app
            .world_mut()
            .entity_mut(entity)
            .insert(vmux_ecs::record::Scope(scope));
        fixture
            .rename(Uuid::new_v4(), before.0, "Retry")
            .result
            .unwrap();
        drop(fixture);
        assert_eq!(Fixture::at(data.path()).profile("personal").1, "Retry");
    }

    #[test]
    fn exhausted_revisions_return_a_result_without_writing_a_new_name() {
        let data = tempfile::tempdir().unwrap();
        let mut fixture = Fixture::at(data.path());
        let before = fixture.profile("personal");
        let entity = fixture.entity("personal");
        fixture
            .app
            .world_mut()
            .get_mut::<Revision>(entity)
            .unwrap()
            .0 = i64::MAX as u64;
        let rejected = fixture.rename(Uuid::new_v4(), before.0, "Rejected");
        assert_eq!(rejected.result.unwrap_err(), Failure::not_ready());
        assert_eq!(fixture.profile("personal").1, before.1);
        fixture
            .app
            .world_mut()
            .get_mut::<Revision>(entity)
            .unwrap()
            .0 = before.3;
        fixture
            .rename(Uuid::new_v4(), before.0, "Retry")
            .result
            .unwrap();
        drop(fixture);
        assert_eq!(Fixture::at(data.path()).profile("personal").1, "Retry");
    }
}
