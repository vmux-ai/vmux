use bevy::prelude::*;
use bevy_cef::prelude::WebviewSource;

use crate::window::VmuxWindow;
use vmux_flex::prelude::*;

impl Plugin for WebviewRevealPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(on_webview_added)
            .add_systems(PostUpdate, reveal_webviews.after(LayoutSystems::Layout));
    }
}

pub struct WebviewRevealPlugin;

#[derive(Component)]
pub struct PendingWebviewReveal(u8);

const REVEAL_FRAMES: u8 = 2;

fn on_webview_added(
    trigger: On<Add, WebviewSource>,
    root: Query<(), With<VmuxWindow>>,
    mut commands: Commands,
) {
    let entity = trigger.event_target();
    if root.contains(entity) {
        return;
    }
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
            *vis = Visibility::Visible;
            commands.entity(entity).remove::<PendingWebviewReveal>();
        } else {
            pending.0 += 1;
        }
    }
}

fn webview_reveal_ready(
    _source: &WebviewSource,
    _has_page_ready: bool,
    pending_frames: u8,
) -> bool {
    pending_frames >= REVEAL_FRAMES
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vmux_ui_webviews_reveal_after_frame_delay_even_without_page_ready() {
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
