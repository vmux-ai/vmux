use crate::native_bridge::HistoryClaims;
use bevy::ecs::system::SystemParam;
use bevy::prelude::{
    App, Commands, Component, Entity, EntityEvent, IntoScheduleConfigs, MessageReader,
    MessageWriter, NonSend, On, Or, Plugin, Query, Remove, Res, Update, With, Without,
};
use bevy_cef::prelude::{BinIpcBlocked, Browsers, HistorySwipeEventReceiver};
use bevy_cef_core::prelude::{HistorySwipeDirection, HistorySwipeRaw};
use vmux_command::ReadCommandRequests;
use vmux_page::HistorySwipe;

use crate::{NavigationOperation, NavigationRequest, WebviewLoadCompleted};

pub(crate) struct HistorySwipePlugin;

impl Plugin for HistorySwipePlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<crate::native_bridge::NativeHistoryPlugin>() {
            app.add_plugins(crate::native_bridge::NativeHistoryPlugin);
        }
        app.add_message::<HistorySwipeRaw>()
            .add_observer(retire)
            .add_systems(
                Update,
                (initialize, receive, sync)
                    .chain()
                    .after(crate::BrowserLoadSet)
                    .before(ReadCommandRequests),
            );
    }
}

#[derive(Component, Default)]
struct Installation {
    token: String,
}

impl Installation {
    fn new() -> Self {
        Self {
            token: uuid::Uuid::new_v4().simple().to_string(),
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
        !self.token.is_empty() && self.token == event.token && event.page_url == current_url
    }
}

fn initialize(
    owners: Query<Entity, (With<vmux_layout::Browser>, Without<Installation>)>,
    mut commands: Commands,
) {
    for owner in &owners {
        commands.entity(owner).insert(Installation::default());
    }
}

#[derive(SystemParam)]
struct NativeDocuments<'w> {
    browsers: NonSend<'w, Browsers>,
}

impl NativeDocuments<'_> {
    fn available(&self, owner: Entity) -> bool {
        self.browsers.has_browser(owner)
    }

    fn url(&self, owner: Entity) -> Option<String> {
        self.browsers.page_url(&owner)
    }

    fn inject(&self, owner: Entity, script: &str) -> bool {
        if self.browsers.page_url(&owner).is_none() {
            return false;
        }
        self.browsers.execute_js(&owner, script);
        true
    }
}

fn receive(receiver: Res<HistorySwipeEventReceiver>, mut events: MessageWriter<HistorySwipeRaw>) {
    while let Ok(event) = receiver.0.try_recv() {
        events.write(event);
    }
}

fn sync(
    mut navigated: MessageReader<bevy_cef_core::prelude::WebviewCommittedNavigationEvent>,
    mut loaded: MessageReader<WebviewLoadCompleted>,
    mut events: MessageReader<HistorySwipeRaw>,
    native: NativeDocuments,
    mut installations: Query<(Entity, &mut Installation), With<vmux_layout::Browser>>,
    owners: Query<(), With<vmux_layout::Browser>>,
    blocked: Query<
        (),
        Or<(
            With<BinIpcBlocked>,
            With<vmux_layout::hosted_page::SurfaceBlocked>,
        )>,
    >,
    mut requests: MessageWriter<NavigationRequest>,
    mut history: HistoryClaims,
) {
    for event in navigated.read() {
        if event.is_main_frame {
            history.invalidate(event.webview, event.at);
            if let Ok((_, mut installation)) = installations.get_mut(event.webview) {
                installation.token.clear();
            }
        }
    }
    for (webview, mut installation) in &mut installations {
        if !native.available(webview) {
            installation.token.clear();
        }
    }
    for event in loaded.read() {
        history.invalidate(event.webview, event.at);
        if !owners.contains(event.webview) {
            continue;
        }
        let Ok((_, mut current)) = installations.get_mut(event.webview) else {
            continue;
        };
        current.token.clear();
        let Some(url) = native.url(event.webview) else {
            continue;
        };
        if !url.starts_with("http://") && !url.starts_with("https://") {
            continue;
        }
        let installation = Installation::new();
        if native.inject(event.webview, &installation.script()) {
            *current = installation;
        }
    }
    for event in events.read() {
        if blocked.contains(event.webview) {
            continue;
        }
        let Ok((_, installation)) = installations.get(event.webview) else {
            continue;
        };
        let Some(url) = native.url(event.webview) else {
            continue;
        };
        if !installation.accepts(event, &url) {
            continue;
        }
        let operation = match event.direction {
            HistorySwipeDirection::Back => NavigationOperation::Back,
            HistorySwipeDirection::Forward => NavigationOperation::Forward,
        };
        if !history.claim(event.webview, operation, event.at) {
            continue;
        }
        requests.write(NavigationRequest::surface(event.webview, operation));
    }
}

fn retire(removed: On<Remove, vmux_layout::Browser>, mut commands: Commands) {
    commands
        .entity(removed.event_target())
        .try_remove::<Installation>();
}

#[cfg(test)]
mod tests {
    use super::*;

    struct InstallationFixture {
        app: App,
        owner: Entity,
    }

    impl InstallationFixture {
        fn new() -> Self {
            let mut app = App::new();
            let (_, receiver) = async_channel::bounded(32);
            app.add_plugins((bevy::prelude::MinimalPlugins, HistorySwipePlugin))
                .insert_non_send(Browsers::default())
                .insert_resource(HistorySwipeEventReceiver(receiver))
                .add_message::<bevy_cef_core::prelude::WebviewCommittedNavigationEvent>()
                .add_message::<WebviewLoadCompleted>()
                .add_message::<NavigationRequest>();
            let owner = app.world_mut().spawn(vmux_layout::Browser).id();
            app.update();
            Self { app, owner }
        }

        fn accepts(&self, event: &HistorySwipeRaw) -> bool {
            self.app
                .world()
                .get::<Installation>(self.owner)
                .is_some_and(|installation| installation.accepts(event, &event.page_url))
        }
    }

    #[test]
    fn browser_reattachment_cannot_reuse_a_retired_document_token() {
        let mut fixture = InstallationFixture::new();
        let installation = Installation::new();
        let event = HistorySwipeRaw {
            at: std::time::Instant::now(),
            webview: fixture.owner,
            page_url: "https://example.com/".to_string(),
            token: installation.token.clone(),
            direction: HistorySwipeDirection::Back,
        };
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.owner)
            .insert(installation);
        assert!(fixture.accepts(&event));
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.owner)
            .remove::<vmux_layout::Browser>();
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.owner)
            .insert(vmux_layout::Browser);
        assert!(!fixture.accepts(&event));
        fixture.app.update();
        assert!(!fixture.accepts(&event));
    }

    #[test]
    fn a_missing_native_browser_retires_its_document_token() {
        let mut fixture = InstallationFixture::new();
        let installation = Installation::new();
        let event = HistorySwipeRaw {
            at: std::time::Instant::now(),
            webview: fixture.owner,
            page_url: "https://example.com/".to_string(),
            token: installation.token.clone(),
            direction: HistorySwipeDirection::Forward,
        };
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.owner)
            .insert(installation);
        assert!(fixture.accepts(&event));
        fixture.app.update();
        assert!(!fixture.accepts(&event));
    }

    #[test]
    fn a_previous_document_cannot_reuse_a_history_notification() {
        let installation = Installation::new();
        let mut event = HistorySwipeRaw {
            at: std::time::Instant::now(),
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
