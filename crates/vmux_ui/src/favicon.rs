//! Favicon URL resolution with multi-tier fallback.
//!
//! Pure helpers ([`favicon_src_for_url`] and friends) and the [`Favicon`] and [`GlobeIcon`]
//! components all work on any target.

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
        ("mistral-vibe", "chat.mistral.ai"),
        ("claude", "claude.ai"),
        ("claude-acp", "claude.ai"),
        ("codex", "chatgpt.com"),
        ("codex-acp", "chatgpt.com"),
        ("gemini", "gemini.google.com"),
    ];
    for &(kind, host) in AGENTS {
        let base = format!("vmux://agent/{kind}");
        if url == base || url.starts_with(&format!("{base}/")) {
            return Some(host);
        }
    }
    None
}

pub fn favicon_src_for_url(favicon_url: &str, url: &str) -> Option<String> {
    // Agent pages: prefer the recognizable brand favicon (claude.ai / chatgpt.com / …) over any
    // passed icon, so an agent reads the same across tab, chat, roster, facepile, and launcher.
    // Only `vmux://agent/<known>` urls match here; the passed icon (e.g. a registry icon) still
    // serves unknown agents and real web pages below.
    if let Some(host) = agent_host(url) {
        return Some(format!(
            "https://www.google.com/s2/favicons?domain={host}&sz=64"
        ));
    }
    if !favicon_url.is_empty() {
        return Some(favicon_url.to_string());
    }
    host_for_favicon_fallback(url)
        .map(|h| format!("https://www.google.com/s2/favicons?domain={h}&sz=64"))
}

pub use components::{Favicon, GlobeIcon};

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
#[path = "favicon.test.rs"]
mod tests;
