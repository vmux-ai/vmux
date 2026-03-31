//! URL scheme checks for persisted / navigable strings.

use crate::LastVisitedUrl;

const MAX_URL_LEN: usize = 4096;

/// YouTube embeds often fail in embedded CEF (e.g. error 153). Treat them as unusable for
/// main-frame navigation and session restore.
fn url_host_is_youtube_embed_problematic(url: &str) -> bool {
    let url = url.trim();
    let rest = if let Some(after) = url.strip_prefix("https://") {
        after
    } else if let Some(after) = url.strip_prefix("http://") {
        after
    } else {
        return false;
    };
    let host = rest.split('/').next().unwrap_or("");
    let host = host.split(':').next().unwrap_or(host).to_ascii_lowercase();
    host == "youtu.be"
        || host.ends_with(".youtu.be")
        || host == "youtube.com"
        || host.ends_with(".youtube.com")
        || host == "youtube-nocookie.com"
        || host.ends_with(".youtube-nocookie.com")
}

/// Replace `url` with `fallback` when the host is known to break embeddable CEF webviews.
pub fn sanitize_embedded_webview_url(url: &str, fallback: &str) -> String {
    let u = url.trim();
    if u.is_empty() {
        return fallback.to_string();
    }
    if url_host_is_youtube_embed_problematic(u) {
        return fallback.to_string();
    }
    u.to_string()
}

/// Allow only navigable schemes for persisted URLs.
pub fn allowed_navigation_url(url: &str) -> bool {
    let url = url.trim();
    if url.is_empty() || url.len() > MAX_URL_LEN {
        return false;
    }
    let Some((scheme, _)) = url.split_once(':') else {
        return false;
    };
    matches!(
        scheme.to_ascii_lowercase().as_str(),
        "http" | "https" | "cef"
    )
}

/// True when `url` is almost certainly a **previous run's** embedded history UI base URL
/// (`http://127.0.0.1:<ephemeral>/…`), which becomes invalid after restart.
///
/// Used to recover old session files that did not persist [`SavedLayoutNode::Leaf::history_pane`](crate::SavedLayoutNode::Leaf).
pub fn legacy_loopback_embedded_history_ui_url(url: &str) -> bool {
    let u = url.trim();
    let rest = if let Some(r) = u.strip_prefix("http://127.0.0.1:") {
        r
    } else if let Some(r) = u.strip_prefix("http://localhost:") {
        r
    } else {
        return false;
    };
    let port: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    if port.is_empty() {
        return false;
    }
    let after_port = &rest[port.len()..];
    let after_port = after_port.to_ascii_lowercase();
    after_port.is_empty()
        || after_port == "/"
        || after_port == "/index.html"
}

/// Initial `WebviewSource` URL: last session if valid, else `fallback`.
pub fn initial_webview_url(last: Option<&LastVisitedUrl>, fallback: &str) -> String {
    let Some(last) = last else {
        return fallback.to_string();
    };
    let u = last.0.trim();
    if u.is_empty() || !allowed_navigation_url(u) {
        fallback.to_string()
    } else {
        sanitize_embedded_webview_url(u, fallback)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_history_ui_loopback_detected() {
        assert!(legacy_loopback_embedded_history_ui_url(
            "http://127.0.0.1:54321/"
        ));
        assert!(legacy_loopback_embedded_history_ui_url(
            "http://127.0.0.1:54321/index.html"
        ));
        assert!(legacy_loopback_embedded_history_ui_url("http://localhost:8080/"));
        assert!(!legacy_loopback_embedded_history_ui_url(
            "http://127.0.0.1:3000/app"
        ));
        assert!(!legacy_loopback_embedded_history_ui_url("https://example.com/"));
    }

    #[test]
    fn youtube_urls_replaced_with_fallback() {
        let fb = "https://www.google.com";
        assert_eq!(
            sanitize_embedded_webview_url("https://www.youtube.com/watch?v=1", fb),
            fb
        );
        assert_eq!(
            sanitize_embedded_webview_url("https://youtu.be/abc", fb),
            fb
        );
        assert_eq!(
            sanitize_embedded_webview_url("https://www.google.com/", fb),
            "https://www.google.com/"
        );
    }
}
