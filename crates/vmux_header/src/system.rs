use bevy::prelude::*;
use bevy_cef::prelude::*;

pub const HEADER_HEIGHT_PX: f32 = 40.0;

#[derive(Component, Clone, Debug, Reflect, Default)]
#[reflect(Component)]
pub struct PageMetadata {
    pub title: String,
    pub url: String,
    pub favicon_url: String,
}

#[derive(Component, Clone, Debug, Reflect, Default)]
#[reflect(Component)]
pub struct NavigationState {
    pub can_go_back: bool,
    pub can_go_forward: bool,
}

pub fn apply_chrome_state_from_cef(
    chrome_rx: Res<WebviewChromeStateReceiver>,
    mut browser_meta: Query<&mut PageMetadata>,
) {
    while let Ok(ev) = chrome_rx.0.try_recv() {
        let Ok(mut meta) = browser_meta.get_mut(ev.webview) else {
            continue;
        };
        if let Some(url) = ev.url {
            // Don't let CEF overwrite terminal session URLs (managed by terminal.rs)
            if !meta.url.starts_with("vmux://terminal") {
                meta.url = url;
                meta.favicon_url.clear();
            }
        }
        if let Some(title) = ev.title {
            meta.title = title;
        }
        if let Some(favicon) = ev.favicon_url {
            meta.favicon_url = favicon;
        }
    }
}
