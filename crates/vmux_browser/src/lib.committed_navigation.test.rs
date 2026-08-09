use super::*;
use bevy_cef::prelude::WebviewCommittedNavigationReceiver;
use bevy_cef_core::prelude::{
    CefTransitionCore, CefTransitionQualifiers, WebviewCommittedNavigationEvent,
};

#[derive(Resource, Default)]
struct Collected(Vec<Entity>);

fn collect(
    mut events: MessageReader<WebviewCommittedNavigationEvent>,
    mut collected: ResMut<Collected>,
) {
    collected.0.extend(events.read().map(|event| event.webview));
}

#[test]
fn infrastructure_navigation_is_not_forwarded() {
    let mut app = App::new();
    let infrastructure = app
        .world_mut()
        .spawn(crate::extensions::bridge_page::ExtensionBridgeWebview {
            extension_id: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
            role: crate::extensions::bridge_page::ExtensionBridgeRole::Transport,
        })
        .id();
    let visible = app.world_mut().spawn_empty().id();
    let (sender, receiver) = async_channel::unbounded();
    app.insert_resource(WebviewCommittedNavigationReceiver(receiver))
        .init_resource::<crate::extensions::bridge_page::ExtensionInfrastructureEntities>()
        .init_resource::<Collected>()
        .add_message::<WebviewCommittedNavigationEvent>()
        .add_systems(Update, (drain_committed_navigation, collect).chain());
    app.world_mut()
        .resource_mut::<crate::extensions::bridge_page::ExtensionInfrastructureEntities>()
        .insert(infrastructure);
    app.world_mut().despawn(infrastructure);
    for webview in [infrastructure, visible] {
        sender
            .send_blocking(WebviewCommittedNavigationEvent {
                webview,
                url: "https://example.com".into(),
                is_main_frame: true,
                transition: CefTransitionCore::Link,
                qualifiers: CefTransitionQualifiers::default(),
            })
            .unwrap();
    }

    app.update();

    assert_eq!(app.world().resource::<Collected>().0, [visible]);
}
