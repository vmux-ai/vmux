use std::sync::Arc;
use std::task::Waker;
use std::time::{Duration, Instant};

use bevy_app::{App, Update};
use bevy_ecs::prelude::*;
use vmux_ecs::blocking::Lane;
use vmux_ecs::{
    BlockingCancellation, Job, JobOwner, JobPlugin, JobRequest, JobResult, JobSet, JobWake,
};
use vmux_transport::framing::{Frame, FrameStream};
use vmux_transport::quic::MessageType;
use vmux_transport::quic::endpoint::Trust;

use crate::connection::{
    Authorization, Authorized, Challenge, Grant, OpenRequest, PollReady, PollRequest, Registered,
    Rejected, RemoteAccount, SendRequest, SessionToken, Slowed, TransportReady, Verified, Waiting,
};
use crate::wire::{Request, Response};
use crate::{Connection, Failure, Server, Set};

pub(crate) struct Plugin;

impl bevy_app::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            JobPlugin::<Open>::default(),
            JobPlugin::<RoundTrip>::default(),
            JobPlugin::<Delay>::default(),
        ))
        .add_systems(Update, (open, send, delay).in_set(Set::Dispatch))
        .add_systems(
            Update,
            (
                receive_open,
                (receive, translate).chain(),
                receive_delay,
                closed,
            )
                .in_set(JobSet::Apply),
        );
    }
}

#[derive(Component)]
struct Transport {
    runtime: Arc<tokio::runtime::Runtime>,
    endpoint: quinn::Endpoint,
    connection: quinn::Connection,
    watch: tokio::task::JoinHandle<()>,
}

impl Drop for Transport {
    fn drop(&mut self) {
        self.watch.abort();
        self.connection
            .close(0u32.into(), b"account connection closed");
        self.endpoint
            .close(0u32.into(), b"account connection closed");
    }
}

#[derive(Clone)]
struct Open {
    server: Server,
    wake: Option<Waker>,
}

impl Job for Open {
    type Output = Result<Transport, String>;

    fn run(self) -> Self::Output {
        let runtime = Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(1)
                .enable_all()
                .build()
                .map_err(|error| error.to_string())?,
        );
        let opened = runtime.block_on(async {
            let canceled = Cancellation::wait();
            tokio::select! {
                _ = canceled => Err("account connection canceled".to_string()),
                result = tokio::time::timeout(Duration::from_secs(10), async {
                    let address = vmux_transport::quic::endpoint::resolve_preferring_ipv4(&self.server.host, self.server.port).await?;
                    let endpoint = Trust::Relay.endpoint(address)?;
                    let connection = endpoint.connect(address, &self.server.host).map_err(|error| error.to_string())?.await.map_err(|error| error.to_string())?;
                    Ok::<_, String>((endpoint, connection))
                }) => result.map_err(|_| "account connection timed out".to_string())?,
            }
        })?;
        let (endpoint, connection) = opened;
        let watched = connection.clone();
        let watch = runtime.spawn(async move {
            watched.closed().await;
            if let Some(wake) = self.wake {
                wake.wake();
            }
        });
        Ok(Transport {
            runtime,
            endpoint,
            connection,
            watch,
        })
    }
}

#[derive(Clone)]
struct RoundTrip {
    runtime: Arc<tokio::runtime::Runtime>,
    connection: quinn::Connection,
    request: Request,
}

impl Job for RoundTrip {
    type Output = Result<Response, String>;

    fn run(self) -> Self::Output {
        self.runtime.block_on(async {
            let canceled = Cancellation::wait();
            tokio::select! {
                _ = canceled => Err("account request canceled".to_string()),
                result = tokio::time::timeout(Duration::from_secs(10), async {
                    let (mut send, mut receive) = self.connection.open_bi().await.map_err(|error| error.to_string())?;
                    let frames = FrameStream::new(16 * 1024);
                    let frame = Frame::json(MessageType(0x0300), &self.request).map_err(|error| error.to_string())?;
                    if frame.body.len() > 16 * 1024 { return Err("account request exceeds frame limit".into()); }
                    frames.open(&mut send, &frame).await.map_err(|error| error.to_string())?;
                    send.finish().map_err(|error| error.to_string())?;
                    let frame = frames.accept(&mut receive).await.map_err(|error| format!("invalid account frame: {error:?}"))?;
                    let response: Response = frame.read_json(MessageType(0x0301)).map_err(|_| "invalid account response")?;
                    if !self.request.accepts(&response) { return Err("unexpected account response".into()); }
                    Ok(response)
                }) => result.map_err(|_| "account request timed out".to_string())?,
            }
        })
    }
}

struct Cancellation;

