use std::net::SocketAddr;
use std::sync::Arc;

use quinn::crypto::rustls::{QuicClientConfig, QuicServerConfig};
use quinn::{ClientConfig, Endpoint, ServerConfig, TransportConfig};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, SignatureScheme};

use super::{ALPN, PROBE_ALPN};

pub const MAX_IDLE_TIMEOUT_MS: u32 = 30_000;

pub const KEEP_ALIVE_MS: u64 = 10_000;

pub const RECEIVE_WINDOW: u32 = 8 * 1024 * 1024;

pub const MAX_CONCURRENT_BIDI_STREAMS: u32 = 64;

pub const MAX_CONCURRENT_UNI_STREAMS: u32 = 0;

#[derive(Clone, Debug)]
pub struct SelfSignedIdentity {
    pub certificate_pem: String,
    pub private_key_pem: String,
    pub fingerprint: String,
}

impl SelfSignedIdentity {
    pub fn generate(subject_alt_names: Vec<String>) -> Result<Self, String> {
        let certified = rcgen::generate_simple_self_signed(subject_alt_names)
            .map_err(|error| format!("certificate generation failed: {error}"))?;
        Ok(Self {
            certificate_pem: certified.cert.pem(),
            private_key_pem: certified.signing_key.serialize_pem(),
            fingerprint: Self::fingerprint_of(certified.cert.der()),
        })
    }

    pub fn from_pem(certificate_pem: String, private_key_pem: String) -> Result<Self, String> {
        let fingerprint = Self::fingerprint_of_pem(&certificate_pem)?;
        Ok(Self {
            certificate_pem,
            private_key_pem,
            fingerprint,
        })
    }

    pub fn fingerprint_of_pem(certificate_pem: &str) -> Result<String, String> {
        let certificate = rustls_pemfile::certs(&mut certificate_pem.as_bytes())
            .next()
            .ok_or_else(|| "certificate file held no certificate".to_string())?
            .map_err(|error| format!("certificate parse failed: {error}"))?;
        Ok(Self::fingerprint_of(&certificate))
    }

    pub fn fingerprint_of(certificate: &CertificateDer<'_>) -> String {
        let digest = ring::digest::digest(&ring::digest::SHA256, certificate.as_ref());
        let mut hex = String::with_capacity(digest.as_ref().len() * 2);
        for byte in digest.as_ref() {
            use std::fmt::Write;
            let _ = write!(hex, "{byte:02x}");
        }
        hex
    }

    pub fn listen(&self, address: SocketAddr) -> Result<Endpoint, String> {
        let config = self.server_config()?;
        Endpoint::server(config, address).map_err(|error| format!("QUIC bind failed: {error}"))
    }

    pub fn listen_answering_probes(&self, address: SocketAddr) -> Result<Endpoint, String> {
        let config = self.server_config_offering(vec![ALPN.to_vec(), PROBE_ALPN.to_vec()])?;
        Endpoint::server(config, address).map_err(|error| format!("QUIC bind failed: {error}"))
    }

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

#[derive(Clone, Debug)]
pub enum Trust {
    Desktop { fingerprint: String },
    Relay { host: String },
}

impl Trust {
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
    transport.max_concurrent_uni_streams(MAX_CONCURRENT_UNI_STREAMS.into());
    Arc::new(transport)
}

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

#[derive(Debug)]
struct PinnedCertificate {
    expected: String,
}

impl PinnedCertificate {
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
