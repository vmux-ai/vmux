pub use components::{Favicon, GlobeIcon};

pub struct FaviconSource;

impl FaviconSource {
    pub fn host(page_url: &str) -> Option<&str> {
        let source = page_url.trim();
        let rest = source
            .strip_prefix("https://")
            .or_else(|| source.strip_prefix("http://"))?;
        rest.split(&['/', '?', '#'][..])
            .next()
            .filter(|host| !host.is_empty())
    }

    fn agent_host(url: &str) -> Option<&'static str> {
        const AGENTS: &[(&str, &str)] = &[
            ("vibe", "chat.mistral.ai"),
            ("mistral-vibe", "chat.mistral.ai"),
            ("claude", "claude.ai"),
            ("claude-acp", "claude.ai"),
            ("codex", "chatgpt.com"),
            ("codex-acp", "chatgpt.com"),
            ("gemini", "gemini.google.com"),
        ];
        for &(kind, host) in AGENTS {
            for prefix in ["vmux://sessions/", "vmux://agent/"] {
                let base = format!("{prefix}{kind}");
                if url == base || url.starts_with(&format!("{base}/")) {
                    return Some(host);
                }
            }
        }
        None
    }

    pub fn vmux(url: &str) -> Option<String> {
        let host = url
            .trim()
            .strip_prefix("vmux://")?
            .split(&['/', '?', '#'][..])
            .next()
            .filter(|host| !host.is_empty())?;
        Some(format!("vmux://{host}/assets/favicons/{host}.svg"))
    }

    pub fn resolve(favicon_url: &str, url: &str) -> Option<String> {
        if let Some(host) = Self::agent_host(url) {
            return Some(format!(
                "https://www.google.com/s2/favicons?domain={host}&sz=64"
            ));
        }
        if !favicon_url.is_empty() {
            return Some(favicon_url.to_string());
        }
        if let Some(favicon) = Self::vmux(url) {
            return Some(favicon);
        }
        Self::host(url)
            .map(|host| format!("https://www.google.com/s2/favicons?domain={host}&sz=64"))
    }
}

mod components {
    use super::FaviconSource;
    use crate::components::icon::Icon;
    use crate::platform::Platform;
    use dioxus::prelude::*;

    const DEFAULT_FAVICON_CLASS: &str = "h-4 w-4 shrink-0 rounded-sm object-contain";
    const DEFAULT_GLOBE_CLASS: &str = "h-4 w-4 shrink-0 text-muted-foreground";
    const FALLBACK_DELAY_MS: u32 = 500;

