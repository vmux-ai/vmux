use std::future::Future;
use std::io::Read;
use std::pin::Pin;
use std::task::{Context, Waker};
use std::time::Duration;

use async_io::Timer;
use bevy_app::{App, Update};
use bevy_ecs::prelude::*;
use reqwest::Url;
use vmux_ecs::{
    Job, JobOwner, JobPlugin, JobRequest, JobResult, JobSet, JobWake, Operation, Revision,
};

use crate::runtime::{Lease, Now};
use crate::{Generation, Provider, Receive, RefreshFailed, Set, snapshot::Document};

pub struct HttpPlugin;

impl bevy_app::Plugin for HttpPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<crate::FeaturePlugin>() {
            app.add_plugins(crate::FeaturePlugin);
        }
        if !app.is_plugin_added::<crate::clock::ClockPlugin>() {
            app.add_plugins(crate::clock::ClockPlugin);
        }
        app.add_plugins(JobPlugin::<Fetch>::default())
            .add_systems(
                Update,
                (remove, reset, poll)
                    .chain()
                    .after(Set::Clock)
                    .before(JobSet::Submit),
            )
            .add_systems(Update, receive.after(JobSet::Apply).before(Set::Accept))
            .add_systems(Update, validate.after(Set::Evaluate));
    }
}

#[derive(Component, Clone)]
#[component(immutable)]
#[require(PollState)]
pub struct Endpoint(Url);

impl Endpoint {
    pub fn new(url: &str) -> Result<Self, String> {
        let url = Url::parse(url).map_err(|error| error.to_string())?;
        let local = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
        if (url.scheme() != "https" && !(url.scheme() == "http" && local))
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || url.path() != "/v1/flags/snapshot"
        {
            return Err("feature endpoint must be HTTPS /v1/flags/snapshot without credentials, query or fragment".into());
        }
        Ok(Self(url))
    }
}

#[derive(Component, Default)]
struct PollState {
    next_at: u64,
    pending: Option<Entity>,
    generation: u64,
    failures: u8,
    etag: Option<String>,
}

impl PollState {
    fn delay(&self, entropy: u64) -> u64 {
        if self.failures == 0 {
            return 48 + entropy % 25;
        }
        let base = (5u64 << self.failures.saturating_sub(1).min(6)).min(250);
        let spread = base / 5;
        base - spread + entropy % (spread * 2 + 1)
    }
}

#[derive(Component)]
struct Pending {
    generation: u64,
    conditional: bool,
}

#[derive(Component)]
struct Candidate {
    generation: u64,
    revision: u64,
    issued_at: u64,
    valid_until: u64,
    fingerprint: [u8; 32],
    etag: Option<String>,
    entropy: u64,
}

#[derive(Component)]
struct Deadline {
    at: u64,
    timer: Timer,
}

#[derive(Clone)]
struct Fetch {
    endpoint: Endpoint,
    environment: String,
    etag: Option<String>,
}

impl Job for Fetch {
    type Output = (u64, Result<Response, String>);

    fn run(self) -> Self::Output {
        (Entropy::sample(), self.get())
    }

    fn lane(&self) -> vmux_ecs::blocking::Lane {
        vmux_ecs::blocking::Lane::Long
    }
}

impl Fetch {
    fn get(mut self) -> Result<Response, String> {
        self.endpoint
            .0
            .query_pairs_mut()
            .append_pair("environment", &self.environment);
        let client = reqwest::blocking::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(3))
            .timeout(Duration::from_secs(5))
            .build()
            .map_err(|error| error.to_string())?;
        let mut request = client.get(self.endpoint.0);
        if let Some(etag) = self.etag {
            request = request.header(reqwest::header::IF_NONE_MATCH, etag);
        }
        let response = request.send().map_err(|error| error.to_string())?;
        let status = response.status().as_u16();
        let mut etag = None;
        if let Some(header) = response.headers().get(reqwest::header::ETAG) {
            let header = header.to_str().map_err(|error| error.to_string())?;
            if header.len() > 128 {
                return Err("feature ETag exceeds 128 bytes".into());
            }
            etag = Some(header.to_string());
        }
        let mut bytes = Vec::new();
        response
            .take(65537)
            .read_to_end(&mut bytes)
            .map_err(|error| error.to_string())?;
        if bytes.len() > 65536 {
            return Err("feature response exceeds 64 KiB".into());
        }
        Ok(Response {
            status,
            etag,
            bytes,
        })
    }
}

