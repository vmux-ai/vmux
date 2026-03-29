//! Shared scene markers and constants for `vmux` and `vmux_webview`.

use bevy::prelude::*;
use serde::Deserialize;

/// Z distance of the world camera from the webview plane at z = 0 (used for frustum sizing).
pub const CAMERA_DISTANCE: f32 = 3.0;

/// Marker for the vmux world-facing camera used to size the webview plane.
#[derive(Component)]
pub struct VmuxWorldCamera;

const MAX_URL_LEN: usize = 4096;

/// Last document URL for the primary webview; persisted with moonshine-save (see `vmux` crate).
#[derive(Resource, Default, Clone, Reflect)]
#[reflect(Resource)]
pub struct LastVisitedUrl(pub String);

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

/// Initial `WebviewSource` URL: last session if valid, else `fallback`.
pub fn initial_webview_url(last: Option<&LastVisitedUrl>, fallback: &str) -> String {
    let Some(last) = last else {
        return fallback.to_string();
    };
    let u = last.0.trim();
    if u.is_empty() || !allowed_navigation_url(u) {
        fallback.to_string()
    } else {
        u.to_string()
    }
}

/// Payload from `window.cef.emit({ url })` (single-arg form matches bevy_cef IPC).
#[derive(Debug, Clone, Deserialize)]
pub struct WebviewDocumentUrlEmit {
    pub url: String,
}
