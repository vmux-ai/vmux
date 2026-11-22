use std::collections::HashMap;
use std::path::PathBuf;

use bevy_ecs::component::Component;
use serde::{Deserialize, Serialize};
use url::Url;
use vmux_transport::{ClientCredential, DeviceId};
use vmux_ui::i18n::translate;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub(crate) struct CredentialsPath(pub(crate) Option<PathBuf>);

impl Default for CredentialsPath {
    fn default() -> Self {
        Self(Self::default_path())
    }
}

impl CredentialsPath {
    #[cfg(test)]
    pub(crate) fn at(path: PathBuf) -> Self {
        Self(Some(path))
    }

    pub(crate) fn load(&self) -> Result<Option<Credentials>, String> {
        let Some(path) = self.0.as_ref() else {
            return Ok(None);
        };
        match std::fs::read_to_string(path) {
            Ok(raw) => serde_json::from_str(&raw)
                .map(Some)
                .map_err(|error| error.to_string()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.to_string()),
        }
    }

    pub(crate) fn save(&self, credentials: &Credentials) -> Result<(), String> {
        let path = self
            .0
            .as_ref()
            .ok_or_else(|| "credentials path unavailable".to_string())?;
        let body = serde_json::to_string(credentials).map_err(|error| error.to_string())?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        vmux_path::AtomicFile::write(path, body.as_bytes()).map_err(|error| error.to_string())?;
        #[cfg(unix)]
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub(crate) fn remove(&self) -> Result<(), String> {
        let Some(path) = self.0.as_ref() else {
            return Ok(());
        };
        match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.to_string()),
        }
    }

    fn default_path() -> Option<PathBuf> {
        let home = std::env::var_os("HOME")?;
        Some(
            PathBuf::from(home)
                .join("Library/Application Support")
                .join(vmux_app::host::Product::remote_data_name())
                .join("pairing.json"),
        )
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub(crate) struct Credentials {
    pub(crate) base_url: String,
    #[serde(alias = "token")]
    pub(crate) relay_token: String,
    #[serde(default)]
    pub(crate) credential: Option<ClientCredential>,
    #[serde(default)]
    pub(crate) client_id: DeviceId,
    #[serde(default)]
    pub(crate) fingerprint: String,
    #[serde(default)]
    pub(crate) device: String,
}

impl Credentials {
    pub(crate) fn endpoint(&self) -> Option<crate::quic::Endpoint> {
        if self.fingerprint.is_empty()
            || self.device.is_empty()
            || self.client_id.as_str().is_empty()
        {
            return None;
        }
        let credential = self.credential.clone()?;
        let parsed = Url::parse(&self.base_url).ok()?;
        let host = parsed.host_str()?;
        let port = parsed.port().unwrap_or(443);
        Some(crate::quic::Endpoint {
            address: format!("{host}:{port}"),
            relay_token: self.relay_token.clone(),
            credential,
            client_id: self.client_id.clone(),
            fingerprint: self.fingerprint.clone(),
            desktop: DeviceId::new(&self.device),
        })
    }

    pub(crate) fn parse(input: &str) -> Result<Self, String> {
        let input = input.trim();
        if input.starts_with("vmux://") {
            return Self::parse_deep_link(input);
        }
        Self::parse_web_link(input)
    }

    pub(crate) fn pairing_url(&self) -> String {
        let Some(ClientCredential::Pairing(pairing_token)) = &self.credential else {
            return String::new();
        };
        let fragment = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("relay_token", &self.relay_token)
            .append_pair("pairing_token", pairing_token)
            .append_pair("fp", &self.fingerprint)
            .append_pair("device", &self.device)
            .finish();
        format!("{base}/#{fragment}", base = self.base_url)
    }

    fn parse_deep_link(input: &str) -> Result<Self, String> {
        let parsed = Url::parse(input).map_err(|_| translate("mobile-url-invalid"))?;
        if parsed.scheme() != "vmux" || parsed.host_str() != Some("pair") {
            return Err(translate("mobile-url-invalid"));
        }
        let mut params = HashMap::new();
        for (name, value) in parsed.query_pairs() {
            params.insert(name.into_owned(), value.into_owned());
        }
        let base_url = params
            .get("base")
            .cloned()
            .ok_or_else(|| translate("mobile-url-no-address"))?;
        let base = Url::parse(&base_url).map_err(|_| translate("mobile-url-bad-address"))?;
        if !matches!(base.scheme(), "http" | "https") {
            return Err(translate("mobile-url-scheme"));
        }
        Self::from_parts(base, &params)
    }

    fn parse_web_link(input: &str) -> Result<Self, String> {
        let start = input
            .find("https://")
            .or_else(|| input.find("http://"))
            .ok_or_else(|| translate("mobile-url-paste-full"))?;
        let candidate = input[start..].split_whitespace().next().unwrap_or_default();
        let parsed = Url::parse(candidate).map_err(|_| translate("mobile-url-invalid"))?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err(translate("mobile-url-scheme"));
        }
        let mut params = HashMap::new();
        if let Some(fragment) = parsed.fragment() {
            for (name, value) in url::form_urlencoded::parse(fragment.as_bytes()) {
                params
                    .entry(name.into_owned())
                    .or_insert_with(|| value.into_owned());
            }
        }
        Self::from_parts(parsed, &params)
    }

    fn from_parts(base: Url, params: &HashMap<String, String>) -> Result<Self, String> {
        let relay_token = Self::required_token(params, "relay_token")?;
        let pairing_token = Self::required_token(params, "pairing_token")?;
        let fingerprint = params.get("fp").cloned().unwrap_or_default();
        let device = params.get("device").cloned().unwrap_or_default();
        let base_url = Self::normalized_base(base);
        if base_url.is_empty() {
            return Err(translate("mobile-url-no-address"));
        }
        Ok(Self {
            base_url,
            relay_token,
            credential: Some(ClientCredential::Pairing(pairing_token)),
            client_id: DeviceId::new(uuid::Uuid::new_v4().simple().to_string()),
            fingerprint,
            device,
        })
    }

    fn required_token(params: &HashMap<String, String>, key: &str) -> Result<String, String> {
        let candidate = params.get(key).or_else(|| params.get("token"));
        match candidate {
            Some(value) if !value.is_empty() => Ok(value.clone()),
            _ => Err(translate("mobile-url-no-token")),
        }
    }

    fn normalized_base(mut url: Url) -> String {
        url.set_fragment(None);
        url.set_query(None);
        if url.origin().ascii_serialization() == "null" {
            return String::new();
        }
        let mut value = url.to_string();
        while value.ends_with('/') {
            value.pop();
        }
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::remote::Api;

    #[test]
    fn a_pairing_link_carries_the_certificate_fingerprint() {
        let expected = "c620a502885ddf230420184cc3a1b190792c14c1049ab76a6a63596054a1025e";

        let pasted = Credentials::parse(&format!(
            "https://mac.example.ts.net/#token=secret&fp={expected}"
        ))
        .unwrap();
        let deep_link = Credentials::parse(&format!(
            "vmux://pair?base=https%3A%2F%2Fmac.example.ts.net&token=secret&fp={expected}"
        ))
        .unwrap();

        assert_eq!(pasted.fingerprint, expected);
        assert_eq!(deep_link.fingerprint, expected);
        assert_eq!(pasted.relay_token, "secret");
        assert_eq!(
            pasted.credential,
            Some(ClientCredential::Pairing("secret".to_string()))
        );
    }

    #[test]
    fn a_link_without_a_fingerprint_parses_but_cannot_be_dialled() {
        let credentials = Credentials::parse("https://mac.example.ts.net/#token=secret").unwrap();

        assert!(credentials.fingerprint.is_empty());
        assert_eq!(credentials.relay_token, "secret");
        assert!(
            Api::new(credentials).is_err(),
            "an unpinned pairing must be refused, not silently downgraded"
        );
    }

    #[test]
    fn a_written_pairing_preserves_transport_credentials() {
        let original = Credentials {
            base_url: "https://mac.example.ts.net".to_string(),
            relay_token: "relay-secret".to_string(),
            credential: Some(ClientCredential::Pairing("pairing-secret".to_string())),
            client_id: DeviceId::new("phone-a"),
            fingerprint: "c620a502885ddf230420184cc3a1b190".to_string(),
            device: "device-1".to_string(),
        };
        let parsed = Credentials::parse(&original.pairing_url()).unwrap();

        assert_eq!(parsed.base_url, original.base_url);
        assert_eq!(parsed.relay_token, original.relay_token);
        assert_eq!(parsed.credential, original.credential);
        assert_eq!(parsed.fingerprint, original.fingerprint);
        assert_eq!(parsed.device, original.device);
        assert!(!parsed.client_id.as_str().is_empty());
        assert_ne!(parsed.client_id, original.client_id);
    }

    #[test]
    fn parses_pairing_url() {
        let credentials =
            Credentials::parse("paste into Vmux: https://mac.example.ts.net/#token=secret")
                .unwrap();

        assert_eq!(credentials.base_url, "https://mac.example.ts.net");
        assert_eq!(credentials.relay_token, "secret");
        assert_eq!(
            credentials.credential,
            Some(ClientCredential::Pairing("secret".to_string()))
        );
        assert!(credentials.fingerprint.is_empty());
        assert!(credentials.device.is_empty());
        assert!(!credentials.client_id.as_str().is_empty());
    }

    #[test]
    fn parses_pairing_deep_link() {
        let credentials = Credentials::parse(
            "vmux://pair?base=https%3A%2F%2Fmac.example.ts.net%3A54821&token=secret",
        )
        .unwrap();

        assert_eq!(credentials.base_url, "https://mac.example.ts.net:54821");
        assert_eq!(credentials.relay_token, "secret");
        assert_eq!(
            credentials.credential,
            Some(ClientCredential::Pairing("secret".to_string()))
        );
        assert!(credentials.fingerprint.is_empty());
        assert!(credentials.device.is_empty());
    }

    #[test]
    fn pairing_url_preserves_relay_path() {
        let credentials =
            Credentials::parse("http://localhost:8787/r/device-1/#token=secret").unwrap();

        assert_eq!(credentials.base_url, "http://localhost:8787/r/device-1");
        assert_eq!(credentials.relay_token, "secret");
    }

    #[test]
    fn credential_write_and_remove_failures_are_observable() {
        let root = std::env::temp_dir().join(format!("vmux-mobile-cred-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let directory_target = root.join("as-dir");
        std::fs::create_dir_all(&directory_target).unwrap();
        let storage = CredentialsPath::at(directory_target);
        let credentials = Credentials {
            base_url: "https://mac.example.ts.net".to_string(),
            relay_token: "relay".to_string(),
            credential: Some(ClientCredential::Device("device-tok".to_string())),
            client_id: DeviceId::new("phone-1"),
            fingerprint: "abcd".to_string(),
            device: "mac-1".to_string(),
        };

        assert!(storage.save(&credentials).is_err());
        assert!(storage.remove().is_err());

        let _ = std::fs::remove_dir_all(&root);
    }
}
