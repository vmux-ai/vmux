//! Host → WASM status payload and display helpers.

use dioxus::prelude::*;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, Default)]
pub struct VmuxStatusPayload {
    pub user: Option<String>,
    pub host: Option<String>,
    pub active_url: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum BridgeMsg {
    Clock { text: String },
    Status { payload: serde_json::Value },
}

fn host_for_display(url: &str) -> Option<String> {
    let u = url::Url::parse(url).ok()?;
    let host = u.host_str()?;
    let h = host.strip_prefix("www.").unwrap_or(host);
    Some(if h.chars().count() > 40 {
        format!("{}…", h.chars().take(38).collect::<String>())
    } else {
        h.to_string()
    })
}

fn window_label(url: &str) -> String {
    host_for_display(url)
        .map(|h| format!("0:{h}*"))
        .unwrap_or_else(|| "0:web*".to_string())
}

fn user_host_line(user: &str, host: &str) -> String {
    match (!user.is_empty(), !host.is_empty()) {
        (true, true) => format!("{user}@{host}"),
        (true, false) => user.to_string(),
        _ if !host.is_empty() => host.to_string(),
        _ => String::new(),
    }
}

pub fn apply_payload(
    raw: serde_json::Value,
    mut user_host: Signal<String>,
    mut win_label: Signal<String>,
) {
    let p: VmuxStatusPayload = match raw {
        serde_json::Value::String(s) => {
            serde_json::from_str(&s).unwrap_or_else(|_| VmuxStatusPayload {
                active_url: Some(s),
                ..Default::default()
            })
        }
        v => serde_json::from_value(v).unwrap_or_default(),
    };

    let empty = |o: &Option<String>| o.as_ref().map_or(true, |s| s.is_empty());
    if empty(&p.user) && empty(&p.host) && empty(&p.active_url) {
        return;
    }

    let u = p.user.as_deref().unwrap_or("");
    let h = p.host.as_deref().unwrap_or("");
    user_host.set(user_host_line(u, h));

    let url = p.active_url.as_deref().unwrap_or("");
    win_label.set(window_label(url));
}
