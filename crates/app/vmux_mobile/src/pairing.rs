use crate::qr_scanner;
use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use url::Url;
use vmux_remote::{ClientCredential, DeviceId};
use vmux_ui::i18n::translate;

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
            desktop: vmux_remote::DeviceId::new(&self.device),
        })
    }

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
            let relay_token = params
                .get("relay_token")
                .or_else(|| params.get("token"))
                .map(|value| value.to_string())
                .filter(|value| !value.is_empty())
                .ok_or_else(|| translate("mobile-url-no-token"))?;
            let pairing_token = params
                .get("pairing_token")
                .or_else(|| params.get("token"))
                .map(|value| value.to_string())
                .filter(|value| !value.is_empty())
                .ok_or_else(|| translate("mobile-url-no-token"))?;
            let base = Url::parse(&base_url).map_err(|_| translate("mobile-url-bad-address"))?;
            if !matches!(base.scheme(), "http" | "https") {
                return Err(translate("mobile-url-scheme"));
            }
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
                relay_token,
                credential: Some(ClientCredential::Pairing(pairing_token)),
                client_id: DeviceId::new(uuid::Uuid::new_v4().simple().to_string()),
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
        let relay_token = parsed
            .fragment()
            .and_then(|fragment| {
                url::form_urlencoded::parse(fragment.as_bytes())
                    .find(|(name, _)| name == "relay_token" || name == "token")
                    .map(|(_, value)| value.into_owned())
            })
            .filter(|token| !token.is_empty())
            .ok_or_else(|| translate("mobile-url-no-token"))?;
        let pairing_token = parsed
            .fragment()
            .and_then(|fragment| {
                url::form_urlencoded::parse(fragment.as_bytes())
                    .find(|(name, _)| name == "pairing_token" || name == "token")
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
            relay_token,
            credential: Some(ClientCredential::Pairing(pairing_token)),
            client_id: DeviceId::new(uuid::Uuid::new_v4().simple().to_string()),
            fingerprint,
            device,
        })
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

#[derive(Props, Clone, PartialEq)]
pub(crate) struct PairCardProps {
    pub(crate) value: String,
    pub(crate) error: String,
    pub(crate) pairing: bool,
    pub(crate) on_value: EventHandler<String>,
    pub(crate) on_pair: EventHandler<()>,
    pub(crate) on_scan: EventHandler<()>,
}

#[component]
pub(crate) fn PairCard(props: PairCardProps) -> Element {
    let unavailable = use_hook(|| match qr_scanner::ScannerSupport::detect() {
        qr_scanner::ScannerSupport::Available => None,
        qr_scanner::ScannerSupport::Unavailable(reason) => Some(reason),
    });
    let mut show_link = use_signal(|| unavailable.is_some() || !props.value.trim().is_empty());

    rsx! {
        div { class: "w-full",
            div { class: "mb-5 text-center",
                h2 { class: "text-base font-semibold text-foreground", {translate("mobile-pair-title")} }
                p { class: "mt-1 text-xs leading-5 text-muted-foreground", {translate("mobile-pair-subtitle")} }
            }
            button {
                class: "flex h-14 w-full items-center justify-center gap-2.5 rounded-2xl bg-primary text-sm font-semibold text-primary-foreground shadow-xl shadow-black/20 disabled:pointer-events-none disabled:opacity-40 disabled:shadow-none active:scale-[0.99] active:bg-primary/90",
                r#type: "button",
                disabled: unavailable.is_some(),
                onclick: move |_| props.on_scan.call(()),
                svg {
                    class: "h-5 w-5",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    path { d: "M3 5a2 2 0 0 1 2-2h2" }
                    path { d: "M17 3h2a2 2 0 0 1 2 2v2" }
                    path { d: "M21 17v2a2 2 0 0 1-2 2h-2" }
                    path { d: "M7 21H5a2 2 0 0 1-2-2v-2" }
                    rect { width: "5", height: "5", x: "7", y: "7", rx: "1" }
                    path { d: "M17 7v.01" }
                    path { d: "M17 12v5" }
                    path { d: "M12 17h5" }
                }
                {translate("mobile-pair-scan")}
            }
            button {
                class: "mx-auto mt-4 block rounded-lg px-3 py-2 text-xs font-medium text-muted-foreground active:bg-accent active:text-accent-foreground",
                r#type: "button",
                onclick: move |_| show_link.set(!show_link()),
                {if show_link() { translate("mobile-pair-hide-link") } else { translate("mobile-pair-show-link") }}
            }
            if let Some(reason) = unavailable.clone() {
                p { class: "mt-3 text-center text-xs leading-5 text-muted-foreground", "{reason}" }
            }
            if show_link() {
                form {
                    class: "mt-2 flex items-center gap-2 rounded-2xl border border-border bg-muted p-1.5",
                    onsubmit: move |event| {
                        event.prevent_default();
                        props.on_pair.call(());
                    },
                    input {
                        class: "h-10 min-w-0 flex-1 bg-transparent px-3 font-mono text-base text-foreground outline-none placeholder:text-muted-foreground",
                        r#type: "url",
                        autofocus: unavailable.is_some(),
                        inputmode: "url",
                        autocomplete: "off",
                        autocapitalize: "none",
                        placeholder: translate("mobile-pair-link-placeholder"),
                        value: "{props.value}",
                        oninput: move |event| props.on_value.call(event.value()),
                    }
                    button {
                        class: "flex h-10 shrink-0 items-center gap-2 rounded-xl bg-secondary px-4 text-xs font-semibold text-secondary-foreground disabled:opacity-70 active:bg-secondary/80",
                        r#type: "submit",
                        disabled: props.pairing,
                        if props.pairing {
                            span { class: "size-3.5 animate-spin rounded-full border-2 border-secondary-foreground/25 border-t-secondary-foreground motion-reduce:animate-none" }
                        }
                        {if props.pairing { translate("mobile-pair-connecting") } else { translate("mobile-pair-connect") }}
                    }
                }
            }
            if !props.error.is_empty() {
                p { class: "mt-3 rounded-xl border border-destructive/20 bg-destructive/[0.06] px-3 py-2 text-xs leading-5 text-destructive", "{props.error}" }
            }
        }
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
}
