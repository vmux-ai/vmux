//! Proves the pinning actually holds over a real handshake.
//!
//! This is the control that replaces `danger_accept_invalid_certs(true)`, which accepted any
//! certificate at all on a private address. A unit test on the comparison function would not
//! catch the failure that matters — a verifier wired up so it never runs, or a rustls config that
//! quietly falls back to the platform roots. So both directions go through a live QUIC endpoint.

#![cfg(not(target_arch = "wasm32"))]

use std::net::{Ipv4Addr, SocketAddr};
use std::time::Duration;

use vmux_remote::quic::endpoint::{
    SelfSignedIdentity, client_endpoint, generate_self_signed, server_endpoint,
};

fn desktop() -> (SelfSignedIdentity, SocketAddr, quinn::Endpoint) {
    let identity = generate_self_signed(vec!["localhost".into(), "127.0.0.1".into()])
        .expect("generate identity");
    let endpoint = server_endpoint(
        (Ipv4Addr::LOCALHOST, 0).into(),
        &identity.certificate_pem,
        &identity.private_key_pem,
    )
    .expect("bind server");
    let address = endpoint.local_addr().expect("local addr");
    (identity, address, endpoint)
}

/// Accept one connection and report whether the handshake completed.
async fn accept_once(endpoint: quinn::Endpoint) -> bool {
    match tokio::time::timeout(Duration::from_secs(5), endpoint.accept()).await {
        Ok(Some(incoming)) => incoming.await.is_ok(),
        _ => false,
    }
}

#[tokio::test]
async fn the_paired_fingerprint_connects() {
    let (identity, address, server) = desktop();
    let accepting = tokio::spawn(accept_once(server));

    let client = client_endpoint(&identity.fingerprint, address).expect("bind client");
    let connecting = client.connect(address, "localhost").expect("dial");
    let connection = tokio::time::timeout(Duration::from_secs(5), connecting)
        .await
        .expect("client did not settle")
        .expect("handshake should succeed against the paired certificate");

    assert!(accepting.await.expect("accept task"));
    connection.close(0u32.into(), b"done");
}

/// The one that matters: a desktop presenting a certificate the phone was not paired with is
/// refused, even though it is a perfectly valid self-signed certificate on loopback.
#[tokio::test]
async fn a_different_certificate_is_refused() {
    let (_identity, address, server) = desktop();
    let accepting = tokio::spawn(accept_once(server));

    let impostor = generate_self_signed(vec!["localhost".into()]).expect("second identity");
    let client = client_endpoint(&impostor.fingerprint, address).expect("bind client");
    let connecting = client.connect(address, "localhost").expect("dial");
    let outcome = tokio::time::timeout(Duration::from_secs(5), connecting)
        .await
        .expect("client did not settle");

    assert!(
        outcome.is_err(),
        "a certificate that is not the paired one must not be accepted"
    );
    assert!(
        !accepting.await.expect("accept task"),
        "the server side must not report a completed handshake either"
    );
}

/// ALPN is the cheap gate: a peer speaking a different application protocol is rejected during
/// the handshake, before any application byte is read.
#[tokio::test]
async fn a_peer_offering_another_alpn_is_rejected() {
    let (_identity, address, server) = desktop();
    let accepting = tokio::spawn(accept_once(server));

    let mut client = quinn::Endpoint::client((Ipv4Addr::UNSPECIFIED, 0).into()).expect("bind");
    // Certificate verification is deliberately disabled here so the only thing left to reject the
    // connection is the ALPN mismatch.
    let mut crypto = rustls::ClientConfig::builder_with_provider(std::sync::Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_protocol_versions(&[&rustls::version::TLS13])
    .unwrap()
    .dangerous()
    .with_custom_certificate_verifier(std::sync::Arc::new(AcceptAnything))
    .with_no_client_auth();
    crypto.alpn_protocols = vec![b"not-vmux".to_vec()];
    client.set_default_client_config(quinn::ClientConfig::new(std::sync::Arc::new(
        quinn::crypto::rustls::QuicClientConfig::try_from(crypto).unwrap(),
    )));

    let connecting = client.connect(address, "localhost").expect("dial");
    let outcome = tokio::time::timeout(Duration::from_secs(5), connecting)
        .await
        .expect("client did not settle");

    assert!(outcome.is_err(), "ALPN mismatch must fail the handshake");
    assert!(!accepting.await.expect("accept task"));
}

/// Trusts everything, so the ALPN test above isolates protocol mismatch from certificate
/// mismatch. Never used outside this file.
#[derive(Debug)]
struct AcceptAnything;

impl rustls::client::danger::ServerCertVerifier for AcceptAnything {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        rustls::crypto::ring::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}
