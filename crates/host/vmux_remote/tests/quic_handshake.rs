use std::net::{Ipv4Addr, SocketAddr};
use std::time::Duration;

use vmux_remote::quic::endpoint::{SelfSignedIdentity, Trust};

fn desktop() -> (SelfSignedIdentity, SocketAddr, quinn::Endpoint) {
    let identity = SelfSignedIdentity::generate(vec!["localhost".into(), "127.0.0.1".into()])
        .expect("generate identity");
    let endpoint = identity
        .listen((Ipv4Addr::LOCALHOST, 0).into())
        .expect("bind server");
    let address = endpoint.local_addr().expect("local addr");
    (identity, address, endpoint)
}

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

    let client = Trust::Desktop {
        fingerprint: identity.fingerprint.clone(),
    }
    .endpoint(address)
    .expect("bind client");
    let connecting = client.connect(address, "localhost").expect("dial");
    let connection = tokio::time::timeout(Duration::from_secs(5), connecting)
        .await
        .expect("client did not settle")
        .expect("handshake should succeed against the paired certificate");

    assert!(accepting.await.expect("accept task"));
    connection.close(0u32.into(), b"done");
}

#[tokio::test]
async fn a_different_certificate_is_refused() {
    let (_identity, address, server) = desktop();
    let accepting = tokio::spawn(accept_once(server));

    let impostor = SelfSignedIdentity::generate(vec!["localhost".into()]).expect("second identity");
    let client = Trust::Desktop {
        fingerprint: impostor.fingerprint.clone(),
    }
    .endpoint(address)
    .expect("bind client");
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

#[tokio::test]
async fn a_peer_offering_another_alpn_is_rejected() {
    let (_identity, address, server) = desktop();
    let accepting = tokio::spawn(accept_once(server));

    let mut client = quinn::Endpoint::client((Ipv4Addr::UNSPECIFIED, 0).into()).expect("bind");
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

#[tokio::test]
async fn a_probe_completes_against_a_listener_that_answers_them() {
    let identity = SelfSignedIdentity::generate(vec!["localhost".into(), "127.0.0.1".into()])
        .expect("generate identity");
    let server = identity
        .listen_answering_probes((Ipv4Addr::LOCALHOST, 0).into())
        .expect("bind server");
    let address = server.local_addr().expect("local addr");
    let accepting = tokio::spawn(accept_once(server));

    let outcome = tokio::time::timeout(
        Duration::from_secs(5),
        Trust::Desktop {
            fingerprint: identity.fingerprint.clone(),
        }
        .probe(address, "localhost"),
    )
    .await
    .expect("probe did not settle");

    assert_eq!(outcome, Ok(()));
    assert!(accepting.await.expect("accept task"));
}

#[tokio::test]
async fn an_ordinary_listener_refuses_a_probe() {
    let (identity, address, server) = desktop();
    let accepting = tokio::spawn(accept_once(server));

    let outcome = tokio::time::timeout(
        Duration::from_secs(5),
        Trust::Desktop {
            fingerprint: identity.fingerprint.clone(),
        }
        .probe(address, "localhost"),
    )
    .await
    .expect("probe did not settle");

    assert!(outcome.is_err(), "the probe ALPN must not be offered here");
    assert!(!accepting.await.expect("accept task"));
}

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
