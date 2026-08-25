use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use tokio::sync::watch;
use vmux_remote::framing::{Frame, FrameStream};
use vmux_remote::quic::endpoint::SelfSignedIdentity;
use vmux_remote::quic::tunnel::TunnelSocket;
use vmux_remote::quic::{Accepted, MessageType, RelaySetup};
use vmux_remote::{DeviceId, PeerRole};

use super::super::server::RemoteState;
use crate::RemotePaths;
use crate::pairing::Relay;

const MIN_INNER_MTU: usize = 1200;

const TUNNEL_OVERHEAD: usize = 64;

pub fn spawn(state: RemoteState, liveness: watch::Receiver<bool>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut backoff = Backoff::new();
        loop {
            let ended = Registration::hold(&state, &liveness).await;
            ended.report();
            tokio::time::sleep(backoff.after(&ended)).await;
        }
    })
}

struct Registration {
    control: quinn::Connection,
    _registered: RegisteredDevice,
    since: Instant,
}

impl Registration {
    async fn hold(state: &RemoteState, liveness: &watch::Receiver<bool>) -> SessionEnd {
        let identity = match super::ensure_identity() {
            Ok(identity) => identity,
            Err(error) => {
                return SessionEnd::Unregistered(format!("desktop identity: {error}"));
            }
        };
        match Self::open(&state.token).await {
            Ok(registration) => registration.serve(state, &identity, liveness).await,
            Err(reason) => SessionEnd::Unregistered(reason),
        }
    }

    async fn open(token: &str) -> Result<Self, String> {
        let relay = Relay::configured();
        let device_id = ensure_device_id().map_err(|error| error.to_string())?;
        let address = resolve(relay.url()).await?;
        let server_name = host_of(relay.url())?;

        let endpoint = vmux_remote::quic::endpoint::Trust::Relay {
            host: server_name.clone(),
        }
        .endpoint(address)
        .map_err(|error| format!("relay client endpoint: {error}"))?;
        let control = endpoint
            .connect(address, &server_name)
            .map_err(|error| format!("relay dial {} at {address}: {error}", relay.url()))?
            .await
            .map_err(|error| format!("relay connect {} at {address}: {error}", relay.url()))?;

        register(&control, &device_id, token).await?;
        let registered = RegisteredDevice::claim(&device_id)
            .map_err(|error| format!("persist relay registration: {error}"))?;
        tracing::info!(device_id = %device_id.as_str(), relay = %relay.url(), "remote quic: registered with the relay");

        Ok(Self {
            control,
            _registered: registered,
            since: Instant::now(),
        })
    }

    async fn serve(
        self,
        state: &RemoteState,
        identity: &SelfSignedIdentity,
        liveness: &watch::Receiver<bool>,
    ) -> SessionEnd {
        let socket = TunnelSocket::new(self.control.clone());
        let Some(budget) = socket.usable_mtu() else {
            return self.ended("the relay refused datagrams, so no packet can be tunnelled");
        };
        if budget < MIN_INNER_MTU + TUNNEL_OVERHEAD {
            return self.ended(format!(
                "the path to the relay carries {budget} bytes, below the {} a QUIC handshake needs",
                MIN_INNER_MTU + TUNNEL_OVERHEAD
            ));
        }
        let inner = match super::inner_endpoint(socket, identity) {
            Ok(inner) => inner,
            Err(error) => return self.ended(error),
        };

        super::accept_loop(inner, state.clone(), liveness.clone(), self.control.clone()).await;

        let reason = format!(
            "control connection closed: {:?}",
            self.control.close_reason()
        );
        self.ended(reason)
    }

    fn ended(self, reason: impl Into<String>) -> SessionEnd {
        SessionEnd::Registered {
            held: self.since.elapsed(),
            reason: reason.into(),
        }
    }
}

enum SessionEnd {
    Unregistered(String),
    Registered { held: Duration, reason: String },
}

impl SessionEnd {
    const STABLE: Duration = Duration::from_secs(60);

    fn proves_the_relay_is_up(&self) -> bool {
        match self {
            Self::Unregistered(_) => false,
            Self::Registered { held, .. } => *held >= Self::STABLE,
        }
    }

    fn report(&self) {
        match self {
            Self::Unregistered(reason) => {
                tracing::warn!(reason = %reason, "remote quic: could not register with the relay");
            }
            Self::Registered { held, reason } => tracing::warn!(
                held_secs = held.as_secs(),
                reason = %reason,
                "remote quic: relay session ended"
            ),
        }
    }
}

struct Backoff {
    delay: Duration,
}

impl Backoff {
    const FIRST: Duration = Duration::from_secs(1);
    const MAX: Duration = Duration::from_secs(30);

    fn new() -> Self {
        Self { delay: Self::FIRST }
    }

    fn after(&mut self, ended: &SessionEnd) -> Duration {
        if ended.proves_the_relay_is_up() {
            self.delay = Self::FIRST;
        }
        let waiting = self.delay;
        self.delay = (self.delay * 2).min(Self::MAX);
        waiting
    }
}

pub(super) struct RegisteredDevice {
    path: PathBuf,
}

impl RegisteredDevice {
    fn claim(device_id: &DeviceId) -> std::io::Result<Self> {
        let path = RemotePaths::current().relay_registration();
        super::super::write_private(&path, device_id.as_str())?;
        Ok(Self { path })
    }

    pub(super) fn release_stale() {
        Self::remove(&RemotePaths::current().relay_registration());
    }

