//! Host → WASM history payload.

use dioxus::prelude::*;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, Default)]
pub struct HistoryEntryPayload {
    pub url: Option<String>,
    pub visited_at_ms: Option<i64>,
    #[serde(default)]
    pub favicon_url: Option<String>,
    #[serde(default)]
    pub favicon_cached_at_ms: Option<i64>,
}

/// One row for the history list (WASM UI).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryEntryWire {
    pub url: String,
    pub visited_at_ms: i64,
    pub favicon_url: Option<String>,
    pub favicon_cached_at_ms: Option<i64>,
}

fn default_history_stream_done() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct VmuxHistoryPayload {
    pub entries: Option<Vec<HistoryEntryPayload>>,
    /// Echoed from host (`HistoryHostPayload`); reserved for stricter ack logic.
    #[serde(default)]
    #[allow(dead_code)]
    pub sync_nonce: Option<u32>,
    /// Continuation slice after the first host emit (same order: newest → older).
    #[serde(default)]
    pub history_stream_append: bool,
    #[serde(default = "default_history_stream_done")]
    pub history_stream_done: bool,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum BridgeMsg {
    History { payload: serde_json::Value },
}

pub fn apply_history_payload(
    raw: serde_json::Value,
    mut entries: Signal<Vec<HistoryEntryWire>>,
    mut bridge_sync_pending: Signal<Option<u32>>,
    mut host_snapshot_received: Signal<bool>,
    mut history_stream_complete: Signal<bool>,
) {
    let p: VmuxHistoryPayload = match raw {
        serde_json::Value::String(s) => serde_json::from_str(&s).unwrap_or_default(),
        v => serde_json::from_value(v).unwrap_or_default(),
    };
    let Some(list) = p.entries else {
        return;
    };
    let append = p.history_stream_append;
    let out: Vec<HistoryEntryWire> = list
        .into_iter()
        .filter_map(|e| {
            let u = e.url.as_ref()?.trim();
            if u.is_empty() {
                return None;
            }
            Some(HistoryEntryWire {
                url: u.to_string(),
                visited_at_ms: e.visited_at_ms.unwrap_or(0),
                favicon_url: e.favicon_url.clone(),
                favicon_cached_at_ms: e.favicon_cached_at_ms,
            })
        })
        .collect();
    if append {
        let mut cur = entries();
        cur.extend(out);
        entries.set(cur);
    } else {
        entries.set(out);
    }
    // After entries so a render never sees "not loading" with stale empty rows.
    host_snapshot_received.set(true);
    // Stop resync retries on the first chunk (nonce only applies there).
    if !append {
        bridge_sync_pending.set(None);
    }
    if !append {
        history_stream_complete.set(p.history_stream_done);
    } else if p.history_stream_done {
        history_stream_complete.set(true);
    }
}
