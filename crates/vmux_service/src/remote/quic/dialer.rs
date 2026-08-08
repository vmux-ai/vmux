//! Registering this desktop with the relay, and serving the phones it sends back.
//!
//! The desktop is behind NAT, so it dials out and holds the connection open rather than waiting
//! to be reached. The relay allocates it a UDP port, tells phones to use it through the pairing
//! link, and tunnels their packets back over this same connection as DATAGRAM frames.
//!
//! Those packets belong to a QUIC session that terminates here, not at the relay. The inner
//! endpoint below is what terminates it — same certificate, same `admit()`, same dispatch as a
//! phone dialling us directly would have reached.

use std::time::Duration;

use tokio::sync::watch;
use vmux_remote::quic::endpoint::SelfSignedIdentity;
use vmux_remote::quic::tunnel::TunnelSocket;
use vmux_remote::quic::{ProtocolVersion, RelayAllocation, RelayHello, decode_hello, encode_hello};
use vmux_remote::{DeviceId, PeerRole};

use super::super::server::RemoteState;

/// Smallest inner packet size QUIC allows. A tunnel that cannot carry this cannot carry a
/// handshake, so coming up would only produce an unreachable desktop.
const MIN_INNER_MTU: usize = 1200;

/// Headroom for the inner endpoint's own framing inside an outer DATAGRAM frame.
const TUNNEL_OVERHEAD: usize = 64;

const FIRST_RETRY: Duration = Duration::from_secs(1);
const MAX_RETRY: Duration = Duration::from_secs(30);

/// Dial the relay, and keep dialling it.
pub fn spawn(
    state: RemoteState,
    identity: SelfSignedIdentity,
    liveness: watch::Receiver<bool>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut backoff = FIRST_RETRY;
        loop {
            match session(&state, &identity, &liveness).await {
                Ok(()) => backoff = FIRST_RETRY,
                Err(error) => {
                    tracing::warn!(%error, "remote quic: relay session ended");
                }
            }
            tokio::time::sleep(backoff).await;
            // Doubling rather than a flat delay: a relay that is down stays down for minutes, and
            // the old HTTP client's fixed two seconds meant a redeploy took a dial per instance
            // per two seconds from every desktop at once.
            backoff = (backoff * 2).min(MAX_RETRY);
        }
    })
}

/// One registration, held until the control connection drops.
async fn session(
    state: &RemoteState,
    identity: &SelfSignedIdentity,
    liveness: &watch::Receiver<bool>,
) -> Result<(), String> {
    let relay_url = configured_relay_url();
    let device_id = ensure_device_id().map_err(|error| error.to_string())?;
    let address = resolve(&relay_url).await?;

    let server_name = host_of(&relay_url)?;
    let endpoint = vmux_remote::quic::endpoint::client_endpoint_relay(&server_name)
        .map_err(|error| format!("relay client endpoint: {error}"))?;
    let control = endpoint
        .connect(address, &server_name)
        .map_err(|error| format!("relay dial: {error}"))?
        .await
        .map_err(|error| format!("relay connect: {error}"))?;

    let port = register(&control, &device_id, &state.token).await?;
    persist_port(port).map_err(|error| format!("persist relay port: {error}"))?;
    tracing::info!(port, relay = %relay_url, "remote quic: registered with the relay");

    let socket = TunnelSocket::new(control.clone());
    let budget = socket
        .usable_mtu()
        .ok_or_else(|| "the relay refused datagrams, so no packet can be tunnelled".to_string())?;
    if budget < MIN_INNER_MTU + TUNNEL_OVERHEAD {
        return Err(format!(
            "the path to the relay carries {budget} bytes, below the {} a QUIC handshake needs",
            MIN_INNER_MTU + TUNNEL_OVERHEAD
        ));
    }

    let inner = super::inner_endpoint(socket, identity)?;
    super::accept_loop(inner, state.clone(), liveness.clone(), control.clone()).await;

    Err(format!(
        "control connection closed: {:?}",
        control.close_reason()
    ))
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
        protocol_version: ProtocolVersion::CURRENT,
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

/// Which relay to dial.
///
/// The file is the normal source, not the fallback: launchd starts this daemon, so it inherits
/// nothing from the shell that launched the app, and the environment is only ever set in a
/// developer's terminal. The app writes what it resolved to disk for exactly this reason.
fn configured_relay_url() -> String {
    if let Ok(from_env) = std::env::var("VMUX_REMOTE_RELAY_URL")
        && let Some(url) = crate::normalize_relay_url(&from_env)
    {
        return url;
    }
    match std::fs::read_to_string(crate::remote_relay_url_path()) {
        Ok(persisted) => crate::resolve_relay_url(Some(&persisted)),
        Err(_) => crate::resolve_relay_url(None),
    }
}

/// The relay's QUIC control port, resolved.
///
/// The URL names the HTTPS endpoint; QUIC listens on the same host and port number over UDP.
async fn resolve(relay_url: &str) -> Result<std::net::SocketAddr, String> {
    let parsed = url::Url::parse(relay_url).map_err(|error| format!("relay url: {error}"))?;
    let host = parsed.host_str().ok_or("relay url has no host")?;
    let port = parsed.port().unwrap_or(443);
    tokio::net::lookup_host((host, port))
        .await
        .map_err(|error| format!("resolve {host}: {error}"))?
        .next()
        .ok_or_else(|| format!("{host} resolved to nothing"))
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
    let path = crate::remote_relay_device_path();
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

fn persist_port(port: u16) -> std::io::Result<()> {
    super::super::write_private(&crate::remote_relay_port_path(), &port.to_string())
}
