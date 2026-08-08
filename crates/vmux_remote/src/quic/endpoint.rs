//! Building the two ends of a QUIC connection, and deciding whether to trust the far one.
//!
//! QUIC has no cleartext mode, so the loopback and LAN paths that used to be plain HTTP now need
//! a certificate. There is no CA that will sign for `192.168.1.4`, so the desktop mints its own
//! ([`SelfSignedIdentity`]) and the pairing QR carries its fingerprint. The phone then trusts
//! exactly that certificate and nothing else — narrower than the public CA set, and far narrower
//! than the `danger_accept_invalid_certs(true)` this replaces, which accepted *any* certificate on
//! a private address.
//!
//! The two sides are shaped differently on purpose. A listener is defined by the identity it
//! presents, so it hangs off [`SelfSignedIdentity`]. A dialler is defined by whose certificate it
//! will accept, so it hangs off [`Trust`].

use std::net::SocketAddr;
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
#[derive(Clone, Debug)]
pub struct SelfSignedIdentity {
    pub certificate_pem: String,
    pub private_key_pem: String,
    /// Lowercase hex SHA-256 over the certificate DER. This is what the pairing link carries and
    /// what the client pins.
    pub fingerprint: String,
}

impl SelfSignedIdentity {
    /// Mint a certificate covering the addresses a phone might reach this desktop on.
    pub fn generate(subject_alt_names: Vec<String>) -> Result<Self, String> {
        let certified = rcgen::generate_simple_self_signed(subject_alt_names)
            .map_err(|error| format!("certificate generation failed: {error}"))?;
        Ok(Self {
            certificate_pem: certified.cert.pem(),
            private_key_pem: certified.signing_key.serialize_pem(),
            fingerprint: certificate_fingerprint(certified.cert.der()),
        })
    }

    /// Adopt an identity already on disk, deriving its fingerprint.
    pub fn from_pem(certificate_pem: String, private_key_pem: String) -> Result<Self, String> {
        let fingerprint = Self::fingerprint_of_pem(&certificate_pem)?;
        Ok(Self {
            certificate_pem,
            private_key_pem,
            fingerprint,
        })
    }

    /// The fingerprint a PEM would be pinned by, without building an identity around it.
    pub fn fingerprint_of_pem(certificate_pem: &str) -> Result<String, String> {
        let certificate = rustls_pemfile::certs(&mut certificate_pem.as_bytes())
            .next()
            .ok_or_else(|| "certificate file held no certificate".to_string())?
            .map_err(|error| format!("certificate parse failed: {error}"))?;
        Ok(certificate_fingerprint(&certificate))
    }

    /// Bind a listener presenting this identity. Port 0 asks the OS to choose, which is what
    /// tests want.
    pub fn listen(&self, address: SocketAddr) -> Result<Endpoint, String> {
        let config = self.server_config()?;
        Endpoint::server(config, address).map_err(|error| format!("QUIC bind failed: {error}"))
    }