    fn remove(path: &Path) {
        match std::fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => tracing::warn!(
                %error,
                path = %path.display(),
                "remote quic: the relay registration could not be cleared"
            ),
        }
    }
}

impl Drop for RegisteredDevice {
    fn drop(&mut self) {
        Self::remove(&self.path);
    }
}

const SETUP: FrameStream = FrameStream::new(16 * 1024);

async fn register(
    control: &quinn::Connection,
    device_id: &DeviceId,
    token: &str,
) -> Result<(), String> {
    let (mut send, mut recv) = control
        .open_bi()
        .await
        .map_err(|error| format!("relay stream: {error}"))?;
    let setup = RelaySetup {
        device_id: device_id.clone(),
        role: PeerRole::Desktop,
        token: token.to_string(),
    };
    let frame = Frame::json(MessageType::RELAY_SETUP, &setup)
        .map_err(|error| format!("encode setup: {error}"))?;
    SETUP
        .open(&mut send, &frame)
        .await
        .map_err(|error| format!("write setup: {error}"))?;
    send.finish().map_err(|error| format!("finish: {error}"))?;

    SETUP
        .accept(&mut recv)
        .await
        .map_err(|error| format!("read acceptance: {error:?}"))?
        .read_json::<Accepted>(MessageType::RELAY_ACCEPTED)
        .map_err(|error| format!("decode acceptance: {error:?}"))?;
    Ok(())
}

async fn resolve(relay_url: &str) -> Result<std::net::SocketAddr, String> {
    let parsed = url::Url::parse(relay_url).map_err(|error| format!("relay url: {error}"))?;
    let host = parsed.host_str().ok_or("relay url has no host")?;
    let port = parsed.port().unwrap_or(443);
    vmux_remote::quic::endpoint::resolve_preferring_ipv4(host, port).await
}

fn host_of(relay_url: &str) -> Result<String, String> {
    let parsed = url::Url::parse(relay_url).map_err(|error| format!("relay url: {error}"))?;
    Ok(parsed
        .host_str()
        .ok_or("relay url has no host")?
        .to_string())
}

fn ensure_device_id() -> std::io::Result<DeviceId> {
    let path = RemotePaths::current().relay_device();
    if let Ok(existing) = std::fs::read_to_string(&path) {
        let existing = existing.trim();
        if !existing.is_empty() {
            return Ok(DeviceId::new(existing));
        }
    }
    let minted = uuid::Uuid::new_v4().simple().to_string();
    super::super::write_private(&path, &minted)?;
    Ok(DeviceId::new(minted))
}

#[cfg(test)]
mod tests {
    use super::*;

    impl SessionEnd {
        fn never_connected() -> Self {
            Self::Unregistered("relay connect https://localhost:8788 at [::1]:8788".to_string())
        }

        fn held_for(held: Duration) -> Self {
            Self::Registered {
                held,
                reason: "control connection closed".to_string(),
            }
        }
    }

    impl Backoff {
        fn delays(ended: impl Fn() -> SessionEnd, rounds: usize) -> Vec<u64> {
            let mut backoff = Self::new();
            let mut seconds = Vec::new();
            for _ in 0..rounds {
                seconds.push(backoff.after(&ended()).as_secs());
            }
            seconds
        }
    }

    #[test]
    fn a_relay_that_is_never_reached_widens_up_to_the_cap() {
        assert_eq!(
            Backoff::delays(SessionEnd::never_connected, 8),
            [1, 2, 4, 8, 16, 30, 30, 30]
        );
    }

    #[test]
    fn a_session_that_stood_restarts_the_sequence() {
        let mut backoff = Backoff::new();
        for _ in 0..6 {
            backoff.after(&SessionEnd::never_connected());
        }

        assert_eq!(
            backoff.after(&SessionEnd::held_for(Duration::from_secs(12 * 60))),
            Duration::from_secs(1)
        );
    }

    #[test]
    fn a_registration_that_flaps_keeps_backing_off() {
        let flapping = || SessionEnd::held_for(Duration::from_millis(200));

        assert_eq!(Backoff::delays(flapping, 4), [1, 2, 4, 8]);
    }

    impl RegisteredDevice {
        fn recorded_at(path: &Path) -> Self {
            std::fs::write(path, "device-1").expect("write registration");
            Self {
                path: path.to_path_buf(),
            }
        }
    }

    #[test]
    fn the_recorded_registration_does_not_outlive_its_session() {
        let directory = tempfile::tempdir().expect("temp dir");
        let path = directory.path().join("remote-relay-registration");
        let claimed = RegisteredDevice::recorded_at(&path);
        assert!(path.exists());

        drop(claimed);

        assert!(
            !path.exists(),
            "the registration outlived the session that held it"
        );
    }

    #[tokio::test]
    async fn aborting_the_dialer_withdraws_the_registration() {
        let directory = tempfile::tempdir().expect("temp dir");
        let path = directory.path().join("remote-relay-registration");

        let claimed = path.clone();
        let (holding, held) = tokio::sync::oneshot::channel();
        let dialer = tokio::spawn(async move {
            let _registered = RegisteredDevice::recorded_at(&claimed);
            let _ = holding.send(());
            std::future::pending::<()>().await;
        });
        held.await.expect("the dialer registered");
        assert!(path.exists());

        dialer.abort();
        let _ = dialer.await;

        assert!(
            !path.exists(),
            "an aborted dialer left the relay port recorded"
        );
    }
}
