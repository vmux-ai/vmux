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
    #[cfg(feature = "profile-sync")]
    use bevy_ecs::system::RunSystemOnce;
    #[cfg(feature = "profile-sync")]
    use ring::rand::SecureRandom;
    use ring::signature::KeyPair;
    #[cfg(feature = "profile-sync")]
    use std::io::{Read, Write};
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
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(12);
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
                assert!(
                    std::time::Instant::now() < deadline,
                    "connection timed out waiting for {:?}; current: {:?}",
                    std::mem::discriminant(&expected),
                    self.connection()
                        .and_then(|entity| self.app.world().get::<Phase>(entity))
                        .map(std::mem::discriminant)
                );
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

        #[cfg(feature = "profile-sync")]
        fn transfer(&mut self) {
            self.app.add_plugins(crate::sync_runtime::ProfileSyncPlugin);
            let provider = self
                .app
                .world_mut()
                .spawn(vmux_feature::Provider::bundle("development"))
                .id();
            self.app.world_mut().write_message(vmux_feature::Tick(1000));
            let document = vmux_feature::snapshot::Document::decode(br#"{"schema_version":1,"environment":"development","revision":1,"issued_at":1000,"valid_until":2000,"flags":{"profile.sync":true}}"#).unwrap();
            self.app.world_mut().write_message(vmux_feature::Receive {
                provider,
                generation: 0,
                document,
            });
            self.app.update();
        }

        #[cfg(feature = "profile-sync")]
        fn upload(&mut self) -> crate::sync::UploadRequest {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
            loop {
                self.app.update();
                let request = self
                    .app
                    .world_mut()
                    .resource_mut::<Messages<crate::sync::UploadRequest>>()
                    .drain()
                    .next();
                if let Some(request) = request {
                    return request;
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "selected Profile was not dispatched"
                );
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
        }

        #[cfg(feature = "profile-sync")]
        fn listed(&mut self, records: Vec<vmux_ecs::record::Record>) {
            self.page(records, None);
        }

        #[cfg(feature = "profile-sync")]
        fn page(&mut self, records: Vec<vmux_ecs::record::Record>, next: Option<Uuid>) {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
            loop {
                self.app.update();
                let request = self
                    .app
                    .world_mut()
                    .resource_mut::<Messages<crate::sync::ListRequest>>()
                    .drain()
                    .next();
                if let Some(request) = request {
                    self.app.world_mut().entity_mut(request.entity).insert(
                        crate::cloud::transport::Reply(Some(Ok(crate::wire::Response::Entities {
                            entities: records,
                            next,
                        }))),
                    );
                    self.app.update();
                    return;
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "remote Profile enumeration did not start"
                );
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
        }

        #[cfg(feature = "profile-sync")]
        fn native(&mut self) {
            let port = std::env::var("VMUX_LOCAL_CLOUD_TEST_ACCOUNT_PORT")
                .expect("local Account test port")
                .parse()
                .unwrap();
            std::env::var("VMUX_LOCAL_QUIC_FINGERPRINT")
                .expect("exact local certificate fingerprint");
            let mut seed = [0; 32];
            ring::rand::SystemRandom::new().fill(&mut seed).unwrap();
            self.app
                .add_plugins((crate::cloud::transport::TransportPlugin, GatewayPlugin));
            self.app.world_mut().entity_mut(self.root).insert((
                crate::CloudAvailable,
                Server {
                    host: "localhost".into(),
                    port,
                },
            ));
            self.app.world_mut().spawn((
                NativeIdentity,
                ChildOf(self.root),
                DeviceKey {
                    id: Uuid::new_v4(),
                    seed: zeroize::Zeroizing::new(seed),
                    reused: false,
                },
            ));
            self.connect();
            let connection = self.phase(Phase::AwaitingApproval);
            let code = self
                .app
                .world()
                .get::<crate::Authorization>(connection)
                .unwrap()
                .user_code
                .clone();
            let port = std::env::var("VMUX_LOCAL_CLOUD_TEST_HTTP_PORT")
                .expect("local approval test port")
                .parse()
                .unwrap();
            let form = NativeHttp::request(port, "GET", "/development/devices", "");
            let token = form
                .split("name=\"token\" value=\"")
                .nth(1)
                .unwrap()
                .split('"')
                .next()
                .unwrap();
            let body = url::form_urlencoded::Serializer::new(String::new())
                .append_pair("user_code", &code)
                .append_pair("token", token)
                .finish();
            NativeHttp::request(port, "POST", "/development/devices/approve", &body);
            self.phase(Phase::Connected);
        }

        #[cfg(feature = "profile-sync")]
        fn synced(&mut self, profile: Uuid, remote_revision: u64) {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(12);
            loop {
                self.app.update();
                let world = self.app.world_mut();
                let committed = world
                    .query_filtered::<(
                        &Id,
                        &Revision,
                        &crate::RemoteRevision,
                        &crate::SyncedRevision,
                    ), (With<Profile>, Without<crate::mutations::Proposal>)>()
                    .iter(world)
                    .any(|(id, local, remote, synced)| {
                        id.0 == profile.to_string()
                            && remote.0 == remote_revision
                            && synced.0 == local.0
                    });
                if committed {
                    return;
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "native Profile synchronization did not commit"
                );
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
        }

        #[cfg(feature = "profile-sync")]
        fn remote(&mut self, profile: Uuid, revision: u64) {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(12);
            loop {
                self.app.update();
                let world = self.app.world_mut();
                let found = world
                    .query_filtered::<(&Id, &Revision), With<crate::RemoteProfile>>()
                    .iter(world)
                    .any(|(id, current)| id.0 == profile.to_string() && current.0 == revision);
                if found {
                    return;
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "native remote Profile enumeration did not complete"
                );
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
        }

        #[cfg(feature = "profile-sync")]
        fn conflict(&mut self, profile: Uuid) -> Uuid {
            let root = self.root;
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
            loop {
                self.app.update();
                let current = self
                    .app
                    .world_mut()
                    .run_system_once(move |projection: crate::SyncProjection| {
                        let (_, busy, conflicted) = projection.activity(root, profile);
                        if busy || !conflicted {
                            return None;
                        }
                        projection.conflict(root, profile).map(|value| value.0)
                    })
                    .unwrap();
                if let Some(current) = current {
                    return current;
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "Profile conflict did not settle"
                );
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
        }

        #[cfg(feature = "profile-sync")]
        fn received(
            &self,
            id: Uuid,
            revision: u64,
            name: Option<&str>,
        ) -> vmux_ecs::record::Record {
            let account = self
                .app
                .world()
                .get::<Binding>(self.account().0)
                .unwrap()
                .account_id
                .as_ref()
                .unwrap()
                .clone();
            let mut components =
                vec![serde_json::json!({"key":"profile","version":1,"value":null})];
            if let Some(name) = name {
                components.push(serde_json::json!({"key":"name","version":1,"value":name}));
            }
            serde_json::from_value(serde_json::json!({"id":id,"scope_id":account,"revision":revision,"components":components,"relationships":[{"key":"account","version":1,"target":account,"position":0,"owned":true}],"deleted":name.is_none()})).unwrap()
        }

        #[cfg(feature = "profile-sync")]
        fn imported(&mut self, id: Uuid, name: &str) -> (Entity, String) {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
            loop {
                self.app.update();
                let world = self.app.world_mut();
                let found = world
                    .query_filtered::<(Entity, &Id, &Name, &ProfileId), (With<Profile>, Without<crate::mutations::Proposal>)>()
                    .iter(world)
                    .find(|(_, current, current_name, _)| {
                        current.0 == id.to_string() && current_name.as_str() == name
                    })
                    .map(|(entity, _, _, directory)| (entity, directory.0.clone()));
                if let Some(found) = found {
                    return found;
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "remote Profile name was not committed"
                );
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
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

    #[cfg(feature = "profile-sync")]
    #[derive(Component)]
    struct NativeIdentity;

    #[cfg(feature = "profile-sync")]
    struct NativeHttp;

    #[cfg(feature = "profile-sync")]
    impl NativeHttp {
        fn request(port: u16, method: &str, path: &str, body: &str) -> String {
            let mut stream = std::net::TcpStream::connect_timeout(
                &std::net::SocketAddr::from(([127, 0, 0, 1], port)),
                std::time::Duration::from_secs(5),
            )
            .unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(10)))
                .unwrap();
            stream
                .set_write_timeout(Some(std::time::Duration::from_secs(10)))
                .unwrap();
            write!(stream, "{method} {path} HTTP/1.1\r\nHost: localhost:{port}\r\nOrigin: http://localhost:{port}\r\nConnection: close\r\nContent-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\n\r\n{body}", body.len()).unwrap();
            let mut output = String::new();
            stream.take(32 * 1024).read_to_string(&mut output).unwrap();
            assert!(
                output.starts_with("HTTP/1.1 200"),
                "local HTTP operation failed: {output}"
            );
            output
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
        active: Query<&ChildOf, With<Connection>>,
        gateways: Query<&ChildOf, With<Gateway>>,
        mut commands: Commands,
    ) {
        for request in requests.read() {
            if active.get(request.entity).is_ok_and(|parent| {
                gateways
                    .iter()
                    .any(|gateway| gateway.parent() == parent.parent())
            }) {
                commands.entity(request.entity).insert(TransportReady);
            }
        }
    }

    fn key(
        mut requests: MessageReader<KeyRequest>,
        active: Query<(), With<Connection>>,
        #[cfg(feature = "profile-sync")] connections: Query<&ChildOf, With<Connection>>,
        #[cfg(feature = "profile-sync")] identities: Query<
            (&ChildOf, &DeviceKey),
            With<NativeIdentity>,
        >,
        mut commands: Commands,
    ) {
        for request in requests.read() {
            #[cfg(feature = "profile-sync")]
            if let Ok(parent) = connections.get(request.entity)
                && let Some((_, key)) = identities
                    .iter()
                    .find(|(owner, _)| owner.parent() == parent.parent())
            {
                commands
                    .entity(request.entity)
                    .insert(KeyPrepared(Some(Ok(DeviceKey {
                        id: key.id,
                        seed: key.seed.clone(),
                        reused: key.reused,
                    }))));
                continue;
            }
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
            let Some((_, remote_id, approved)) = gateways
                .iter()
                .find(|(parent, _, _)| parent.parent() == owner.parent())
            else {
                continue;
            };
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
                #[cfg(feature = "cloud")]
                Request::Profiles { .. } | Request::ChangeProfile(_) => {
                    panic!("Profile synchronization needs the dedicated gateway fixture")
                }
            }
        }
    }

    fn delay(
        mut requests: MessageReader<PollRequest>,
        active: Query<&ChildOf, With<Connection>>,
        gateways: Query<&ChildOf, With<Gateway>>,
        mut commands: Commands,
    ) {
        for request in requests.read() {
            if active.get(request.entity).is_ok_and(|parent| {
                gateways
                    .iter()
                    .any(|gateway| gateway.parent() == parent.parent())
            }) {
                commands.entity(request.entity).insert(PollReady);
            }
        }
    }

    #[cfg(feature = "profile-sync")]
    #[test]
    #[ignore = "requires an explicitly selected isolated local Account and approval service"]
    fn two_native_clients_sync_through_the_local_account_without_application_launch() {
        let first_data = tempfile::tempdir().unwrap();
        let second_data = tempfile::tempdir().unwrap();
        let mut first = Fixture::at(first_data.path());
        first.transfer();
        first.native();
        let first_account = first.account().1;
        let profile = first.profile("personal").0;
        first
            .rename(Uuid::new_v4(), profile, "Native shared name")
            .result
            .unwrap();
        first.select(profile, true).result.unwrap();
        first.synced(profile, 1);
        let mut second = Fixture::at(second_data.path());
        second.transfer();
        second.native();
        let second_account = second.account().1;
        assert_ne!(first_account, second_account);
        second.remote(profile, 1);
        let second_binding = second.account().2;
        second.app.world_mut().write_message(crate::ImportRequest {
            root: second.root,
            profile_id: profile,
            binding_revision: second_binding,
            remote_revision: 1,
        });
        let (entity, directory) = second.imported(profile, "Native shared name");
        assert_eq!(
            second.app.world().get::<Owner>(entity).unwrap().0,
            second.account().0
        );
        let note = second_data
            .path()
            .join("profiles")
            .join(&directory)
            .join("native-fixture.txt");
        std::fs::write(&note, "local data stays on this device").unwrap();
        second
            .rename(Uuid::new_v4(), profile, "Native second device name")
            .result
            .unwrap();
        second.synced(profile, 2);
        first.imported(profile, "Native second device name");
        assert_eq!(
            std::fs::read_to_string(&note).unwrap(),
            "local data stays on this device"
        );
        drop(second);
        let mut reopened = Fixture::at(second_data.path());
        assert_eq!(reopened.profile(&directory).0, profile);
        assert_eq!(reopened.profile(&directory).1, "Native second device name");
        assert_eq!(reopened.account().1, second_account);
        reopened.transfer();
        reopened
            .rename(Uuid::new_v4(), profile, "Native offline edit")
            .result
            .unwrap();
        assert!(reopened.connection().is_none());
        reopened.native();
        reopened.synced(profile, 3);
        first.imported(profile, "Native offline edit");
        assert_eq!(
            std::fs::read_to_string(&note).unwrap(),
            "local data stays on this device"
        );
    }

    #[test]
    fn synchronized_names_refuse_oversized_utf8_without_losing_offline_edits() {
        let data = tempfile::tempdir().unwrap();
        let mut fixture = Fixture::at(data.path());
        fixture.gateway(Uuid::from_u128(99), true);
        fixture.connect();
        fixture.phase(Phase::Connected);
        let profile = fixture.profile("personal").0;
        let long_name = "日".repeat(86);
        fixture
            .rename(Uuid::new_v4(), profile, &long_name)
            .result
            .unwrap();
        assert_eq!(
            fixture.select(profile, true).result.unwrap_err().kind(),
            crate::FailureKind::InvalidName
        );
        let boundary_name = format!("{}a", "日".repeat(85));
        fixture
            .rename(Uuid::new_v4(), profile, &boundary_name)
            .result
            .unwrap();
        fixture.select(profile, true).result.unwrap();
        let committed = fixture.profile("personal");
        assert_eq!(
            fixture
                .rename(Uuid::new_v4(), profile, &long_name)
                .result
                .unwrap_err()
                .kind(),
            crate::FailureKind::InvalidName
        );
        assert_eq!(fixture.profile("personal"), committed);
        fixture.select(profile, false).result.unwrap();
        fixture
            .rename(Uuid::new_v4(), profile, &long_name)
            .result
            .unwrap();
        drop(fixture);
        let mut reopened = Fixture::at(data.path());
        assert_eq!(reopened.profile("personal").1, long_name);
    }

    #[cfg(feature = "profile-sync")]
    #[test]
    fn malformed_remote_pages_are_rejected_before_any_profile_or_cursor_is_published() {
        for case in [
            "duplicate",
            "descending",
            "cursor",
            "partial",
            "scope",
            "name",
            "tombstone",
        ] {
            let data = tempfile::tempdir().unwrap();
            let mut fixture = Fixture::at(data.path());
            fixture.transfer();
            fixture.gateway(Uuid::from_u128(99), true);
            fixture.connect();
            fixture.phase(Phase::Connected);
            let first = fixture.received(Uuid::from_u128(1), 1, Some("First"));
            let mut second = fixture.received(Uuid::from_u128(2), 1, Some("Second"));
            let mut next = None;
            let records = match case {
                "duplicate" => vec![first.clone(), first],
                "descending" => vec![second, first],
                "cursor" => {
                    next = Some(Uuid::from_u128(99));
                    (1..=16)
                        .map(|id| fixture.received(Uuid::from_u128(id), 1, Some("Page")))
                        .collect()
                }
                "partial" => {
                    next = Some(second.id);
                    vec![first, second]
                }
                "scope" => {
                    second.scope_id = Uuid::from_u128(100);
                    vec![first, second]
                }
                "name" => {
                    second.components[1].value = serde_json::Value::String("日".repeat(86));
                    vec![first, second]
                }
                "tombstone" => {
                    second.deleted = true;
                    second.components[1].key = "private_path".into();
                    vec![first, second]
                }
                _ => unreachable!(),
            };
            fixture.page(records, next);
            let world = fixture.app.world_mut();
            assert_eq!(
                world
                    .query_filtered::<Entity, With<crate::sync_runtime::RemoteProfile>>()
                    .iter(world)
                    .count(),
                0,
                "{case}"
            );
            assert_eq!(
                world
                    .resource_mut::<Messages<crate::sync::ListRequest>>()
                    .drain()
                    .count(),
                0,
                "{case}"
            );
        }
    }

    #[cfg(feature = "profile-sync")]
    #[test]
    fn enabling_transfer_honors_saved_consent_without_uploading_unselected_profiles() {
        let data = tempfile::tempdir().unwrap();
        let mut fixture = Fixture::at(data.path());
        fixture.gateway(Uuid::from_u128(99), true);
        fixture.connect();
        fixture.phase(Phase::Connected);
        let selected = fixture.profile("personal").0;
        let unselected = fixture
            .create(Uuid::new_v4(), "Unselected profile")
            .result
            .unwrap()
            .entity_id;
        fixture.select(selected, true).result.unwrap();
        drop(fixture);
        let mut fixture = Fixture::at(data.path());
        fixture.transfer();
        fixture.gateway(Uuid::from_u128(99), true);
        fixture.connect();
        fixture.phase(Phase::Connected);
        let upload = fixture.upload();
        assert_eq!(upload.change.id, selected);
        assert_ne!(upload.change.id, unselected);
        let database = rusqlite::Connection::open(data.path().join("accounts.sqlite")).unwrap();
        let queued: Vec<String> = database
            .prepare("select distinct profile_id from profile_outbox")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(queued, vec![selected.to_string()]);
    }

    #[cfg(feature = "profile-sync")]
    #[test]
    fn remote_conflict_resolution_rejects_stale_intent_and_rolls_back_a_failed_checkpoint() {
        let data = tempfile::tempdir().unwrap();
        let mut fixture = Fixture::at(data.path());
        fixture.transfer();
        fixture.gateway(Uuid::from_u128(99), true);
        fixture.connect();
        fixture.phase(Phase::Connected);
        let profile = fixture.profile("personal").0;
        fixture.select(profile, true).result.unwrap();
        let upload = fixture.upload();
        let remote = fixture.received(profile, 4, Some("Remote name"));
        fixture
            .app
            .world_mut()
            .entity_mut(upload.entity)
            .insert(crate::cloud::transport::Reply(Some(Ok(
                crate::wire::Response::Conflict {
                    current: Some(remote),
                },
            ))));
        let conflict = fixture.conflict(profile);
        let local = fixture.profile("personal");
        let binding_revision = fixture.account().2;
        let mut request = crate::UseRemoteRequest {
            root: fixture.root,
            profile_id: profile,
            binding_revision: binding_revision - 1,
            local_revision: local.3,
            remote_revision: 4,
            conflict_id: conflict,
        };
        fixture.app.world_mut().write_message(request);
        fixture.app.update();
        assert_eq!(fixture.profile("personal"), local);
        assert_eq!(fixture.conflict(profile), conflict);
        fixture
            .rename(Uuid::new_v4(), profile, "New local edit")
            .result
            .unwrap();
        request = crate::UseRemoteRequest {
            root: fixture.root,
            profile_id: profile,
            binding_revision,
            local_revision: local.3,
            remote_revision: 4,
            conflict_id: conflict,
        };
        fixture.app.world_mut().write_message(request);
        fixture.app.update();
        let current = fixture.profile("personal");
        assert_eq!(current.1, "New local edit");
        assert_eq!(fixture.conflict(profile), conflict);
        let database = rusqlite::Connection::open(data.path().join("accounts.sqlite")).unwrap();
        let pending: Vec<String> = database
            .prepare("select value from profile_outbox order by sequence")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        database.execute_batch("create trigger refuse_checkpoint before insert on profile_checkpoint begin select raise(abort, 'checkpoint refused'); end;").unwrap();
        fixture
            .app
            .world_mut()
            .write_message(crate::UseRemoteRequest {
                root: fixture.root,
                profile_id: profile,
                binding_revision,
                local_revision: current.3,
                remote_revision: 4,
                conflict_id: conflict,
            });
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
        loop {
            fixture.app.update();
            let failed = fixture
                .app
                .world_mut()
                .resource_mut::<Messages<MutationCompleted>>()
                .drain()
                .any(|result| result.result.is_err());
            if failed {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "failed adoption was not reported"
            );
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert_eq!(fixture.profile("personal"), current);
        assert_eq!(fixture.conflict(profile), conflict);
        let retained: Vec<String> = database
            .prepare("select value from profile_outbox order by sequence")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(retained, pending);
        database
            .execute_batch("drop trigger refuse_checkpoint;")
            .unwrap();
        fixture
            .app
            .world_mut()
            .write_message(crate::UseRemoteRequest {
                root: fixture.root,
                profile_id: profile,
                binding_revision,
                local_revision: current.3,
                remote_revision: 4,
                conflict_id: conflict,
            });
        fixture.imported(profile, "Remote name");
        drop(fixture);
        let mut reopened = Fixture::at(data.path());
        assert_eq!(reopened.profile("personal").1, "Remote name");
        let count: u64 = database
            .query_row("select count(*) from profile_outbox", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[cfg(feature = "profile-sync")]
    #[test]
    fn edits_during_a_send_use_the_acknowledged_base_and_keep_the_committed_local_name() {
        let data = tempfile::tempdir().unwrap();
        let mut fixture = Fixture::at(data.path());
        fixture.transfer();
        fixture.gateway(Uuid::from_u128(99), true);
        fixture.connect();
        fixture.phase(Phase::Connected);
        let profile = fixture.profile("personal").0;
        fixture.select(profile, true).result.unwrap();
        let first = fixture.upload();
        fixture
            .rename(Uuid::new_v4(), profile, "Later committed name")
            .result
            .unwrap();
        fixture
            .app
            .world_mut()
            .entity_mut(first.entity)
            .insert(crate::cloud::transport::Reply(Some(Ok(
                crate::wire::Response::Committed(vmux_ecs::record::Receipt {
                    operation_id: first.change.operation_id,
                    entity_id: profile,
                    revision: 1,
                }),
            ))));
        let later = fixture.upload();
        assert_eq!(later.change.base_revision, 1);
        assert_ne!(later.change.operation_id, first.change.operation_id);
        assert_eq!(fixture.profile("personal").1, "Later committed name");
        assert_eq!(later.change.name.as_deref(), Some("Later committed name"));
    }

    #[cfg(feature = "profile-sync")]
    #[test]
    fn revoking_consent_closes_the_dispatch_of_an_already_prepared_frame() {
        let data = tempfile::tempdir().unwrap();
        let mut fixture = Fixture::at(data.path());
        fixture.transfer();
        fixture.gateway(Uuid::from_u128(99), true);
        fixture.connect();
        fixture.phase(Phase::Connected);
        let profile = fixture.profile("personal").0;
        fixture.select(profile, true).result.unwrap();
        let request = fixture.upload();
        let entity = request.entity;
        let connection = request.connection;
        let change = request.change.clone();
        assert!(
            fixture
                .app
                .world_mut()
                .run_system_once(move |context: crate::sync_runtime::Context| context
                    .permits(entity, connection, &change))
                .unwrap()
        );
        fixture.select(profile, false).result.unwrap();
        let change = request.change;
        assert!(
            !fixture
                .app
                .world_mut()
                .run_system_once(move |context: crate::sync_runtime::Context| context
                    .permits(entity, connection, &change))
                .unwrap()
        );
        assert!(data.path().join("profiles/personal").exists());
    }

    #[cfg(feature = "profile-sync")]
    #[test]
    fn remote_import_updates_and_tombstones_preserve_local_account_identity_and_files() {
        let data = tempfile::tempdir().unwrap();
        let mut fixture = Fixture::at(data.path());
        let local_account = fixture.account().1;
        fixture.transfer();
        fixture.gateway(Uuid::from_u128(99), true);
        fixture.connect();
        fixture.phase(Phase::Connected);
        let profile = Uuid::new_v4();
        fixture.listed(vec![fixture.received(profile, 1, Some("Imported name"))]);
        let binding_revision = fixture.account().2;
        fixture.app.world_mut().write_message(crate::ImportRequest {
            root: fixture.root,
            profile_id: profile,
            binding_revision,
            remote_revision: 1,
        });
        let (entity, directory) = fixture.imported(profile, "Imported name");
        let file = data
            .path()
            .join("profiles")
            .join(&directory)
            .join("private-fixture");
        std::fs::write(&file, b"retained local contents").unwrap();
        fixture.listed(vec![fixture.received(profile, 2, Some("Changed remotely"))]);
        assert_eq!(fixture.imported(profile, "Changed remotely").1, directory);
        fixture.listed(vec![fixture.received(profile, 3, None)]);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
        while fixture
            .app
            .world()
            .get::<SyncEnabled>(entity)
            .is_some_and(|enabled| enabled.0)
        {
            fixture.app.update();
            assert!(
                std::time::Instant::now() < deadline,
                "remote deletion did not close transfer"
            );
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert_eq!(fixture.account().1, local_account);
        assert_eq!(std::fs::read(file).unwrap(), b"retained local contents");
        drop(fixture);
        let mut restored = Fixture::at(data.path());
        assert_eq!(restored.account().1, local_account);
        assert_eq!(restored.profile(&directory).0, profile);
        assert_eq!(restored.profile(&directory).1, "Changed remotely");
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
