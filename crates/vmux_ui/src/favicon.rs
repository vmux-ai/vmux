//! Favicon URL resolution with multi-tier fallback.
//!
//! Pure helpers ([`favicon_src_for_url`] and friends) work on any target. The
//! [`Favicon`] and [`GlobeIcon`] components are wasm-only.

pub fn host_for_favicon_fallback(page_url: &str) -> Option<&str> {
    let s = page_url.trim();
    let rest = s
        .strip_prefix("https://")
        .or_else(|| s.strip_prefix("http://"))?;
    rest.split(&['/', '?', '#'][..])
        .next()
        .filter(|h| !h.is_empty())
}

pub fn agent_host(url: &str) -> Option<&'static str> {
    const AGENTS: &[(&str, &str)] = &[
        ("vibe", "chat.mistral.ai"),
        ("claude", "claude.ai"),
        ("codex", "chatgpt.com"),
    ];
    for &(kind, host) in AGENTS {
        if url.starts_with(&format!("vmux://agent/{kind}/cli/"))
            || url.starts_with(&format!("vmux://agent/{kind}/"))
        {
            return Some(host);
        }
    }
    None
}

pub fn favicon_src_for_url(favicon_url: &str, url: &str) -> Option<String> {
    if !favicon_url.is_empty() {
        return Some(favicon_url.to_string());
    }
    if let Some(host) = agent_host(url) {
        return Some(format!(
            "https://www.google.com/s2/favicons?domain={host}&sz=32"
        ));
    }
    host_for_favicon_fallback(url)
        .map(|h| format!("https://www.google.com/s2/favicons?domain={h}&sz=32"))
}

#[cfg(target_arch = "wasm32")]
pub use components::{Favicon, GlobeIcon};

#[cfg(target_arch = "wasm32")]
mod components {
    use super::favicon_src_for_url;
    use crate::components::icon::Icon;
    use dioxus::prelude::*;

    const DEFAULT_FAVICON_CLASS: &str = "h-4 w-4 shrink-0 rounded-sm object-contain";
    const DEFAULT_GLOBE_CLASS: &str = "h-4 w-4 shrink-0 text-muted-foreground";

    #[component]
    pub fn Favicon(
        favicon_url: String,
        url: String,
        class: Option<String>,
        globe_class: Option<String>,
    ) -> Element {
        let img_class = class.unwrap_or_else(|| DEFAULT_FAVICON_CLASS.to_string());
        let globe_class = globe_class.unwrap_or_else(|| DEFAULT_GLOBE_CLASS.to_string());
        let mut errored = use_signal(|| false);
        let mut prev_src = use_signal(|| None::<String>);
        let src = favicon_src_for_url(&favicon_url, &url);
        if *prev_src.read() != src {
            prev_src.set(src.clone());
            errored.set(false);
        }
        rsx! {
            if let Some(src) = src.as_ref() {
                if errored() {
                    GlobeIcon { class: globe_class }
                } else {
                    img {
                        class: "{img_class}",
                        src: "{src}",
                        onerror: move |_| errored.set(true),
                    }
                }
            } else {
                GlobeIcon { class: globe_class }
            }
        }
    }

    #[component]
    pub fn GlobeIcon(class: Option<String>) -> Element {
        let class = class.unwrap_or_else(|| DEFAULT_GLOBE_CLASS.to_string());
        rsx! {
            Icon { class: "{class}",
                path { d: "M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20Z" }
                path { d: "M2 12h20" }
                path { d: "M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10Z" }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_extracted_from_https_url() {
        assert_eq!(
            host_for_favicon_fallback("https://example.com/path"),
            Some("example.com")
        );
    }

    #[test]
    fn host_extracted_from_http_url() {
        assert_eq!(
            host_for_favicon_fallback("http://example.com/"),
            Some("example.com")
        );
    }

    #[test]
    fn host_extracted_when_query_string_present() {
        assert_eq!(
            host_for_favicon_fallback("https://www.google.com/search?q=mistral.ai"),
            Some("www.google.com")
        );
    }

    #[test]
    fn host_extracted_when_fragment_present() {
        assert_eq!(
            host_for_favicon_fallback("https://example.com#frag"),
            Some("example.com")
        );
    }

    #[test]
    fn host_none_for_non_http_scheme() {
        assert_eq!(host_for_favicon_fallback("vmux://history/"), None);
        assert_eq!(host_for_favicon_fallback("ftp://example.com"), None);
        assert_eq!(host_for_favicon_fallback(""), None);
    }

    #[test]
    fn host_none_when_empty_after_scheme() {
        assert_eq!(host_for_favicon_fallback("https://"), None);
    }

    #[test]
    fn agent_host_maps_vibe() {
        assert_eq!(
            agent_host("vmux://agent/vibe/chat/abc"),
            Some("chat.mistral.ai")
        );
        assert_eq!(
            agent_host("vmux://agent/vibe/cli/abc"),
            Some("chat.mistral.ai")
        );
    }

    #[test]
    fn agent_host_maps_claude_and_codex() {
        assert_eq!(agent_host("vmux://agent/claude/x"), Some("claude.ai"));
        assert_eq!(agent_host("vmux://agent/codex/x"), Some("chatgpt.com"));
    }

    #[test]
    fn agent_host_unknown_returns_none() {
        assert_eq!(agent_host("vmux://agent/unknown/x"), None);
        assert_eq!(agent_host("https://example.com"), None);
    }

    #[test]
    fn favicon_src_returns_real_when_present() {
        assert_eq!(
            favicon_src_for_url("https://cdn.example.com/icon.png", "https://example.com/"),
            Some("https://cdn.example.com/icon.png".to_string())
        );
    }

    #[test]
    fn favicon_src_falls_back_to_google_s2_for_http_url() {
        assert_eq!(
            favicon_src_for_url("", "https://mistral.ai/"),
            Some("https://www.google.com/s2/favicons?domain=mistral.ai&sz=32".to_string())
        );
    }

    #[test]
    fn favicon_src_falls_back_to_google_s2_for_google_search() {
        assert_eq!(
            favicon_src_for_url("", "https://www.google.com/search?q=mistral.ai"),
            Some("https://www.google.com/s2/favicons?domain=www.google.com&sz=32".to_string())
        );
    }

    #[test]
    fn favicon_src_falls_back_to_agent_host() {
        assert_eq!(
            favicon_src_for_url("", "vmux://agent/vibe/chat/abc"),
            Some("https://www.google.com/s2/favicons?domain=chat.mistral.ai&sz=32".to_string())
        );
    }

    #[test]
    fn favicon_src_none_for_vmux_scheme_without_agent() {
        assert_eq!(favicon_src_for_url("", "vmux://history/"), None);
        assert_eq!(favicon_src_for_url("", ""), None);
    }
}
