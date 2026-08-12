//! Registering this desktop with the relay, and serving the phones it sends back.
//!
//! The desktop is behind NAT, so it dials out and holds the connection open rather than waiting
//! to be reached. The relay allocates it a UDP port, tells phones to use it through the pairing
//! link, and tunnels their packets back over this same connection as DATAGRAM frames.
//!
//! Those packets belong to a QUIC session that terminates here, not at the relay. The inner
//! endpoint below is what terminates it — same certificate, same `admit()`, same dispatch as a
//! phone dialling us directly would have reached.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use tokio::sync::watch;
use vmux_remote::quic::endpoint::SelfSignedIdentity;
use vmux_remote::quic::tunnel::TunnelSocket;
use vmux_remote::quic::{RelayAllocation, RelayHello, decode_hello, encode_hello};
use vmux_remote::{DeviceId, PeerRole};

use super::super::server::RemoteState;
use crate::RemotePaths;
use crate::pairing::Relay;

/// Smallest inner packet size QUIC allows. A tunnel that cannot carry this cannot carry a
/// handshake, so coming up would only produce an unreachable desktop.
const MIN_INNER_MTU: usize = 1200;

/// Headroom for the inner endpoint's own framing inside an outer DATAGRAM frame.
const TUNNEL_OVERHEAD: usize = 64;

/// Dial the relay, and keep dialling it for as long as this task is allowed to run.
///
/// Nothing here decides whether Remote should be reachable — [`super::Supervisor`] does, by
/// running this task or not. Aborting it is the way out, which every guard below is written to
/// survive.
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

/// One registration with the relay: the control connection this desktop dialled out on, and the
/// port the relay allocated behind it.
struct Registration {
    control: quinn::Connection,
    port: AllocatedPort,
    since: Instant,
}

impl Registration {
    /// Dial, register, serve every phone the relay tunnels back, and say how it ended.
    ///
    /// The certificate is loaded per attempt rather than once for the process, so a desktop with
    /// Remote off writes none at all, and a failure to write one is reported and retried through
    /// the same backoff as every other reason a session never got started.
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

    /// Dial the relay's control port and claim the port it hands back.
    ///
    /// Every failure here names the address that was tried. A relay URL left over from an older
    /// build points at a port nothing listens on, and over UDP that is indistinguishable from a
    /// slow network — without the address in the message the only symptom is a bare timeout.
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

        let allocated = register(&control, &device_id, token).await?;
        let port = AllocatedPort::claim(allocated)
            .map_err(|error| format!("persist relay port: {error}"))?;
        tracing::info!(port = allocated, relay = %relay.url(), "remote quic: registered with the relay");

        Ok(Self {
            control,
            port,
            since: Instant::now(),
        })
    }

    /// Serve tunnelled phones until the control connection drops.
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

    /// End the session, which gives up the allocated port along with it.
    fn ended(self, reason: impl Into<String>) -> SessionEnd {
        SessionEnd::Registered {
            port: self.port.number(),
            held: self.since.elapsed(),
            reason: reason.into(),
        }
    }
}

/// How a relay session ended, and how far it got.
///
/// A session always ends in a failure — the control connection closing is the ordinary way out —
/// so the useful distinction is not success against error but whether it ever registered.
enum SessionEnd {
    /// No registration was established: name resolution, the dial, the handshake or the hello
    /// failed, so the relay is holding no port for this desktop.
    Unregistered(String),
    /// The relay allocated `port`, and the registration stood for `held` before it ended.
    Registered {
        port: u16,
        held: Duration,
        reason: String,
    },
}

impl SessionEnd {
    /// How long a registration has to stand before it counts as evidence the relay is healthy.
    ///
    /// Registering is not enough on its own: a relay being redeployed can accept a registration
    /// and tear it down moments later, and treating that as success would put every desktop back
    /// to a dial per second — the stampede [`Backoff`] exists to prevent.
    const STABLE: Duration = Duration::from_secs(60);

    /// Whether the next dial can go straight out rather than backing off further.
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
            Self::Registered { port, held, reason } => tracing::warn!(
                port,
                held_secs = held.as_secs(),
                reason = %reason,
                "remote quic: relay session ended"
            ),
        }
    }
}

/// How long to wait before dialling the relay again.
///
/// Doubling rather than a flat delay: a relay that is down stays down for minutes, and the old
/// HTTP client's fixed two seconds meant a redeploy took a dial per instance per two seconds from
/// every desktop at once.
struct Backoff {
    delay: Duration,
}

impl Backoff {
    const FIRST: Duration = Duration::from_secs(1);
    const MAX: Duration = Duration::from_secs(30);

    fn new() -> Self {
        Self { delay: Self::FIRST }
    }

    /// How long to wait after `ended`, folding it into the sequence.
    ///
    /// A session that stood long enough to prove the relay is up starts the sequence over, so a
    /// desktop that was connected for hours reconnects in a second instead of inheriting the cap
    /// from however many attempts it took to get connected in the first place.
    fn after(&mut self, ended: &SessionEnd) -> Duration {
        if ended.proves_the_relay_is_up() {
            self.delay = Self::FIRST;
        }
        let waiting = self.delay;
        self.delay = (self.delay * 2).min(Self::MAX);
        waiting
    }
}

/// The port the relay allocated this desktop, recorded for as long as the registration holds it.
///
/// The pairing link is built from this file and has no other way to learn the port. The relay
/// frees the port the moment the registration ends, so a file that outlives its session points
/// phones at nothing — which a phone can only discover by waiting out a timeout. Removing it on
/// drop is also what stops the app offering a link with no session behind it: every reader
/// already treats a missing port as "not registered yet".
///
/// Turning Remote off aborts the dialer, which drops this along with everything else the task
/// held, so the switch gives the port back without anything having to remember to.
pub(super) struct AllocatedPort {
    number: u16,
    path: PathBuf,
}

