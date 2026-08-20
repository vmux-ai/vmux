//! The pairing a phone holds for one desktop, and the links it is written as.

use serde::{Deserialize, Serialize};
use url::Url;
use vmux_ui::i18n::translate;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub(crate) struct Credentials {
    pub(crate) base_url: String,
    pub(crate) token: String,
    /// SHA-256 of the desktop's QUIC certificate, pinned when dialling it.
    ///
    /// Defaulted rather than required so a pairing written by an older build still deserialises.
    /// It is refused on use instead, which tells the phone to scan again rather than silently
    /// forgetting the Mac.
    #[serde(default)]
    pub(crate) fingerprint: String,
    /// Which desktop to ask the relay for.
    ///
    /// Every desktop is reached at the same relay address, so the link has to name one. Defaulted
    /// for the same reason as the fingerprint: a pairing written before the relay routed by
    /// identity still deserialises, and is refused on use rather than forgotten silently.
    #[serde(default)]
    pub(crate) device: String,
}

impl Credentials {
    /// Build the QUIC endpoint from a pairing, when it carried both a fingerprint and a device.
    ///
    /// A pairing missing either cannot reach anything: the fingerprint is what the inner session
    /// pins, and the device is what the relay routes on. Returning `None` sends the phone back to
    /// the scanner rather than into a dial that would be refused.
    pub(crate) fn endpoint(&self) -> Option<crate::quic_api::Endpoint> {
        if self.fingerprint.is_empty() || self.device.is_empty() {
            return None;
        }
        let parsed = Url::parse(&self.base_url).ok()?;
        let host = parsed.host_str()?;
        let port = parsed.port().unwrap_or(443);
        Some(crate::quic_api::Endpoint {
            address: format!("{host}:{port}"),
            token: self.token.clone(),
            fingerprint: self.fingerprint.clone(),
            desktop: vmux_remote::DeviceId::new(&self.device),
        })
    }

    /// Read a pairing out of either link the desktop hands out: a pasted `https://` address with
    /// the secrets in its fragment, or a `vmux://pair` deep link with them in its query.
    pub(crate) fn parse(input: &str) -> Result<Credentials, String> {
        let input = input.trim();
        if input.starts_with("vmux://") {
            let parsed = Url::parse(input).map_err(|_| translate("mobile-url-invalid"))?;
            if parsed.scheme() != "vmux" || parsed.host_str() != Some("pair") {
                return Err(translate("mobile-url-invalid"));
            }
            let params = parsed
                .query_pairs()
                .collect::<std::collections::HashMap<_, _>>();
            let base_url = params
                .get("base")
                .map(|value| value.to_string())
                .ok_or_else(|| translate("mobile-url-no-address"))?;
            let token = params
                .get("token")
                .map(|value| value.to_string())
                .filter(|value| !value.is_empty())
                .ok_or_else(|| translate("mobile-url-no-token"))?;
            let base = Url::parse(&base_url).map_err(|_| translate("mobile-url-bad-address"))?;
            if !matches!(base.scheme(), "http" | "https") {
                return Err(translate("mobile-url-scheme"));
            }
            // Absent when the desktop has no QUIC listener yet, which leaves the phone on HTTP
            // rather than failing to pair.
            let fingerprint = params
                .get("fp")
                .map(|value| value.to_string())
                .unwrap_or_default();
            let device = params
                .get("device")
                .map(|value| value.to_string())
                .unwrap_or_default();
            let base_url = normalized_pairing_base(base)?;
            if base_url.is_empty() {
                return Err(translate("mobile-url-no-address"));
            }
            return Ok(Credentials {
                base_url,
                token,
                fingerprint,
                device,
            });
        }
        let start = input
            .find("https://")
            .or_else(|| input.find("http://"))
            .ok_or_else(|| translate("mobile-url-paste-full"))?;
        let candidate = input[start..].split_whitespace().next().unwrap_or_default();
        let parsed = Url::parse(candidate).map_err(|_| translate("mobile-url-invalid"))?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err(translate("mobile-url-scheme"));
        }
        let token = parsed
            .fragment()
            .and_then(|fragment| {
                url::form_urlencoded::parse(fragment.as_bytes())
                    .find(|(name, _)| name == "token")
                    .map(|(_, value)| value.into_owned())
            })
            .filter(|token| !token.is_empty())
            .ok_or_else(|| translate("mobile-url-no-token"))?;
        let fingerprint = parsed
            .fragment()
            .and_then(|fragment| {
                url::form_urlencoded::parse(fragment.as_bytes())
                    .find(|(name, _)| name == "fp")
                    .map(|(_, value)| value.into_owned())
            })
            .unwrap_or_default();
        let device = parsed
            .fragment()
            .and_then(|fragment| {
                url::form_urlencoded::parse(fragment.as_bytes())
                    .find(|(name, _)| name == "device")
                    .map(|(_, value)| value.into_owned())
            })
            .unwrap_or_default();
        let base_url = normalized_pairing_base(parsed)?;
        if base_url.is_empty() {
            return Err(translate("mobile-url-no-address"));
        }
        Ok(Credentials {
            base_url,
            token,
            fingerprint,
            device,
        })
    }

    /// The pasteable form of this pairing.
    pub(crate) fn pairing_url(&self) -> String {
        format!("{}/#token={}", self.base_url, self.token)
    }
}