struct Response {
    status: u16,
    etag: Option<String>,
    bytes: Vec<u8>,
}

struct Entropy;

impl Entropy {
    fn sample() -> u64 {
        let mut bytes = [0u8; 8];
        if getrandom::fill(&mut bytes).is_err() {
            return 0;
        }
        u64::from_be_bytes(bytes)
    }
}

type PollingProviders<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Name,
        &'static Endpoint,
        &'static Generation,
        &'static Lease,
        &'static mut PollState,
        Option<&'static mut Deadline>,
    ),
    With<Provider>,
>;

type FetchResults<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static ChildOf,
        &'static Pending,
        &'static JobOwner<Fetch>,
        &'static mut JobResult<Fetch>,
    ),
>;

type ReceivingProviders<'w, 's> = Query<
    'w,
    's,
    (&'static Generation, &'static Lease, &'static mut PollState),
    (With<Provider>, With<Endpoint>),
>;

fn remove(
    mut providers: Query<(Entity, &PollState, &mut Lease), Without<Endpoint>>,
    mut commands: Commands,
) {
    for (entity, poll, mut lease) in &mut providers {
        if let Some(pending) = poll.pending {
            commands.entity(pending).try_despawn();
        }
        lease.available = false;
        commands
            .entity(entity)
            .remove::<(PollState, Candidate, Deadline)>();
    }
}

type ChangedProviders<'w, 's> = Query<
    'w,
    's,
    (
        &'static mut Generation,
        &'static mut Lease,
        &'static mut Revision,
        Ref<'static, Name>,
    ),
    (
        With<Provider>,
        With<Endpoint>,
        Or<(Changed<Endpoint>, Changed<Name>)>,
    ),
>;

fn reset(mut providers: ChangedProviders) {
    for (mut generation, mut lease, mut revision, environment) in &mut providers {
        generation.0 = generation.0.saturating_add(1);
        lease.available = false;
        if environment.is_changed() {
            revision.0 = 0;
        }
    }
}

fn poll(
    mut providers: PollingProviders,
    now: Res<Now>,
    wake: Option<Res<JobWake>>,
    mut commands: Commands,
) {
    for (provider, environment, endpoint, generation, lease, mut state, deadline) in &mut providers
    {
        if state.generation != generation.0 {
            if let Some(pending) = state.pending.take() {
                commands.entity(pending).try_despawn();
            }
            *state = PollState {
                generation: generation.0,
                ..Default::default()
            };
            commands.entity(provider).remove::<Candidate>();
        }
        if state.pending.is_none() && now.0 >= state.next_at {
            let task = commands
                .spawn((
                    Operation::bundle("feature.fetch").expect("static operation kind"),
                    ChildOf(provider),
                    Pending {
                        generation: generation.0,
                        conditional: state.etag.is_some(),
                    },
                    JobRequest(Fetch {
                        endpoint: endpoint.clone(),
                        environment: environment.as_str().to_string(),
                        etag: state.etag.clone(),
                    }),
                ))
                .id();
            state.pending = Some(task);
        }
        let mut at = state.next_at;
        if state.pending.is_some() {
            at = now.0.saturating_add(5);
        }
        if lease.available {
            at = at.min(lease.valid_until);
        }
        let mut context =
            Context::from_waker(wake.as_deref().map_or(Waker::noop(), |wake| &wake.0));
        let delay = Duration::from_secs(at.saturating_sub(now.0));
        if let Some(mut deadline) = deadline {
            if deadline.at != at {
                deadline.at = at;
                deadline.timer = Timer::after(delay);
            }
            let _ = Pin::new(&mut deadline.timer).poll(&mut context);
        } else {
            let mut timer = Timer::after(delay);
            let _ = Pin::new(&mut timer).poll(&mut context);
            commands.entity(provider).insert(Deadline { at, timer });
        }
    }
}

fn receive(
    mut results: FetchResults,
    mut providers: ReceivingProviders,
    now: Res<Now>,
    mut accepted: MessageWriter<Receive>,
    mut failures: MessageWriter<RefreshFailed>,
    mut commands: Commands,
) {
    for (entity, owner, pending, job_owner, mut result) in &mut results {
        let provider = owner.parent();
        let Ok((generation, lease, mut state)) = providers.get_mut(provider) else {
            commands.entity(entity).despawn();
            continue;
        };
        if !job_owner.accepts(Some(owner))
            || generation.0 != pending.generation
            || state.pending != Some(entity)
        {
            commands.entity(entity).despawn();
            continue;
        }
        let outcome = std::mem::replace(&mut result.0, Err("consumed feature fetch".into()));
        let (entropy, outcome) = match outcome {
            Ok(value) => value,
            Err(error) => (0, Err(error)),
        };
        let mut success = false;
        if let Ok(response) = outcome {
            if response.status == 304 && pending.conditional && lease.available {
                success = true;
            } else if response.status == 200
                && let Ok(document) = Document::decode(&response.bytes)
            {
                commands.entity(provider).insert(Candidate {
                    generation: generation.0,
                    revision: document.revision,
                    issued_at: document.issued_at,
                    valid_until: document.valid_until,
                    fingerprint: document.fingerprint(),
                    etag: response.etag,
                    entropy,
                });
                accepted.write(Receive {
                    provider,
                    generation: generation.0,
                    document,
                });
                success = true;
            }
        }
        state.pending = None;
        if success {
            state.failures = 0;
        } else {
            state.failures = state.failures.saturating_add(1);
            state.etag = None;
            failures.write(RefreshFailed {
                provider,
                generation: generation.0,
            });
        }
        state.next_at = now.0.saturating_add(state.delay(entropy));
        commands.entity(entity).despawn();
    }
}

fn validate(
    mut providers: Query<(
        Entity,
        &Revision,
        &Generation,
        &Lease,
        &Candidate,
        &mut PollState,
    )>,
    now: Res<Now>,
    mut commands: Commands,
) {
    for (provider, revision, generation, lease, candidate, mut state) in &mut providers {
        if lease.available
            && generation.0 == candidate.generation
            && revision.0 == candidate.revision
            && lease.fingerprint == candidate.fingerprint
            && lease.issued_at == candidate.issued_at
            && lease.valid_until == candidate.valid_until
        {
            state.etag = candidate.etag.clone();
        } else {
            state.etag = None;
            state.failures = state.failures.saturating_add(1);
            state.next_at = now.0.saturating_add(state.delay(candidate.entropy));
        }
        commands.entity(provider).remove::<Candidate>();
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::net::{TcpListener, TcpStream};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc::{self, Receiver};
    use std::thread::JoinHandle;
    use std::time::{Instant, SystemTime, UNIX_EPOCH};

    use super::*;
    use crate::{Current, Definition, Tick};

    struct Server {
        url: String,
        requests: Receiver<String>,
        stopped: Arc<AtomicBool>,
        worker: Option<JoinHandle<()>>,
    }

    impl Server {
        fn new(replies: Vec<String>) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let url = format!(
                "http://{}/v1/flags/snapshot",
                listener.local_addr().unwrap()
            );
            listener.set_nonblocking(true).unwrap();
            let stopped = Arc::new(AtomicBool::new(false));
            let stop = stopped.clone();
            let (send, requests) = mpsc::channel();
            let worker = std::thread::spawn(move || {
                for reply in replies {
                    let mut stream = loop {
                        if stop.load(Ordering::Acquire) {
                            return;
                        }
                        match listener.accept() {
                            Ok((stream, _)) => break stream,
                            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                                std::thread::sleep(Duration::from_millis(1))
                            }
                            Err(error) => panic!("HTTP fixture accept failed: {error}"),
                        }
                    };
                    stream.set_nonblocking(false).unwrap();
                    stream
                        .set_read_timeout(Some(Duration::from_secs(2)))
                        .unwrap();
                    let request = Self::request(&mut stream);
                    send.send(request).unwrap();
                    stream.write_all(reply.as_bytes()).unwrap();
                }
            });
            Self {
                url,
                requests,
                stopped,
                worker: Some(worker),
            }
        }

        fn request(stream: &mut TcpStream) -> String {
            let mut bytes = Vec::new();
            while !bytes.ends_with(b"\r\n\r\n") {
                let mut byte = [0u8];
                stream.read_exact(&mut byte).unwrap();
                bytes.push(byte[0]);
                assert!(
                    bytes.len() <= 8192,
                    "HTTP fixture request headers exceed bound"
                );
            }
            String::from_utf8(bytes).unwrap()
        }

        fn reply(status: u16, body: &str, etag: &str) -> String {
            format!(
                "HTTP/1.1 {status} fixture\r\nContent-Length: {}\r\nETag: {etag}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
        }
    }

    impl Drop for Server {
        fn drop(&mut self) {
            self.stopped.store(true, Ordering::Release);
            let outcome = self.worker.take().unwrap().join();
            if !std::thread::panicking() {
                outcome.unwrap();
            }
        }
    }

    struct Fixture {
        app: App,
        provider: Entity,
        flag: Entity,
        server: Server,
    }

    struct Notify(mpsc::SyncSender<()>);

    impl std::task::Wake for Notify {
        fn wake(self: Arc<Self>) {
            let _ = self.0.try_send(());
        }

        fn wake_by_ref(self: &Arc<Self>) {
            let _ = self.0.try_send(());
        }
    }

    impl Fixture {
        fn new(replies: Vec<String>) -> Self {
            let server = Server::new(replies);
            let mut app = App::new();
            app.add_plugins(HttpPlugin);
            let provider = app
                .world_mut()
                .spawn((
                    Provider::bundle("production"),
                    Endpoint::new(&server.url).unwrap(),
                ))
                .id();
            let flag = app
                .world_mut()
                .spawn(Definition::bundle(provider, "account.cloud", false))
                .id();
            Self {
                app,
                provider,
                flag,
                server,
            }
        }

        fn fetch(&mut self) -> String {
            self.app
                .world_mut()
                .get_mut::<PollState>(self.provider)
                .unwrap()
                .next_at = 0;
            let timeout = Instant::now() + Duration::from_secs(5);
            loop {
                self.app.update();
                let state = self.app.world().get::<PollState>(self.provider).unwrap();
                if state.pending.is_none() && state.next_at > 0 {
                    return self
                        .server
                        .requests
                        .recv_timeout(Duration::from_secs(1))
                        .unwrap();
                }
                assert!(
                    Instant::now() < timeout,
                    "production HTTP plugin did not complete its fetch"
                );
                std::thread::sleep(Duration::from_millis(1));
            }
        }

        fn enabled(&self) -> bool {
            self.app.world().get::<Current>(self.flag).unwrap().0
        }

        fn wakes(&mut self) -> Receiver<()> {
            let (send, wake) = mpsc::sync_channel(8);
            self.app
                .insert_resource(JobWake(Waker::from(Arc::new(Notify(send)))));
            wake
        }

        fn document(revision: u64, until: u64) -> String {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();
            serde_json::to_string(&Document {
                schema_version: 1,
                environment: "production".into(),
                revision,
                issued_at: now,
                valid_until: until,
                flags: std::collections::BTreeMap::from([("account.cloud".into(), true)]),
            })
            .unwrap()
        }
    }

    #[test]
    fn conditional_get_keeps_the_existing_expiry_and_a_failed_fetch_closes_admission() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let mut fixture = Fixture::new(vec![
            Server::reply(200, &Fixture::document(1, now + 100), "\"r1\""),
            Server::reply(304, "", "\"r1\""),
            Server::reply(503, "unavailable", "\"r1\""),
        ]);
        let request = fixture.fetch();
        assert!(request.starts_with("GET /v1/flags/snapshot?environment=production HTTP/1.1\r\n"));
        assert!(!request.to_lowercase().contains("if-none-match:"));
        assert!(fixture.enabled());
        let expiry = fixture
            .app
            .world()
            .get::<Lease>(fixture.provider)
            .unwrap()
            .valid_until;
        let request = fixture.fetch();
        assert!(request.to_lowercase().contains("if-none-match: \"r1\"\r\n"));
        assert!(fixture.enabled());
        assert_eq!(
            fixture
                .app
                .world()
                .get::<Lease>(fixture.provider)
                .unwrap()
                .valid_until,
            expiry
        );
        fixture.fetch();
        assert!(!fixture.enabled());
        assert!(
            fixture
                .app
                .world()
                .get::<PollState>(fixture.provider)
                .unwrap()
                .etag
                .is_none()
        );
    }

    #[test]
    fn malformed_response_cannot_install_its_validator_or_replace_the_last_revision() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let mut fixture = Fixture::new(vec![
            Server::reply(200, &Fixture::document(1, now + 100), "\"valid\""),
            Server::reply(200, "{\"schema_version\":999}", "\"invalid\""),
            Server::reply(200, &Fixture::document(2, now + 100), "\"valid2\""),
        ]);
        fixture.fetch();
        assert!(fixture.enabled());
        fixture.fetch();
        assert!(!fixture.enabled());
        assert_eq!(
            fixture
                .app
                .world()
                .get::<Revision>(fixture.provider)
                .unwrap()
                .0,
            1
        );
        let request = fixture.fetch();
        assert!(!request.to_lowercase().contains("if-none-match:"));
        assert!(fixture.enabled());
        assert_eq!(
            fixture
                .app
                .world()
                .get::<Revision>(fixture.provider)
                .unwrap()
                .0,
            2
        );
    }

    #[test]
    fn an_expired_conditional_response_cannot_renew_a_snapshot_lease() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let mut fixture = Fixture::new(vec![
            Server::reply(200, &Fixture::document(1, now + 100), "\"r1\""),
            Server::reply(304, "", "\"r1\""),
        ]);
        fixture.fetch();
        assert!(fixture.enabled());
        fixture.app.world_mut().write_message(Tick(now + 100));
        fixture.app.update();
        assert!(!fixture.enabled());
        fixture.fetch();
        assert!(!fixture.enabled());
    }

    #[test]
    fn endpoint_configuration_refuses_credentials_and_public_cleartext() {
        for url in [
            "http://api.vmux.ai/v1/flags/snapshot",
            "https://user:secret@api.vmux.ai/v1/flags/snapshot",
            "https://api.vmux.ai/other",
            "https://api.vmux.ai/v1/flags/snapshot?environment=other",
        ] {
            assert!(Endpoint::new(url).is_err(), "{url}");
        }
        assert!(Endpoint::new("https://api.vmux.ai/v1/flags/snapshot").is_ok());
    }

    #[test]
    fn lease_expiry_wakes_an_idle_host_before_the_next_network_poll() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let mut fixture = Fixture::new(vec![Server::reply(
            200,
            &Fixture::document(1, now + 2),
            "\"r1\"",
        )]);
        let wake = fixture.wakes();
        fixture.fetch();
        fixture.app.update();
        while wake.try_recv().is_ok() {}
        assert!(fixture.enabled());
        wake.recv_timeout(Duration::from_secs(3))
            .expect("lease timer must wake an idle host");
        fixture.app.update();
        assert!(!fixture.enabled());
    }

    #[test]
    fn replacing_the_provider_endpoint_closes_admission_until_its_own_fetch_completes() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let mut fixture = Fixture::new(vec![Server::reply(
            200,
            &Fixture::document(1, now + 100),
            "\"r1\"",
        )]);
        fixture.fetch();
        assert!(fixture.enabled());
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.provider)
            .insert(Endpoint::new(&fixture.server.url).unwrap());
        fixture.app.update();
        assert!(!fixture.enabled());
    }

    #[test]
    fn environment_change_accepts_its_own_revision_sequence() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let staging = Document {
            schema_version: 1,
            environment: "staging".into(),
            revision: 1,
            issued_at: now,
            valid_until: now + 100,
            flags: std::collections::BTreeMap::from([("account.cloud".into(), false)]),
        };
        let mut fixture = Fixture::new(vec![
            Server::reply(200, &Fixture::document(5, now + 100), "\"prod5\""),
            Server::reply(200, &serde_json::to_string(&staging).unwrap(), "\"stage1\""),
        ]);
        fixture.fetch();
        assert!(fixture.enabled());
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.provider)
            .insert(Name::new("staging"));
        let request = fixture.fetch();
        assert!(request.starts_with("GET /v1/flags/snapshot?environment=staging HTTP/1.1\r\n"));
        assert!(!request.to_lowercase().contains("if-none-match:"));
        assert!(!fixture.enabled());
        assert_eq!(
            fixture
                .app
                .world()
                .get::<Revision>(fixture.provider)
                .unwrap()
                .0,
            1
        );
    }
}
