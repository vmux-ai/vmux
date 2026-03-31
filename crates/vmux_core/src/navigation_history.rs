//! Visited URLs for session-adjacent persistence and history UI (not CEF back/forward).

use std::net::IpAddr;
use std::time::{SystemTime, UNIX_EPOCH};

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Host segment for [`favicon_url_for_page_url`] (skips `data:`, `about:`, localhost, bare IPs).
pub fn page_host_for_favicon_url(url: &str) -> Option<String> {
    let t = url.trim();
    if t.is_empty() || t.starts_with("data:") || t.starts_with("about:") {
        return None;
    }
    let rest = t
        .strip_prefix("https://")
        .or_else(|| t.strip_prefix("http://"))
        .unwrap_or("");
    let host_end = rest.find('/').unwrap_or(rest.len());
    let host = rest[..host_end].rsplit('@').next().unwrap_or("").trim();
    if host.is_empty() {
        return None;
    }
    let host = if let Some(inner) = host.strip_prefix('[').and_then(|h| h.strip_suffix(']')) {
        inner
    } else {
        host
    };
    if host.eq_ignore_ascii_case("localhost") {
        return None;
    }
    if host.parse::<IpAddr>().is_ok() {
        return None;
    }
    Some(host.to_ascii_lowercase())
}

/// PNG favicon URL for Bevy/UI (`WebAssetReader` needs a direct image URL; matches command palette).
pub fn favicon_url_for_page_url(url: &str) -> Option<String> {
    let host = page_host_for_favicon_url(url)?;
    Some(format!(
        "https://t3.gstatic.com/faviconV2?client=SOCIAL&type=FAVICON&fallback_opts=TYPE,SIZE,URL&url=http://{host}/&size=32"
    ))
}

/// One visit record (newest entries are stored at the front of [`NavigationHistory::entries`]).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NavigationHistoryEntry {
    pub url: String,
    /// Unix time in milliseconds.
    pub visited_at_ms: i64,
    /// Cached favicon asset URL (same scheme as command palette / gstatic PNG).
    #[serde(default)]
    pub favicon_url: Option<String>,
    /// When [`Self::favicon_url`] was recorded (ms since Unix epoch).
    #[serde(default)]
    pub favicon_cached_at_ms: Option<i64>,
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Bounded, deduped visit list (resource). Persisted to disk by `vmux_session`.
#[derive(Resource, Debug, Clone, Default)]
pub struct NavigationHistory {
    pub entries: Vec<NavigationHistoryEntry>,
    /// Incremented when `entries` changes; host UI can compare to avoid redundant `HostEmitEvent`.
    pub revision: u64,
}

impl NavigationHistory {
    /// Cap for RAM, disk (`navigation_history.ron`), and host→history UI payloads.
    pub const MAX_ENTRIES: usize = 5000;

    /// Append a visit if it differs from the most recent URL. Returns `true` if storage changed.
    pub fn push_visit(&mut self, url: String) -> bool {
        let url = url.trim().to_string();
        if url.is_empty() {
            return false;
        }
        if self.entries.first().is_some_and(|e| e.url == url) {
            return false;
        }
        let now = now_ms();
        let favicon_url = favicon_url_for_page_url(&url);
        let favicon_cached_at_ms = favicon_url.as_ref().map(|_| now);
        self.entries.insert(
            0,
            NavigationHistoryEntry {
                url,
                visited_at_ms: now,
                favicon_url,
                favicon_cached_at_ms,
            },
        );
        self.entries.truncate(Self::MAX_ENTRIES);
        self.revision = self.revision.wrapping_add(1);
        true
    }

    /// Remove all visits. Returns `true` if storage changed.
    pub fn clear(&mut self) -> bool {
        if self.entries.is_empty() {
            return false;
        }
        self.entries.clear();
        self.revision = self.revision.wrapping_add(1);
        true
    }
}

/// File payload for `navigation_history.ron`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NavigationHistoryFile {
    pub entries: Vec<NavigationHistoryEntry>,
}

impl From<&NavigationHistory> for NavigationHistoryFile {
    fn from(h: &NavigationHistory) -> Self {
        Self {
            entries: h.entries.clone(),
        }
    }
}

impl From<NavigationHistoryFile> for NavigationHistory {
    fn from(f: NavigationHistoryFile) -> Self {
        let mut entries: Vec<NavigationHistoryEntry> = f
            .entries
            .into_iter()
            .map(|mut e| {
                if e.favicon_url.is_none() {
                    e.favicon_url = favicon_url_for_page_url(&e.url);
                    e.favicon_cached_at_ms = e.favicon_url.as_ref().map(|_| e.visited_at_ms);
                }
                e
            })
            .collect();
        entries.truncate(Self::MAX_ENTRIES);
        Self {
            entries,
            revision: 1,
        }
    }
}