fn normalized_pairing_base(mut url: Url) -> Result<String, String> {
    url.set_fragment(None);
    url.set_query(None);
    if url.origin().ascii_serialization() == "null" {
        return Ok(String::new());
    }
    let mut value = url.to_string();
    while value.ends_with('/') {
        value.pop();
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::Api;

    /// The fingerprint is the whole basis for trusting the desktop's certificate. If it were
    /// dropped while parsing, the phone would silently fall back to an unpinned connection —
    /// a downgrade with no visible symptom, so both pairing shapes are covered.
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
        assert_eq!(
            pasted.token, "secret",
            "the token must survive alongside it"
        );
    }

    /// A link with no fingerprint parses but cannot be used: there is no unpinned transport left
    /// to fall back to. It has to fail here, at the point of use, rather than at parse time —
    /// that is what lets the phone say "scan again" instead of "malformed link".
    #[test]
    fn a_link_without_a_fingerprint_parses_but_cannot_be_dialled() {
        let credentials = Credentials::parse("https://mac.example.ts.net/#token=secret").unwrap();

        assert!(credentials.fingerprint.is_empty());
        assert_eq!(credentials.token, "secret");
        assert!(
            Api::new(credentials).is_err(),
            "an unpinned pairing must be refused, not silently downgraded"
        );
    }

    #[test]
    fn parses_pairing_url() {
        assert_eq!(
            Credentials::parse("paste into Vmux: https://mac.example.ts.net/#token=secret")
                .unwrap(),
            Credentials {
                base_url: "https://mac.example.ts.net".to_string(),
                token: "secret".to_string(),
                fingerprint: String::new(),
                device: String::new(),
            }
        );
    }

    #[test]
    fn parses_pairing_deep_link() {
        assert_eq!(
            Credentials::parse(
                "vmux://pair?base=https%3A%2F%2Fmac.example.ts.net%3A54821&token=secret"
            )
            .unwrap(),
            Credentials {
                base_url: "https://mac.example.ts.net:54821".to_string(),
                token: "secret".to_string(),
                fingerprint: String::new(),
                device: String::new(),
            }
        );
    }

    #[test]
    fn pairing_url_preserves_relay_path() {
        assert_eq!(
            Credentials::parse("http://localhost:8787/r/device-1/#token=secret").unwrap(),
            Credentials {
                base_url: "http://localhost:8787/r/device-1".to_string(),
                token: "secret".to_string(),
                fingerprint: String::new(),
                device: String::new(),
            }
        );
    }
}