    /// The server half of the TLS configuration, for callers driving their own endpoint.
    pub fn server_config(&self) -> Result<ServerConfig, String> {
        let certificates: Vec<CertificateDer<'static>> =
            rustls_pemfile::certs(&mut self.certificate_pem.as_bytes())
                .collect::<Result<_, _>>()
                .map_err(|error| format!("certificate parse failed: {error}"))?;
        let key: PrivateKeyDer<'static> =
            rustls_pemfile::private_key(&mut self.private_key_pem.as_bytes())
                .map_err(|error| format!("private key parse failed: {error}"))?
                .ok_or_else(|| "private key file held no key".to_string())?;

        let mut crypto = rustls::ServerConfig::builder_with_provider(provider())
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
}

/// Whose certificate a dialler is willing to accept.
#[derive(Clone, Debug)]
pub enum Trust {
    /// Exactly one certificate, by fingerprint — a desktop no CA will vouch for.
    ///
    /// The hostname is not checked: the certificate is pinned outright, so the name adds nothing,
    /// and a phone reaches the same desktop as `127.0.0.1`, a LAN address, or whatever the pairing
    /// link recorded.
    Desktop { fingerprint: String },
    /// A relay, verified by name against the public roots.
    ///
    /// Not pinned, because there is nothing stable to pin: the relay's certificate is renewed on
    /// its own schedule, so a fingerprint captured at pairing time would start rejecting it
    /// without warning. A relay on loopback or a private address is a development stack whose
    /// certificate no public root signs, so nothing is verified there — the same allowance the
    /// HTTP client made, and not reachable from anywhere an attacker could sit.
    Relay { host: String },
}

impl Trust {
    /// Bind a client endpoint on an ephemeral local port, configured for this decision.
    ///
    /// `remote` decides which family the local socket binds. An IPv4 socket cannot dial an IPv6
    /// peer, and `localhost` resolves to `::1` first on macOS, so binding a fixed family turns a
    /// perfectly reachable peer into `invalid remote address`.
    pub fn endpoint(&self, remote: SocketAddr) -> Result<Endpoint, String> {
        let local: SocketAddr = if remote.is_ipv4() {
            (std::net::Ipv4Addr::UNSPECIFIED, 0).into()
        } else {
            (std::net::Ipv6Addr::UNSPECIFIED, 0).into()
        };
        let mut endpoint =
            Endpoint::client(local).map_err(|error| format!("QUIC client bind failed: {error}"))?;
        endpoint.set_default_client_config(self.client_config()?);
        Ok(endpoint)
    }

