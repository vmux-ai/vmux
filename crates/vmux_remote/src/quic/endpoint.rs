//! Building the two ends of a QUIC connection, and deciding whether to trust the far one.
//!
//! QUIC has no cleartext mode, so the loopback and LAN paths that used to be plain HTTP now need
//! a certificate. There is no CA that will sign for `192.168.1.4`, so the desktop mints its own
//! and the pairing QR carries its fingerprint. The phone then trusts exactly that certificate
//! and nothing else — narrower than the public CA set, and far narrower than the
//! `danger_accept_invalid_certs(true)` this replaces, which accepted *any* certificate on a
//! private address.

use std::sync::Arc;

use quinn::crypto::rustls::{QuicClientConfig, QuicServerConfig};
use quinn::{ClientConfig, Endpoint, ServerConfig, TransportConfig};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, SignatureScheme};

use super::ALPN;

/// How long a connection may sit idle before either end considers it dead.
///
/// Deliberately short. A backgrounded phone has its UDP socket torn down by the OS without a
/// close frame, so the alternative to a timeout is a connection that looks alive forever.
pub const MAX_IDLE_TIMEOUT_MS: u32 = 30_000;

/// Keep-alive interval. Comfortably inside [`MAX_IDLE_TIMEOUT_MS`] so an idle-but-live connection
/// is never mistaken for a dead one.
pub const KEEP_ALIVE_MS: u64 = 10_000;

/// Upper bound on buffered-but-unread bytes per connection.
///
/// The prompt-size check still runs in the dispatcher; this stops a hostile peer from making the
/// daemon hold the bytes before that check is reached.
pub const RECEIVE_WINDOW: u32 = 8 * 1024 * 1024;

/// Concurrent request streams one peer may open.
pub const MAX_CONCURRENT_BIDI_STREAMS: u32 = 64;

/// A self-signed identity for a desktop that no CA will vouch for.
pub struct SelfSignedIdentity {
    pub certificate_pem: String,
    pub private_key_pem: String,
    /// Lowercase hex SHA-256 over the certificate DER. This is what the pairing link carries and
    /// what the client pins.
    pub fingerprint: String,
}

/// Mint a certificate covering the addresses a phone might reach this desktop on.
pub fn generate_self_signed(subject_alt_names: Vec<String>) -> Result<SelfSignedIdentity, String> {
    let certified = rcgen::generate_simple_self_signed(subject_alt_names)
        .map_err(|error| format!("certificate generation failed: {error}"))?;
    Ok(SelfSignedIdentity {
        certificate_pem: certified.cert.pem(),
        private_key_pem: certified.signing_key.serialize_pem(),
        fingerprint: certificate_fingerprint(certified.cert.der()),
    })
}

