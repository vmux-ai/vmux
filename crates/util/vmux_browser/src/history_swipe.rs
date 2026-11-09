use bevy::prelude::*;
use bevy_cef::prelude::{BinIpcBlocked, Browsers, HistorySwipeEventReceiver};
use bevy_cef_core::prelude::{HistorySwipeDirection, HistorySwipeRaw};
use std::sync::atomic::{AtomicU64, Ordering};
use vmux_command::ReadCommandRequests;
use vmux_page::HistorySwipe;

use crate::{NavigationOperation, NavigationRequest, WebviewLoadCompleted};

pub(crate) struct HistorySwipePlugin;

impl Plugin for HistorySwipePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(crate::native_bridge::NativeHistoryPlugin)
            .add_systems(
                Update,
                sync.after(crate::BrowserLoadSet)
                    .before(ReadCommandRequests),
            );
    }
}

struct Installation {
    token: String,
}

impl Installation {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let serial = NEXT.fetch_add(1, Ordering::Relaxed);
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;
        Self {
            token: format!("{timestamp:016x}{serial:016x}"),
        }
    }

    fn script(&self) -> String {
        let recognizer = HistorySwipe::script("emit(direction, token);");
        format!(
            "(() => {{ const emit = window.cef?.historySwipe; if (typeof emit !== 'function') return; const token = '{}'; {recognizer} }})();",
            self.token,
        )
    }

    fn accepts(&self, event: &HistorySwipeRaw, current_url: &str) -> bool {
        self.token == event.token && event.page_url == current_url
    }
}

fn sync(
    mut navigated: MessageReader<bevy_cef_core::prelude::WebviewCommittedNavigationEvent>,
    mut loaded: MessageReader<WebviewLoadCompleted>,
    receiver: Res<HistorySwipeEventReceiver>,
    browsers: NonSend<Browsers>,
    mut installations: Local<std::collections::HashMap<Entity, Installation>>,
    blocked: Query<
        (),
        Or<(
            With<BinIpcBlocked>,
            With<vmux_layout::hosted_page::SurfaceBlocked>,
        )>,
    >,
    mut requests: MessageWriter<NavigationRequest>,
) {
    for event in navigated.read() {
        if event.is_main_frame {
            crate::NativeBridge::invalidate_history_input(event.webview);
            installations.remove(&event.webview);
        }
    }
    installations.retain(|webview, _| browsers.has_browser(*webview));
    for event in loaded.read() {
        crate::NativeBridge::invalidate_history_input(event.webview);
        let Some(url) = browsers.page_url(&event.webview) else {
            continue;
        };
        if !url.starts_with("http://") && !url.starts_with("https://") {
            continue;
        }
        let installation = Installation::new();
        browsers.execute_js(&event.webview, &installation.script());
        installations.insert(event.webview, installation);
    }
    while let Ok(event) = receiver.0.try_recv() {
        if blocked.contains(event.webview) {
            continue;
        }
        let Some(installation) = installations.get(&event.webview) else {
            continue;
        };
        let Some(url) = browsers.page_url(&event.webview) else {
            continue;
        };
        if !installation.accepts(&event, &url) {
            continue;
        }
        let operation = match event.direction {
            HistorySwipeDirection::Back => NavigationOperation::Back,
            HistorySwipeDirection::Forward => NavigationOperation::Forward,
        };
        if !crate::NativeBridge::claim_history_input(event.webview, operation) {
            continue;
        }
        requests.write(NavigationRequest::surface(event.webview, operation));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_previous_document_cannot_reuse_a_history_notification() {
        let installation = Installation::new();
        let mut event = HistorySwipeRaw {
            webview: Entity::PLACEHOLDER,
            page_url: "https://example.com/".to_string(),
            token: installation.token.clone(),
            direction: HistorySwipeDirection::Back,
        };
        assert!(installation.accepts(&event, "https://example.com/"));
        assert!(!installation.accepts(&event, "https://example.com/next"));
        let next = Installation::new();
        assert!(!next.accepts(&event, "https://example.com/"));
        event.page_url = "https://other.example.com/".to_string();
        assert!(!installation.accepts(&event, "https://example.com/"));
        event.page_url = "https://example.com/next#section".to_string();
        assert!(installation.accepts(&event, "https://example.com/next#section"));
    }
}