impl AllocatedPort {
    /// Record `number` as this desktop's, replacing whatever the last session left behind.
    fn claim(number: u16) -> std::io::Result<Self> {
        let path = RemotePaths::current().relay_port();
        super::super::write_private(&path, &number.to_string())?;
        Ok(Self { number, path })
    }

    /// Forget a port recorded by a previous process.
    ///
    /// [`Drop`] covers a session this process owned; a daemon that was killed never got to run it,
    /// so the file is cleared once at startup before anything can read it as live. That has to
    /// happen whether or not Remote is on, since the stale port is readable either way.
    pub(super) fn release_stale() {
        Self::remove(&RemotePaths::current().relay_port());
    }

    fn number(&self) -> u16 {
        self.number
    }

    fn remove(path: &Path) {
        match std::fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => tracing::warn!(
                %error,
                path = %path.display(),
                "remote quic: the allocated relay port could not be cleared"
            ),
        }
    }
}

impl Drop for AllocatedPort {
    fn drop(&mut self) {
        Self::remove(&self.path);
    }
}

/// Send the hello and read back the port phones should dial.
async fn register(
    control: &quinn::Connection,
    device_id: &DeviceId,
    token: &str,
) -> Result<u16, String> {
    let (mut send, mut recv) = control
        .open_bi()
        .await
        .map_err(|error| format!("relay stream: {error}"))?;
    let hello = RelayHello {
        device_id: device_id.clone(),
        role: PeerRole::Desktop,
        token: token.to_string(),
    };
    let bytes = encode_hello(&hello).map_err(|error| format!("encode hello: {error}"))?;
    send.write_all(&bytes)
        .await
        .map_err(|error| format!("write hello: {error}"))?;
    send.finish().map_err(|error| format!("finish: {error}"))?;

    let answer = recv
        .read_to_end(16 * 1024)
        .await
        .map_err(|error| format!("read allocation: {error}"))?;
    let (allocation, _) = decode_hello::<RelayAllocation>(&answer)
        .map_err(|error| format!("decode allocation: {error:?}"))?;
    Ok(allocation.port)
}

/// The relay's QUIC control port, resolved.
///
/// The URL names the HTTPS endpoint; QUIC listens on the same host and port number over UDP.
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

/// This desktop's identity to the relay, minted once and kept.
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
                port: 41003,
                held,
                reason: "control connection closed".to_string(),
            }
        }
    }

    impl Backoff {
        /// The delays a fresh backoff produces over `rounds` sessions that all end the same way.
        fn delays(ended: impl Fn() -> SessionEnd, rounds: usize) -> Vec<u64> {
            let mut backoff = Self::new();
            let mut seconds = Vec::new();
            for _ in 0..rounds {
                seconds.push(backoff.after(&ended()).as_secs());
            }
            seconds
        }
    }

    /// A relay that cannot be reached at all has to be dialled less and less often, or every
    /// desktop in the fleet hammers it in lockstep while it is down.
    #[test]
    fn a_relay_that_is_never_reached_widens_up_to_the_cap() {
        assert_eq!(
            Backoff::delays(SessionEnd::never_connected, 8),
            [1, 2, 4, 8, 16, 30, 30, 30]
        );
    }

    /// The bug this guards: a session is only ever reported as an error, so nothing used to reset
    /// the delay and a registration that stood for twelve minutes still came back capped.
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

    /// Registering is not proof on its own. A relay mid-redeploy accepts a registration and drops
    /// it moments later, so resetting on that alone would restore the per-second stampede.
    #[test]
    fn a_registration_that_flaps_keeps_backing_off() {
        let flapping = || SessionEnd::held_for(Duration::from_millis(200));

        assert_eq!(Backoff::delays(flapping, 4), [1, 2, 4, 8]);
    }

    impl AllocatedPort {
        /// A recorded port under `path`, so a test never writes into the user's profile.
        fn recorded_at(path: &Path) -> Self {
            std::fs::write(path, "41003").expect("write port");
            Self {
                number: 41003,
                path: path.to_path_buf(),
            }
        }
    }

    /// The relay frees the port when the registration ends, so the recorded one must not outlive
    /// it — a pairing link built from a stale port sends the phone somewhere nothing answers.
    #[test]
    fn the_recorded_port_does_not_outlive_its_registration() {
        let directory = tempfile::tempdir().expect("temp dir");
        let path = directory.path().join("remote-relay-port");
        let claimed = AllocatedPort::recorded_at(&path);
        assert!(path.exists());

        drop(claimed);

        assert!(
            !path.exists(),
            "the allocated port outlived the session that held it"
        );
    }

    /// Switching Remote off takes the dialer down by aborting it, which is not a path a guard runs
    /// on by default anywhere — a future that is never polled again is simply dropped. If that
    /// drop did not reach here, disabling Remote would leave the relay holding a port for a
    /// desktop that no longer answers.
    #[tokio::test]
    async fn aborting_the_dialer_gives_the_port_back() {
        let directory = tempfile::tempdir().expect("temp dir");
        let path = directory.path().join("remote-relay-port");

        let claimed = path.clone();
        let (holding, held) = tokio::sync::oneshot::channel();
        let dialer = tokio::spawn(async move {
            let _port = AllocatedPort::recorded_at(&claimed);
            let _ = holding.send(());
            std::future::pending::<()>().await;
        });
        held.await.expect("the dialer claimed a port");
        assert!(path.exists());

        dialer.abort();
        let _ = dialer.await;

        assert!(
            !path.exists(),
            "an aborted dialer left the relay port recorded"
        );
    }
}