/// SHA-256 over the whole certificate DER, lowercase hex.
///
/// The certificate rather than its SubjectPublicKeyInfo, because the verifier only ever sees the
/// peer's DER and extracting SPKI from it would mean parsing ASN.1 here. The cost is that
/// re-issuing invalidates every paired client — acceptable while re-pairing is a QR scan, and the
/// reason `ensure_identity` reuses a stored certificate instead of minting one per launch.
pub fn certificate_fingerprint(certificate: &CertificateDer<'_>) -> String {
    hex_lower(ring::digest::digest(&ring::digest::SHA256, certificate.as_ref()).as_ref())
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

fn transport_config() -> Arc<TransportConfig> {
    let mut transport = TransportConfig::default();
    transport.max_idle_timeout(Some(
        std::time::Duration::from_millis(MAX_IDLE_TIMEOUT_MS as u64)
            .try_into()
            .expect("idle timeout fits"),
    ));
    transport.keep_alive_interval(Some(std::time::Duration::from_millis(KEEP_ALIVE_MS)));
    transport.receive_window(RECEIVE_WINDOW.into());
    transport.max_concurrent_bidi_streams(MAX_CONCURRENT_BIDI_STREAMS.into());
    transport.max_concurrent_uni_streams(MAX_CONCURRENT_BIDI_STREAMS.into());
    Arc::new(transport)
}

/// Server config for a desktop presenting `identity`.
pub fn server_config(certificate_pem: &str, private_key_pem: &str) -> Result<ServerConfig, String> {
    let certificates: Vec<CertificateDer<'static>> =
        rustls_pemfile::certs(&mut certificate_pem.as_bytes())
            .collect::<Result<_, _>>()
            .map_err(|error| format!("certificate parse failed: {error}"))?;
    let key: PrivateKeyDer<'static> = rustls_pemfile::private_key(&mut private_key_pem.as_bytes())
        .map_err(|error| format!("private key parse failed: {error}"))?
        .ok_or_else(|| "private key file held no key".to_string())?;

    let mut crypto = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_protocol_versions(&[&rustls::version::TLS13])
    .map_err(|error| format!("TLS config failed: {error}"))?
    .with_no_client_auth()
    .with_single_cert(certificates, key)
    .map_err(|error| format!("TLS config failed: {error}"))?;
    crypto.alpn_protocols = vec![ALPN.to_vec()];

    let quic = QuicServerConfig::try_from(crypto)
        .map_err(|error| format!("QUIC server config failed: {error}"))?;
    let mut config = ServerConfig::with_crypto(Arc::new(quic));
    config.transport_config(transport_config());
    Ok(config)
}

/// Client config that trusts exactly one certificate.
pub fn client_config_pinned(fingerprint: &str) -> Result<ClientConfig, String> {
    let verifier = Arc::new(PinnedCertificate {
        expected: fingerprint.to_ascii_lowercase(),
    });
    let mut crypto = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_protocol_versions(&[&rustls::version::TLS13])
    .map_err(|error| format!("TLS config failed: {error}"))?
    .dangerous()
    .with_custom_certificate_verifier(verifier)
    .with_no_client_auth();
    crypto.alpn_protocols = vec![ALPN.to_vec()];

    let quic = QuicClientConfig::try_from(crypto)
        .map_err(|error| format!("QUIC client config failed: {error}"))?;
    let mut config = ClientConfig::new(Arc::new(quic));
    config.transport_config(transport_config());
    Ok(config)
}

/// Bind a server endpoint. Port 0 asks the OS to choose, which is what tests want.
pub fn server_endpoint(
    address: std::net::SocketAddr,
    certificate_pem: &str,
    private_key_pem: &str,
) -> Result<Endpoint, String> {
    let config = server_config(certificate_pem, private_key_pem)?;
    Endpoint::server(config, address).map_err(|error| format!("QUIC bind failed: {error}"))
}

/// Bind a client endpoint on an ephemeral local port.
pub fn client_endpoint(fingerprint: &str) -> Result<Endpoint, String> {
    let mut endpoint = Endpoint::client((std::net::Ipv4Addr::UNSPECIFIED, 0).into())
        .map_err(|error| format!("QUIC client bind failed: {error}"))?;
    endpoint.set_default_client_config(client_config_pinned(fingerprint)?);
    Ok(endpoint)
}

/// Accepts one specific certificate and refuses everything else.
///
/// Name verification is deliberately skipped: the certificate is pinned outright, so the hostname
/// adds nothing, and a phone reaches the same desktop as `127.0.0.1`, a LAN address, or whatever
/// the pairing link recorded.
#[derive(Debug)]
struct PinnedCertificate {
    expected: String,
}

impl ServerCertVerifier for PinnedCertificate {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        let presented = certificate_fingerprint(end_entity);
        if constant_time_eq(presented.as_bytes(), self.expected.as_bytes()) {
            Ok(ServerCertVerified::assertion())
        } else {
            Err(rustls::Error::General(
                "certificate fingerprint does not match the paired desktop".to_string(),
            ))
        }
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        // TLS 1.3 only; reaching here means the peer negotiated something we did not offer.
        Err(rustls::Error::General("TLS 1.2 is not offered".to_string()))
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        rustls::crypto::ring::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// Length-independent only for equal-length inputs, which is all this compares: two hex digests.
fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0u8, |acc, (a, b)| acc | (a ^ b))
        == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fingerprint_is_stable_and_key_specific() {
        let first = generate_self_signed(vec!["localhost".into()]).unwrap();
        let second = generate_self_signed(vec!["localhost".into()]).unwrap();

        assert_eq!(first.fingerprint.len(), 64);
        assert!(
            first
                .fingerprint
                .chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        );
        assert_ne!(
            first.fingerprint, second.fingerprint,
            "two independently generated identities must not share a fingerprint"
        );
    }

    #[test]
    fn configs_build_from_a_generated_identity() {
        let identity = generate_self_signed(vec!["localhost".into(), "127.0.0.1".into()]).unwrap();

        server_config(&identity.certificate_pem, &identity.private_key_pem).unwrap();
        client_config_pinned(&identity.fingerprint).unwrap();
    }

    #[test]
    fn comparison_rejects_a_different_or_truncated_fingerprint() {
        assert!(constant_time_eq(b"abcd", b"abcd"));
        assert!(!constant_time_eq(b"abcd", b"abce"));
        assert!(!constant_time_eq(b"abcd", b"abc"));
    }
}
