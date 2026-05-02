//! Generic anti-flash reveal for newly-spawned webviews.
//!
//! When a `WebviewSource` is added to an entity, hide it via
//! `Visibility::Hidden` and start a frame counter. After a few frames
//! Bevy's UI layout has run and bevy_cef has resized the underlying CEF
//! webview.

use bevy::{prelude::*, ui::UiSystems};
use bevy_cef::prelude::WebviewSource;
use bevy_cef_core::prelude::webview_debug_log;

use crate::layout::window::VmuxWindow;

pub(crate) struct WebviewRevealPlugin;

impl Plugin for WebviewRevealPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(on_webview_added)
            .add_systems(PostUpdate, reveal_webviews.after(UiSystems::Layout));
    }
}

/// Frame counter for a hidden webview waiting to be revealed.
#[derive(Component)]
pub(crate) struct PendingWebviewReveal(u8);

/// Number of frames to wait before revealing a freshly spawned webview.
/// 2 frames lets Bevy UI layout + bevy_cef resize the CEF surface so the
/// first visible paint is at the correct size.
const REVEAL_FRAMES: u8 = 2;

fn on_webview_added(
    trigger: On<Add, WebviewSource>,
    root: Query<(), With<VmuxWindow>>,
    mut commands: Commands,
) {
    let entity = trigger.event_target();
    // Don't hide the root window's own webview surface.
    if root.contains(entity) {
        return;
    }
    webview_debug_log(format!("webview added entity={entity:?}"));
    commands
        .entity(entity)
        .insert((Visibility::Hidden, PendingWebviewReveal(0)));
}

fn reveal_webviews(
    mut commands: Commands,
    mut query: Query<(
        Entity,
        &WebviewSource,
        &mut Visibility,
        &mut PendingWebviewReveal,
    )>,
) {
    for (entity, source, mut vis, mut pending) in &mut query {
        if webview_reveal_ready(source, false, pending.0) {
            *vis = Visibility::Inherited;
            commands.entity(entity).remove::<PendingWebviewReveal>();
            webview_debug_log(format!(
                "webview reveal entity={entity:?} source={source:?}"
            ));
        } else {
            pending.0 += 1;
        }
    }
}

fn webview_reveal_ready(_source: &WebviewSource, _has_ui_ready: bool, pending_frames: u8) -> bool {
    pending_frames >= REVEAL_FRAMES
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vmux_ui_webviews_reveal_after_frame_delay_even_without_ui_ready() {
        assert!(!webview_reveal_ready(
            &WebviewSource::new("vmux://header/"),
            false,
            REVEAL_FRAMES - 1
        ));
        assert!(webview_reveal_ready(
            &WebviewSource::new("vmux://header/"),
            false,
            REVEAL_FRAMES
        ));
    }

    #[test]
    fn tab_content_reveal_still_uses_frame_delay_only() {
        assert!(!webview_reveal_ready(
            &WebviewSource::new("https://example.com/"),
            false,
            REVEAL_FRAMES - 1
        ));
        assert!(webview_reveal_ready(
            &WebviewSource::new("https://example.com/"),
            false,
            REVEAL_FRAMES
        ));
    }

    #[test]
    fn unknown_vmux_urls_are_treated_as_content() {
        assert!(webview_reveal_ready(
            &WebviewSource::new("vmux://unknown/"),
            false,
            REVEAL_FRAMES
        ));
    }
}
