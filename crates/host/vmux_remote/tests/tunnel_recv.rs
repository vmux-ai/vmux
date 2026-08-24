//! Proves a dead tunnel reports itself in a way quinn acts on.
//!
//! quinn ignores `ConnectionReset` from `poll_recv` — it is injectable by an attacker, so the
//! endpoint driver retries the read, and the retry skips the loop's own work limiter. A closed
//! control connection is permanent, so reporting that kind spins a core until the process dies
//! rather than retiring the endpoint. Only a live socket shows this: the error kind is chosen in
//! one place and its consequence lives in a dependency.

use std::io;
use std::net::{Ipv4Addr, SocketAddr};
use std::time::Duration;

use quinn::AsyncUdpSocket;
use quinn::udp::RecvMeta;
use vmux_remote::quic::endpoint::{SelfSignedIdentity, Trust};
use vmux_remote::quic::tunnel::TunnelSocket;

fn relay() -> (SelfSignedIdentity, SocketAddr, quinn::Endpoint) {
    let identity = SelfSignedIdentity::generate(vec!["localhost".into(), "127.0.0.1".into()])
        .expect("generate identity");
    let endpoint = identity
        .listen((Ipv4Addr::LOCALHOST, 0).into())
        .expect("bind server");
    let address = endpoint.local_addr().expect("local addr");
    (identity, address, endpoint)
}

#[tokio::test]
async fn a_closed_control_connection_reports_an_error_quinn_will_not_swallow() {
    let (identity, address, server) = relay();
    let accepting = tokio::spawn(async move {
        match tokio::time::timeout(Duration::from_secs(5), server.accept()).await {
            Ok(Some(incoming)) => incoming.await.is_ok(),
            _ => false,
        }
    });

    let client = Trust::Desktop {
        fingerprint: identity.fingerprint.clone(),
    }
    .endpoint(address)
    .expect("bind client");
    let control = tokio::time::timeout(
        Duration::from_secs(5),
        client.connect(address, "localhost").expect("dial"),
    )
    .await
    .expect("client did not settle")
    .expect("handshake should succeed against the paired certificate");
    assert!(accepting.await.expect("accept task"));

    let socket = TunnelSocket::new(control.clone());
    control.close(0u32.into(), b"relay went away");

    let mut buffer = [0u8; 1500];
    let error = tokio::time::timeout(
        Duration::from_secs(5),
        std::future::poll_fn(|cx| {
            let mut bufs = [io::IoSliceMut::new(&mut buffer)];
            let mut meta = [RecvMeta::default()];
            socket.poll_recv(cx, &mut bufs, &mut meta)
        }),
    )
    .await
    .expect("poll_recv never settled on a closed control connection")
    .expect_err("a closed control connection has no datagram to report");

    assert_ne!(
        error.kind(),
        io::ErrorKind::ConnectionReset,
        "quinn retries this kind without yielding, so the endpoint driver spins instead of retiring"
    );
}
