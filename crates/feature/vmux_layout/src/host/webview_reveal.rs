use bevy::prelude::*;
use bevy::winit::{EventLoopProxyWrapper, WinitUserEvent};
use bevy_cef::prelude::WebviewSource;
use vmux_ecs::page::PageReady;

use crate::cef::Browser;
use vmux_flex::prelude::*;

impl Plugin for WebviewRevealPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(webview_added)
            .add_systems(PostUpdate, reveal_webviews.after(LayoutSystems::Layout));
    }
}

pub struct WebviewRevealPlugin;

#[derive(Component)]
pub struct PendingWebviewReveal(u8);

const REVEAL_FRAMES: u8 = 2;

impl PendingWebviewReveal {
    pub(crate) fn pending() -> Self {
        Self(0)
    }
}

fn webview_added(
    trigger: On<Add, WebviewSource>,
    browsers: Query<(), With<Browser>>,
    mut commands: Commands,
) {
    let entity = trigger.event_target();
    if !browsers.contains(entity) {
        return;
    }
    commands
        .entity(entity)
        .insert((Visibility::Hidden, PendingWebviewReveal::pending()));
}

fn reveal_webviews(
    mut commands: Commands,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut query: Query<(
        Entity,
        &WebviewSource,
        &mut Visibility,
        &mut PendingWebviewReveal,
        Has<PageReady>,
    )>,
) {
    let mut progressed = false;
    for (entity, source, mut vis, mut pending, ready) in &mut query {
        if webview_reveal_ready(source, ready, pending.0) {
            *vis = Visibility::Visible;
            commands.entity(entity).remove::<PendingWebviewReveal>();
            progressed = true;
        } else if ready {
            pending.0 += 1;
            progressed = true;
        } else if pending.0 != 0 {
            pending.0 = 0;
        }
    }
    if progressed && let Some(proxy) = proxy {
        let _ = proxy.send_event(WinitUserEvent::WakeUp);
    }
}

fn webview_reveal_ready(_source: &WebviewSource, has_page_ready: bool, pending_frames: u8) -> bool {
    has_page_ready && pending_frames >= REVEAL_FRAMES
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infrastructure_webviews_do_not_wait_for_visual_reveal() {
        let mut app = App::new();
        app.add_observer(webview_added);

        let infrastructure = app
            .world_mut()
            .spawn(WebviewSource::new("chrome-extension://bridge/"))
            .id();
        let browser = app
            .world_mut()
            .spawn((Browser, WebviewSource::new("https://example.com/")))
            .id();
        app.update();

        assert!(
            app.world()
                .get::<PendingWebviewReveal>(infrastructure)
                .is_none()
        );
        assert!(app.world().get::<PendingWebviewReveal>(browser).is_some());
        assert_eq!(
            app.world().get::<Visibility>(browser),
            Some(&Visibility::Hidden)
        );
    }

    #[test]
    fn vmux_ui_webviews_wait_for_page_ready() {
        assert!(!webview_reveal_ready(
            &WebviewSource::new("vmux://header/"),
            false,
            REVEAL_FRAMES - 1
        ));
        assert!(webview_reveal_ready(
            &WebviewSource::new("vmux://header/"),
            true,
            REVEAL_FRAMES
        ));
    }

    #[test]
    fn tab_content_reveal_waits_after_page_ready() {
        assert!(!webview_reveal_ready(
            &WebviewSource::new("https://example.com/"),
            false,
            REVEAL_FRAMES - 1
        ));
        assert!(webview_reveal_ready(
            &WebviewSource::new("https://example.com/"),
            true,
            REVEAL_FRAMES
        ));
    }

    #[test]
    fn unknown_vmux_urls_are_treated_as_content() {
        assert!(webview_reveal_ready(
            &WebviewSource::new("vmux://unknown/"),
            true,
            REVEAL_FRAMES
        ));
    }

    #[test]
    fn a_new_navigation_waits_for_its_own_reveal_frames() {
        let mut app = App::new();
        app.add_systems(Update, reveal_webviews);
        let browser = app
            .world_mut()
            .spawn((
                Browser,
                WebviewSource::new("vmux://projects/"),
                Visibility::Hidden,
                PendingWebviewReveal::pending(),
                PageReady {},
            ))
            .id();

        app.update();
        assert_eq!(
            app.world().get::<PendingWebviewReveal>(browser).unwrap().0,
            1
        );

        app.world_mut().entity_mut(browser).remove::<PageReady>();
        app.update();
        assert_eq!(
            app.world().get::<PendingWebviewReveal>(browser).unwrap().0,
            0
        );

        app.world_mut().entity_mut(browser).insert(PageReady {});
        app.update();
        assert_eq!(
            app.world().get::<PendingWebviewReveal>(browser).unwrap().0,
            1
        );
        assert_eq!(
            app.world().get::<Visibility>(browser),
            Some(&Visibility::Hidden)
        );
    }
}