impl Cancellation {
    async fn wait() {
        loop {
            if BlockingCancellation::canceled() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
}

#[derive(Clone)]
struct Delay(u64);

impl Job for Delay {
    type Output = Result<(), String>;

    fn run(self) -> Self::Output {
        let end = Instant::now() + Duration::from_secs(self.0.min(60));
        while Instant::now() < end {
            if BlockingCancellation::canceled() {
                return Err("account polling canceled".into());
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        Ok(())
    }

    fn lane(&self) -> Lane {
        Lane::Long
    }
}

fn open(
    mut requests: MessageReader<OpenRequest>,
    active: Query<(), With<Connection>>,
    wake: Option<Res<JobWake>>,
    mut commands: Commands,
) {
    for request in requests.read() {
        if !active.contains(request.entity) {
            continue;
        }
        commands.entity(request.entity).insert(JobRequest(Open {
            server: request.server.clone(),
            wake: wake.as_ref().map(|wake| wake.0.clone()),
        }));
        if let Some(wake) = &wake {
            wake.0.wake_by_ref();
        }
    }
}

fn send(
    mut requests: MessageReader<SendRequest>,
    peers: Query<&Transport, With<Connection>>,
    wake: Option<Res<JobWake>>,
    mut commands: Commands,
) {
    for request in requests.read() {
        let Ok(peer) = peers.get(request.entity) else {
            continue;
        };
        commands
            .entity(request.entity)
            .insert(JobRequest(RoundTrip {
                runtime: peer.runtime.clone(),
                connection: peer.connection.clone(),
                request: request.request.clone(),
            }));
        if let Some(wake) = &wake {
            wake.0.wake_by_ref();
        }
    }
}

fn delay(
    mut requests: MessageReader<PollRequest>,
    active: Query<(), With<Connection>>,
    wake: Option<Res<JobWake>>,
    mut commands: Commands,
) {
    for request in requests.read() {
        if active.contains(request.entity) {
            commands
                .entity(request.entity)
                .insert(JobRequest(Delay(request.seconds)));
            if let Some(wake) = &wake {
                wake.0.wake_by_ref();
            }
        }
    }
}

fn receive_open(mut results: Results<Open>, mut commands: Commands) {
    for (entity, mut result, owner, parent) in &mut results {
        if !owner.accepts(parent) {
            continue;
        }
        let value = std::mem::replace(&mut result.0, Err("connection result consumed".into()));
        match value {
            Ok(Ok(peer)) => {
                commands.entity(entity).insert((peer, TransportReady));
            }
            Ok(Err(error)) | Err(error) => {
                commands
                    .entity(entity)
                    .insert(Rejected(Failure::Storage(error)));
            }
        }
        commands
            .entity(entity)
            .remove::<JobResult<Open>>()
            .remove::<JobRequest<Open>>();
    }
}

#[derive(Component)]
struct Reply(Option<Result<Response, String>>);

fn receive(mut results: Results<RoundTrip>, mut commands: Commands) {
    for (entity, mut result, owner, parent) in &mut results {
        if !owner.accepts(parent) {
            continue;
        }
        let value = std::mem::replace(&mut result.0, Err("request result consumed".into()));
        let response = match value {
            Ok(response) => response,
            Err(error) => Err(error),
        };
        commands
            .entity(entity)
            .insert(Reply(Some(response)))
            .remove::<JobResult<RoundTrip>>()
            .remove::<JobRequest<RoundTrip>>();
    }
}

fn translate(mut replies: Query<(Entity, &mut Reply), With<Connection>>, mut commands: Commands) {
    for (entity, mut reply) in &mut replies {
        let Some(value) = reply.0.take() else {
            continue;
        };
        let response = match value {
            Ok(response) => response,
            Err(error) => {
                commands
                    .entity(entity)
                    .insert(Rejected(Failure::Storage(error)))
                    .remove::<Reply>();
                continue;
            }
        };
        let mut target = commands.entity(entity);
        match response {
            Response::DeviceAuthorization {
                device_code,
                user_code,
                verification_uri,
                expires_in_seconds,
                interval_seconds,
            } => {
                if expires_in_seconds == 0 || expires_in_seconds > 900 {
                    target.insert(Rejected(Failure::InvalidStore));
                } else {
                    target.insert((
                        Authorized,
                        Authorization {
                            user_code,
                            verification_uri,
                        },
                        Grant {
                            code: device_code,
                            expires: Instant::now() + Duration::from_secs(expires_in_seconds),
                            interval: interval_seconds,
                        },
                    ));
                }
            }
            Response::AuthorizationPending => {
                target.insert(Waiting);
            }
            Response::SlowDown => {
                target.insert(Slowed);
            }
            Response::ExpiredToken => {
                target.insert(Rejected(Failure::AuthorizationExpired));
            }
            Response::AccessDenied => {
                target.insert(Rejected(Failure::AccessDenied));
            }
            Response::SignedIn { session_token } => {
                target.insert(SessionToken(session_token));
            }
            Response::DeviceRegistered => {
                target.insert(Registered);
            }
            Response::Challenge { nonce } => {
                target.insert(Challenge(nonce));
            }
            Response::Authenticated => {
                target.insert(Verified);
            }
            Response::NotAuthenticated => {
                target.insert(Rejected(Failure::NotAuthenticated));
            }
            Response::Account { id } => {
                target.insert(RemoteAccount(id));
            }
            Response::RegistrationRefused => {
                target.insert(Rejected(Failure::AccountMismatch));
            }
            Response::Busy | Response::Failed | Response::Malformed => {
                target.insert(Rejected(Failure::Storage("account request refused".into())));
            }
        }
        target.remove::<Reply>();
    }
}

fn receive_delay(mut results: Results<Delay>, mut commands: Commands) {
    for (entity, mut result, owner, parent) in &mut results {
        if !owner.accepts(parent) {
            continue;
        }
        let value = std::mem::replace(&mut result.0, Err("poll result consumed".into()));
        match value {
            Ok(Ok(())) => {
                commands.entity(entity).insert(PollReady);
            }
            Ok(Err(error)) | Err(error) => {
                commands
                    .entity(entity)
                    .insert(Rejected(Failure::Storage(error)));
            }
        }
        commands
            .entity(entity)
            .remove::<JobResult<Delay>>()
            .remove::<JobRequest<Delay>>();
    }
}

fn closed(peers: Query<(Entity, &Transport), With<Connection>>, mut commands: Commands) {
    for (entity, peer) in &peers {
        if peer.connection.close_reason().is_some() {
            commands.entity(entity).insert(Rejected(Failure::Storage(
                "account connection closed".into(),
            )));
        }
    }
}

type Results<'w, 's, J> = Query<
    'w,
    's,
    (
        Entity,
        &'static mut JobResult<J>,
        &'static JobOwner<J>,
        Option<&'static ChildOf>,
    ),
>;
