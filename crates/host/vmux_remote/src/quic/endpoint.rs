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

use super::{ALPN, PROBE_ALPN};

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
            fingerprint: Self::fingerprint_of(certified.cert.der()),
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
        Ok(Self::fingerprint_of(&certificate))
    }

    /// SHA-256 over the whole certificate DER, lowercase hex.
    ///
    /// The certificate rather than its SubjectPublicKeyInfo, because a verifier only ever sees the
    /// peer's DER and extracting SPKI from it would mean parsing ASN.1 here. The cost is that
    /// re-issuing invalidates every paired client — acceptable while re-pairing is a QR scan, and
    /// the reason a stored certificate is reused instead of minted per launch.
    pub fn fingerprint_of(certificate: &CertificateDer<'_>) -> String {
        let digest = ring::digest::digest(&ring::digest::SHA256, certificate.as_ref());
        let mut hex = String::with_capacity(digest.as_ref().len() * 2);
        for byte in digest.as_ref() {
            use std::fmt::Write;
            let _ = write!(hex, "{byte:02x}");
        }
        hex
    }

    /// Bind a listener presenting this identity. Port 0 asks the OS to choose, which is what
    /// tests want.
    pub fn listen(&self, address: SocketAddr) -> Result<Endpoint, String> {
        let config = self.server_config()?;
        Endpoint::server(config, address).map_err(|error| format!("QUIC bind failed: {error}"))
    }

    /// Bind a listener that also completes [`PROBE_ALPN`] handshakes.
    ///
    /// Only the relay offers this. On a desktop it would be an unauthenticated way for anyone who
    /// found the port to confirm someone is home, which is what pinning and the pairing token are
    /// there to prevent.
    pub fn listen_answering_probes(&self, address: SocketAddr) -> Result<Endpoint, String> {
        let config = self.server_config_offering(vec![ALPN.to_vec(), PROBE_ALPN.to_vec()])?;
        Endpoint::server(config, address).map_err(|error| format!("QUIC bind failed: {error}"))
    }

    /// The server half of the TLS configuration, for callers driving their own endpoint.
    pub fn server_config(&self) -> Result<ServerConfig, String> {
        self.server_config_offering(vec![ALPN.to_vec()])
    }

    fn server_config_offering(&self, alpn_protocols: Vec<Vec<u8>>) -> Result<ServerConfig, String> {
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
        crypto.alpn_protocols = alpn_protocols;

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

    /// A client endpoint over `socket` rather than a real UDP socket, for dialling through a
    /// tunnel.
    ///
    /// `None` for the server config is what makes it client-only: the peer at the other end of a
    /// tunnel is one desktop, and nothing dials back through it.
    pub fn endpoint_on(
        &self,
        socket: std::sync::Arc<dyn quinn::AsyncUdpSocket>,
    ) -> Result<Endpoint, String> {
        let mut endpoint = Endpoint::new_with_abstract_socket(
            quinn::EndpointConfig::default(),
            None,
            socket,
            std::sync::Arc::new(quinn::TokioRuntime),
        )
        .map_err(|error| format!("QUIC tunnel endpoint failed: {error}"))?;
        endpoint.set_default_client_config(self.client_config()?);
        Ok(endpoint)
    }

    /// Complete a [`PROBE_ALPN`] handshake against `remote` and hang up.
    ///
    /// Returning `Ok` means the UDP port answered, the certificate verified, and the peer's accept
    /// loop was running — the four things a deploy wants to know, none of which a TCP check on a
    /// companion port can establish. Nothing is registered and nothing is sent.
    pub async fn probe(&self, remote: SocketAddr, server_name: &str) -> Result<(), String> {
        let local: SocketAddr = if remote.is_ipv4() {
            (std::net::Ipv4Addr::UNSPECIFIED, 0).into()
        } else {
            (std::net::Ipv6Addr::UNSPECIFIED, 0).into()
        };
        let mut endpoint =
            Endpoint::client(local).map_err(|error| format!("QUIC client bind failed: {error}"))?;
        endpoint.set_default_client_config(self.client_config_offering(PROBE_ALPN)?);

        let connection = endpoint
            .connect(remote, server_name)
            .map_err(|error| format!("probe connect failed: {error}"))?
            .await
            .map_err(|error| format!("probe handshake failed: {error}"))?;
        connection.close(0u32.into(), b"probe");
        endpoint.wait_idle().await;
        Ok(())
    }

    /// Whether a relay host can only be a development stack, and so has no publicly-signed
    /// certificate to verify against.
    fn is_local_development_host(host: &str) -> bool {
        if host.eq_ignore_ascii_case("localhost") {
            return true;
        }
        match host.parse::<std::net::IpAddr>() {
            Ok(std::net::IpAddr::V4(ip)) => {
                ip.is_loopback() || ip.is_private() || ip.is_link_local()
            }
            Ok(std::net::IpAddr::V6(ip)) => ip.is_loopback() || ip.segments()[0] & 0xfe00 == 0xfc00,
            Err(_) => false,
        }
    }

    fn client_config(&self) -> Result<ClientConfig, String> {
        self.client_config_offering(ALPN)
    }

    fn client_config_offering(&self, alpn: &[u8]) -> Result<ClientConfig, String> {
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
            Self::Relay { host } if Self::is_local_development_host(host) => builder
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
        crypto.alpn_protocols = vec![alpn.to_vec()];

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

fn provider() -> Arc<rustls::crypto::CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
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

impl PinnedCertificate {
    /// Length-independent for equal-length inputs, which is all this compares: two hex digests.
    ///
    /// Written as a loop rather than a fold so the absence of an early exit is visible — the whole
    /// point is that every byte is compared regardless of where the first difference falls.
    fn matches(&self, presented: &str) -> bool {
        let (left, right) = (presented.as_bytes(), self.expected.as_bytes());
        if left.len() != right.len() {
            return false;
        }
        let mut difference = 0u8;
        for index in 0..left.len() {
            difference |= left[index] ^ right[index];
        }
        difference == 0
    }
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
        if self.matches(&SelfSignedIdentity::fingerprint_of(end_entity)) {
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
        assert!(Trust::is_local_development_host("localhost"));
        assert!(Trust::is_local_development_host("127.0.0.1"));
        assert!(Trust::is_local_development_host("192.168.1.4"));
        assert!(!Trust::is_local_development_host("relay.vmux.ai"));
        assert!(!Trust::is_local_development_host("8.8.8.8"));
    }

    #[test]
    fn comparison_rejects_a_different_or_truncated_fingerprint() {
        let pin = PinnedCertificate {
            expected: "abcd".into(),
        };

        assert!(pin.matches("abcd"));
        assert!(!pin.matches("abce"));
        assert!(!pin.matches("abc"));
    }
}