    fn client_config(&self) -> Result<ClientConfig, String> {
        let builder = rustls::ClientConfig::builder_with_provider(provider())
            .with_protocol_versions(&[&rustls::version::TLS13])
            .map_err(|error| format!("TLS config failed: {error}"))?;

        let mut crypto = match self {
            Self::Desktop { fingerprint } => builder
                .dangerous()
                .with_custom_certificate_verifier(Arc::new(PinnedCertificate {
                    expected: fingerprint.to_ascii_lowercase(),
                }))
                .with_no_client_auth(),
            Self::Relay { host } if is_local_development_host(host) => builder
                .dangerous()
                .with_custom_certificate_verifier(Arc::new(AnyCertificate))
                .with_no_client_auth(),
            Self::Relay { .. } => {
                let roots = rustls::RootCertStore {
                    roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
                };
                builder.with_root_certificates(roots).with_no_client_auth()
            }
        };
        crypto.alpn_protocols = vec![ALPN.to_vec()];

        let quic = QuicClientConfig::try_from(crypto)
            .map_err(|error| format!("QUIC client config failed: {error}"))?;
        let mut config = ClientConfig::new(Arc::new(quic));
        config.transport_config(transport_config());
        Ok(config)
    }
}

/// Resolve `host:port`, preferring IPv4.
///
/// `localhost` resolves to `::1` before `127.0.0.1` on macOS, and a UDP port published by Docker
/// answers on IPv4 only however it advertises itself — so taking the first result silently sends
/// every packet somewhere that never replies. IPv6 is still used when it is all the host has.
pub async fn resolve_preferring_ipv4(host: &str, port: u16) -> Result<SocketAddr, String> {
    let resolved: Vec<SocketAddr> = tokio::net::lookup_host((host, port))
        .await
        .map_err(|error| format!("resolve {host}: {error}"))?
        .collect();
    resolved
        .iter()
        .find(|address| address.is_ipv4())
        .or_else(|| resolved.first())
        .copied()
        .ok_or_else(|| format!("{host} resolved to nothing"))
}

/// SHA-256 over the whole certificate DER, lowercase hex.
///
/// The certificate rather than its SubjectPublicKeyInfo, because the verifier only ever sees the
/// peer's DER and extracting SPKI from it would mean parsing ASN.1 here. The cost is that
/// re-issuing invalidates every paired client — acceptable while re-pairing is a QR scan, and the
/// reason a stored certificate is reused instead of minting one per launch.
pub fn certificate_fingerprint(certificate: &CertificateDer<'_>) -> String {
    hex_lower(ring::digest::digest(&ring::digest::SHA256, certificate.as_ref()).as_ref())
}

fn provider() -> Arc<rustls::crypto::CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
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

/// Whether this host can only be a development relay.
fn is_local_development_host(host: &str) -> bool {
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    match host.parse::<std::net::IpAddr>() {
        Ok(std::net::IpAddr::V4(ip)) => ip.is_loopback() || ip.is_private() || ip.is_link_local(),
        Ok(std::net::IpAddr::V6(ip)) => ip.is_loopback() || ip.segments()[0] & 0xfe00 == 0xfc00,
        Err(_) => false,
    }
}

/// Accepts anything. Only ever installed for a relay on a private address.
#[derive(Debug)]
struct AnyCertificate;

impl ServerCertVerifier for AnyCertificate {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Err(rustls::Error::PeerIncompatible(
            rustls::PeerIncompatible::ServerTlsVersionIsDisabledByOurConfig,
        ))
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
            &provider().signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// Accepts one specific certificate and refuses everything else.
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
            &provider().signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// Length-independent only for equal-length inputs, which is all this compares: two hex digests.
///
/// Written as a loop rather than a fold so the absence of an early exit is visible: the whole
/// point is that every byte is compared regardless of where the first difference falls.
fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut difference = 0u8;
    for index in 0..left.len() {
        difference |= left[index] ^ right[index];
    }
    difference == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fingerprint_is_stable_and_key_specific() {
        let first = SelfSignedIdentity::generate(vec!["localhost".into()]).unwrap();
        let second = SelfSignedIdentity::generate(vec!["localhost".into()]).unwrap();

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

    /// Reloading from disk has to produce the same fingerprint the pairing link recorded, or every
    /// paired phone silently stops trusting the desktop after a restart.
    #[test]
    fn an_identity_reloaded_from_pem_keeps_its_fingerprint() {
        let minted = SelfSignedIdentity::generate(vec!["localhost".into()]).unwrap();

        let reloaded = SelfSignedIdentity::from_pem(
            minted.certificate_pem.clone(),
            minted.private_key_pem.clone(),
        )
        .unwrap();

        assert_eq!(reloaded.fingerprint, minted.fingerprint);
        assert_eq!(
            SelfSignedIdentity::fingerprint_of_pem(&minted.certificate_pem).unwrap(),
            minted.fingerprint
        );
    }

    #[test]
    fn configs_build_for_both_ends() {
        let identity =
            SelfSignedIdentity::generate(vec!["localhost".into(), "127.0.0.1".into()]).unwrap();

        identity.server_config().unwrap();
        Trust::Desktop {
            fingerprint: identity.fingerprint.clone(),
        }
        .client_config()
        .unwrap();
        Trust::Relay {
            host: "relay.vmux.ai".into(),
        }
        .client_config()
        .unwrap();
        Trust::Relay {
            host: "localhost".into(),
        }
        .client_config()
        .unwrap();
    }

    /// A public relay must not fall into the verify-nothing branch meant for a dev stack.
    #[test]
    fn only_private_relay_hosts_skip_verification() {
        assert!(is_local_development_host("localhost"));
        assert!(is_local_development_host("127.0.0.1"));
        assert!(is_local_development_host("192.168.1.4"));
        assert!(!is_local_development_host("relay.vmux.ai"));
        assert!(!is_local_development_host("8.8.8.8"));
    }

    #[test]
    fn comparison_rejects_a_different_or_truncated_fingerprint() {
        assert!(constant_time_eq(b"abcd", b"abcd"));
        assert!(!constant_time_eq(b"abcd", b"abce"));
        assert!(!constant_time_eq(b"abcd", b"abc"));
    }
}