    #[component]
    pub fn Favicon(
        favicon_url: String,
        url: String,
        class: Option<String>,
        globe_class: Option<String>,
    ) -> Element {
        let img_class = class.unwrap_or_else(|| DEFAULT_FAVICON_CLASS.to_string());
        let globe_class = globe_class.unwrap_or_else(|| DEFAULT_GLOBE_CLASS.to_string());
        let src = FaviconSource::resolve(&favicon_url, &url);
        let source_key = src.clone().unwrap_or_default();
        let mut fallback_for = use_signal(|| None::<String>);
        let mut failed_for = use_signal(|| None::<String>);
        let mut generation = use_signal(|| 0_u32);
        use_effect(use_reactive!(|(favicon_url, url)| {
            let source = FaviconSource::resolve(&favicon_url, &url);
            let key = source.clone().unwrap_or_default();
            let next = generation.peek().wrapping_add(1);
            generation.set(next);
            fallback_for.set(None);
            failed_for.set(None);
            if source.is_none() {
                spawn(async move {
                    Platform::sleep(FALLBACK_DELAY_MS).await;
                    if generation() == next {
                        fallback_for.set(Some(key));
                    }
                });
            }
        }));
        let show_fallback = fallback_for().as_deref() == Some(source_key.as_str());
        let source_failed = failed_for().as_deref() == Some(source_key.as_str());
        rsx! {
            if let Some(src) = src.as_ref() {
                if source_failed {
                    if show_fallback {
                        GlobeIcon { class: globe_class }
                    } else {
                        span { class: "{globe_class} opacity-0" }
                    }
                } else {
                    img {
                        class: "{img_class}",
                        src: "{src}",
                        draggable: "false",
                        onerror: {
                            let key = source_key.clone();
                            move |_| {
                                failed_for.set(Some(key.clone()));
                                fallback_for.set(None);
                                let next = generation.peek().wrapping_add(1);
                                generation.set(next);
                                let key = key.clone();
                                spawn(async move {
                                    Platform::sleep(FALLBACK_DELAY_MS).await;
                                    if generation() == next {
                                        fallback_for.set(Some(key));
                                    }
                                });
                            }
                        },
                    }
                }
            } else if show_fallback {
                GlobeIcon { class: globe_class }
            } else {
                span { class: "{globe_class} opacity-0" }
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
            FaviconSource::host("https://example.com/path"),
            Some("example.com")
        );
    }

    #[test]
    fn host_extracted_from_http_url() {
        assert_eq!(
            FaviconSource::host("http://example.com/"),
            Some("example.com")
        );
    }

    #[test]
    fn host_extracted_when_query_string_present() {
        assert_eq!(
            FaviconSource::host("https://www.google.com/search?q=mistral.ai"),
            Some("www.google.com")
        );
    }

    #[test]
    fn host_extracted_when_fragment_present() {
        assert_eq!(
            FaviconSource::host("https://example.com#frag"),
            Some("example.com")
        );
    }

    #[test]
    fn host_none_for_non_http_scheme() {
        assert_eq!(FaviconSource::host("vmux://history/"), None);
        assert_eq!(FaviconSource::host("ftp://example.com"), None);
        assert_eq!(FaviconSource::host(""), None);
    }

    #[test]
    fn host_none_when_empty_after_scheme() {
        assert_eq!(FaviconSource::host("https://"), None);
    }

    #[test]
    fn agent_host_maps_vibe() {
        assert_eq!(
            FaviconSource::agent_host("vmux://sessions/vibe/chat/abc"),
            Some("chat.mistral.ai")
        );
        assert_eq!(
            FaviconSource::agent_host("vmux://sessions/vibe/cli/abc"),
            Some("chat.mistral.ai")
        );
        assert_eq!(
            FaviconSource::agent_host("vmux://sessions/mistral-vibe/session-1"),
            Some("chat.mistral.ai")
        );
    }

    #[test]
    fn agent_host_maps_claude_and_codex() {
        assert_eq!(
            FaviconSource::agent_host("vmux://sessions/claude/x"),
            Some("claude.ai")
        );
        assert_eq!(
            FaviconSource::agent_host("vmux://sessions/claude-acp/x"),
            Some("claude.ai")
        );
        assert_eq!(
            FaviconSource::agent_host("vmux://sessions/codex/x"),
            Some("chatgpt.com")
        );
        assert_eq!(
            FaviconSource::agent_host("vmux://sessions/codex-acp/x"),
            Some("chatgpt.com")
        );
    }

    #[test]
    fn agent_host_unknown_returns_none() {
        assert_eq!(FaviconSource::agent_host("vmux://sessions/unknown/x"), None);
        assert_eq!(FaviconSource::agent_host("https://example.com"), None);
    }

    #[test]
    fn favicon_src_returns_real_when_present() {
        assert_eq!(
            FaviconSource::resolve("https://cdn.example.com/icon.png", "https://example.com/"),
            Some("https://cdn.example.com/icon.png".to_string())
        );
    }

    #[test]
    fn favicon_src_prefers_agent_host_over_passed_icon() {
        assert_eq!(
            FaviconSource::resolve(
                "https://cdn.example/claude-acp.svg",
                "vmux://sessions/claude"
            ),
            Some("https://www.google.com/s2/favicons?domain=claude.ai&sz=64".to_string())
        );
    }

    #[test]
    fn favicon_src_falls_back_to_google_s2_for_http_url() {
        assert_eq!(
            FaviconSource::resolve("", "https://mistral.ai/"),
            Some("https://www.google.com/s2/favicons?domain=mistral.ai&sz=64".to_string())
        );
    }

    #[test]
    fn favicon_src_falls_back_to_google_s2_for_google_search() {
        assert_eq!(
            FaviconSource::resolve("", "https://www.google.com/search?q=mistral.ai"),
            Some("https://www.google.com/s2/favicons?domain=www.google.com&sz=64".to_string())
        );
    }

    #[test]
    fn favicon_src_falls_back_to_agent_host() {
        assert_eq!(
            FaviconSource::resolve("", "vmux://sessions/vibe/chat/abc"),
            Some("https://www.google.com/s2/favicons?domain=chat.mistral.ai&sz=64".to_string())
        );
    }

    #[test]
    fn favicon_src_uses_the_page_host_for_vmux_pages() {
        assert_eq!(
            FaviconSource::resolve("", "vmux://history/"),
            Some("vmux://history/assets/favicons/history.svg".to_string())
        );
    }

    #[test]
    fn favicon_src_none_for_an_empty_url() {
        assert_eq!(FaviconSource::resolve("", ""), None);
    }

    #[test]
    fn agent_host_matches_single_segment_acp_url() {
        assert_eq!(
            FaviconSource::agent_host("vmux://sessions/claude"),
            Some("claude.ai")
        );
        assert_eq!(
            FaviconSource::agent_host("vmux://sessions/codex"),
            Some("chatgpt.com")
        );
        assert_eq!(
            FaviconSource::agent_host("vmux://sessions/claude/cli"),
            Some("claude.ai")
        );
    }

    #[test]
    fn agent_host_maps_gemini() {
        assert_eq!(
            FaviconSource::agent_host("vmux://sessions/gemini"),
            Some("gemini.google.com")
        );
    }

    #[test]
    fn agent_host_does_not_over_match_similar_ids() {
        assert_eq!(FaviconSource::agent_host("vmux://sessions/claudex"), None);
    }
}
