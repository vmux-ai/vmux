use bevy::ecs::system::{SystemParam, SystemState};
use bevy::prelude::*;
use bevy_cef::prelude::{Browsers, HostWindow, JsEmitEventPlugin, Receive, UiEventPlugin, UiInput};
use vmux_api::mcp::{
    ConnectorPopupBoundsRequest, ConnectorPopupCloseRequest, ConnectorPopupShowRequest,
    ConnectorPopupState,
};
use vmux_ecs::{
    Active, BrowserLoadSet, BrowserOverlaySet, KeyboardOwner, PageOpenRequest, PageOpenTarget,
    PopupWebview, UiStateWrite, WebviewLoadCompleted, WindowOverlay,
};
use vmux_flex::prelude::{LayoutSystems, Visibility};
use vmux_tool::{ConnectionSet, ConnectorCancelRequest, ConnectorPopupRequest};

use crate::overlay::PopupBounds;
use crate::state::LayoutUiState;
use crate::{Browser, LayoutCef};

pub(super) struct Plugin;

impl bevy::app::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ConnectorPopupRequest>()
            .add_message::<PageOpenRequest>()
            .add_message::<WebviewLoadCompleted>()
            .add_plugins(UiEventPlugin::<(
                ConnectorPopupBoundsRequest,
                ConnectorPopupCloseRequest,
                ConnectorPopupShowRequest,
            )>::default())
            .add_plugins(JsEmitEventPlugin::<Dismiss>::default())
            .add_observer(bounds)
            .add_observer(close)
            .add_observer(show)
            .add_observer(dismiss)
            .add_systems(
                Update,
                (receive, dispose, project).chain().after(ConnectionSet),
            )
            .add_systems(Update, inject.after(BrowserLoadSet))
            .add_systems(
                PostUpdate,
                present
                    .in_set(BrowserOverlaySet)
                    .after(LayoutSystems::Layout),
            );
    }
}

#[derive(Component, Clone)]
struct Popup {
    source: Entity,
    generation: u64,
    layout: Entity,
    provider: String,
    origin: String,
    code: Option<String>,
}

impl Popup {
    fn origin(url: &str) -> String {
        let Some((_, rest)) = url.split_once("://") else {
            return String::new();
        };
        rest.split(['/', '?', '#'])
            .next()
            .unwrap_or_default()
            .to_string()
    }

    fn key(&self) -> String {
        format!("{}:{}", self.source.to_bits(), self.generation)
    }

    fn state(&self, name: &Name, open: bool) -> ConnectorPopupState {
        ConnectorPopupState {
            id: self.key(),
            provider: self.provider.clone(),
            name: name.as_str().to_string(),
            origin: self.origin.clone(),
            code: self.code.clone(),
            open,
        }
    }
}

#[derive(Component)]
struct Shown;

#[derive(Component)]
struct Presented;

#[derive(SystemParam)]
struct Host<'w, 's> {
    popups: Query<
        'w,
        's,
        (
            Entity,
            &'static Popup,
            &'static Name,
            Has<Active>,
            Has<Shown>,
            Option<&'static PopupBounds>,
        ),
    >,
    layouts: Query<'w, 's, (Entity, &'static HostWindow), With<LayoutCef>>,
    hosts: Query<'w, 's, &'static HostWindow>,
    parents: Query<'w, 's, &'static ChildOf>,
    sources: Query<'w, 's, Entity>,
    browsers: NonSend<'w, Browsers>,
    opens: MessageWriter<'w, PageOpenRequest>,
    commands: Commands<'w, 's>,
}

impl Host<'_, '_> {
    fn window(&self, mut source: Entity) -> Option<Entity> {
        loop {
            if let Ok(host) = self.hosts.get(source) {
                return Some(host.0);
            }
            source = self.parents.get(source).ok()?.parent();
        }
    }

    fn publish(&mut self, layout: Entity, state: ConnectorPopupState) {
        self.commands
            .trigger(UiStateWrite::<LayoutUiState>::from_event(layout, &state));
    }

    fn select(&mut self, entity: Entity, popup: &Popup, name: &Name, bounded: bool) {
        for (other, current, _, _, _, _) in &self.popups {
            if other != entity && current.layout == popup.layout {
                self.browsers.hide_child_window(&other);
                self.commands
                    .entity(other)
                    .remove::<(Active, Shown, KeyboardOwner, Presented)>()
                    .insert(Visibility::Hidden);
            }
        }
        self.commands.entity(entity).insert((Active, Shown));
        if bounded {
            self.commands
                .entity(entity)
                .insert((Visibility::Visible, KeyboardOwner));
        }
        self.publish(popup.layout, popup.state(name, true));
    }

    fn open(
        &mut self,
        source: Entity,
        generation: u64,
        provider: String,
        name: String,
        url: String,
        code: Option<String>,
    ) {
        if !cfg!(target_os = "macos") {
            self.opens.write(PageOpenRequest {
                target: PageOpenTarget::NewStack,
                url,
                request_id: None,
            });
            return;
        }
        let Some(window) = self.window(source) else {
            return;
        };
        let Some((layout, host)) = self.layouts.iter().find(|(_, host)| host.0 == window) else {
            return;
        };
        let window = host.0;
        if let Some((entity, popup, name, _, _, bounds)) = self
            .popups
            .iter()
            .find(|(_, popup, _, _, _, _)| popup.source == source && popup.generation == generation)
        {
            let popup = popup.clone();
            let name = name.clone();
            let bounded = bounds.is_some();
            self.select(entity, &popup, &name, bounded);
            return;
        }
        let popup = Popup {
            source,
            generation,
            layout,
            provider,
            origin: Popup::origin(&url),
            code,
        };
        let name = Name::new(name);
        let entity = self
            .commands
            .spawn(Browser::new_with_title(&url, name.as_str()))
            .id();
        self.commands.entity(entity).insert((
            popup.clone(),
            name.clone(),
            HostWindow(window),
            WindowOverlay,
            PopupWebview,
            Visibility::Hidden,
        ));
        self.select(entity, &popup, &name, false);
    }

    fn cancel(&mut self, entity: Entity, popup: &Popup, selected: bool) {
        self.commands.trigger(ConnectorCancelRequest {
            source: popup.source,
            generation: popup.generation,
        });
        self.retire(entity, popup, selected);
    }

    fn retire(&mut self, entity: Entity, popup: &Popup, selected: bool) {
        self.browsers.hide_child_window(&entity);
        self.commands.entity(entity).try_despawn();
        if selected {
            self.publish(popup.layout, ConnectorPopupState::default());
        }
    }

    fn close(&mut self, source: Entity, generation: u64) {
        let mut targets = Vec::new();
        for (entity, popup, _, selected, _, _) in &self.popups {
            if popup.source == source && popup.generation == generation {
                targets.push((entity, popup.clone(), selected));
            }
        }
        for (entity, popup, selected) in targets {
            self.retire(entity, &popup, selected);
        }
    }
}

fn receive(
    world: &mut World,
    state: &mut SystemState<(MessageReader<ConnectorPopupRequest>, Host)>,
) {
    let requests: Vec<_> = {
        let Ok((mut messages, _)) = state.get_mut(world) else {
            return;
        };
        messages.read().cloned().collect()
    };
    for request in requests {
        {
            let Ok((_, mut host)) = state.get_mut(world) else {
                continue;
            };
            match request {
                ConnectorPopupRequest::Open {
                    source,
                    generation,
                    provider,
                    name,
                    url,
                    code,
                } => host.open(source, generation, provider, name, url, code),
                ConnectorPopupRequest::Close { source, generation } => {
                    host.close(source, generation)
                }
            }
        }
        state.apply(world);
    }
}

fn bounds(
    trigger: On<UiInput<ConnectorPopupBoundsRequest>>,
    popups: Query<(Entity, &Popup), With<Shown>>,
    mut commands: Commands,
) {
    let request = &trigger.payload;
    let Some(bounds) = PopupBounds::new(request.left, request.top, request.width, request.height)
    else {
        return;
    };
    for (entity, popup) in &popups {
        if popup.layout == trigger.webview && popup.key() == request.id {
            commands
                .entity(entity)
                .insert((bounds, Visibility::Visible, KeyboardOwner));
        }
    }
}

fn show(trigger: On<UiInput<ConnectorPopupShowRequest>>, mut host: Host) {
    let target = host
        .popups
        .iter()
        .find(|(_, popup, _, _, _, _)| {
            popup.layout == trigger.webview && popup.key() == trigger.payload.id
        })
        .map(|(entity, popup, name, _, _, bounds)| {
            (entity, popup.clone(), name.clone(), bounds.is_some())
        });
    if let Some((entity, popup, name, bounded)) = target {
        host.select(entity, &popup, &name, bounded);
    }
}

fn close(trigger: On<UiInput<ConnectorPopupCloseRequest>>, mut host: Host) {
    let target = host
        .popups
        .iter()
        .find(|(_, popup, _, _, _, _)| {
            popup.layout == trigger.webview && popup.key() == trigger.payload.id
        })
        .map(|(entity, popup, name, selected, _, _)| {
            (entity, popup.clone(), name.clone(), selected)
        });
    if let Some((entity, popup, _, selected)) = target {
        host.cancel(entity, &popup, selected);
    }
}

#[derive(serde::Deserialize)]
struct Dismiss {
    channel: String,
    id: String,
}

fn dismiss(trigger: On<Receive<Dismiss>>, mut host: Host) {
    if trigger.payload.channel != "vmux-connector-popup-close" {
        return;
    };
    let Ok((_, popup, _, selected, _, _)) = host.popups.get(trigger.webview) else {
        return;
    };
    if popup.key() != trigger.payload.id {
        return;
    };
    let popup = popup.clone();
    host.cancel(trigger.webview, &popup, selected);
}

fn dispose(mut host: Host) {
    let mut targets = Vec::new();
    for (entity, popup, _, selected, _, _) in &host.popups {
        if !host.sources.contains(popup.source) || !host.layouts.contains(popup.layout) {
            targets.push((entity, popup.clone(), selected));
        }
    }
    for (entity, popup, selected) in targets {
        host.retire(entity, &popup, selected);
    }
}

fn inject(
    mut messages: MessageReader<WebviewLoadCompleted>,
    popups: Query<&Popup>,
    browsers: NonSend<Browsers>,
) {
    for message in messages.read() {
        let Ok(popup) = popups.get(message.webview) else {
            continue;
        };
        let Ok(id) = serde_json::to_string(&popup.key()) else {
            continue;
        };
        browsers.execute_js(&message.webview, &format!(r#"document.addEventListener('keydown', event => {{ if (event.key === 'Escape') {{ event.preventDefault(); cef.emit({{ channel: 'vmux-connector-popup-close', id: {id} }}); }} }});"#));
    }
}

fn project(
    mut popups: Query<
        (
            &mut Popup,
            &Name,
            &vmux_ecs::PageMetadata,
            Has<Active>,
            Has<Shown>,
        ),
        Changed<vmux_ecs::PageMetadata>,
    >,
    mut commands: Commands,
) {
    for (mut popup, name, metadata, selected, open) in &mut popups {
        let origin = Popup::origin(&metadata.url);
        if popup.origin == origin {
            continue;
        }
        popup.origin = origin;
        if selected {
            commands.trigger(UiStateWrite::<LayoutUiState>::from_event(
                popup.layout,
                &popup.state(name, open),
            ));
        }
    }
}

fn present(
    popups: Query<(Entity, &PopupBounds, &HostWindow, Has<Presented>), With<Shown>>,
    windows: Query<&Window>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    for (entity, bounds, host, presented) in &popups {
        let Ok(window) = windows.get(host.0) else {
            continue;
        };
        if bounds.present(&browsers, entity, window, presented) && !presented {
            commands.entity(entity).insert(Presented);
        }
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    use crate::stack::Stack;
    use vmux_ecs::PageMetadata;

    #[derive(Resource, Default)]
    struct Cancellations(Vec<(Entity, u64)>);

    fn capture(trigger: On<ConnectorCancelRequest>, mut cancellations: ResMut<Cancellations>) {
        cancellations.0.push((trigger.source, trigger.generation));
    }

    struct Fixture {
        app: App,
        layout: Entity,
        source: Entity,
    }

    impl Fixture {
        fn new() -> Self {
            let mut app = App::new();
            app.insert_non_send(Browsers::default())
                .init_resource::<Cancellations>()
                .add_observer(capture)
                .init_resource::<bevy_cef::prelude::IpcEventRawBuffer>()
                .add_plugins((vmux_ecs::UiStatePlugin::<LayoutUiState>::default(), Plugin));
            let window = app.world_mut().spawn(Window::default()).id();
            let layout = app.world_mut().spawn((LayoutCef, HostWindow(window))).id();
            let source = app
                .world_mut()
                .spawn((
                    Stack::default(),
                    HostWindow(window),
                    PageMetadata {
                        url: "vmux://tools/connectors".into(),
                        ..default()
                    },
                ))
                .id();
            Self {
                app,
                layout,
                source,
            }
        }

        fn open(&mut self, generation: u64) {
            self.app
                .world_mut()
                .write_message(ConnectorPopupRequest::Open {
                    source: self.source,
                    generation,
                    provider: "github".into(),
                    name: "GitHub".into(),
                    url: "https://github.com/login/device".into(),
                    code: Some("ABCD-EFGH".into()),
                });
            self.app.update();
        }

        fn popup(&mut self) -> (Entity, String) {
            let world = self.app.world_mut();
            let (entity, popup) = world.query::<(Entity, &Popup)>().single(world).unwrap();
            (entity, popup.key())
        }
    }

    #[test]
    fn authentication_uses_an_overlay_without_replacing_the_tools_stack() {
        let mut fixture = Fixture::new();
        fixture.open(1);
        let (popup, _) = fixture.popup();
        let world = fixture.app.world_mut();
        assert_eq!(world.query::<&Stack>().iter(world).count(), 1);
        assert_eq!(
            world.get::<PageMetadata>(fixture.source).unwrap().url,
            "vmux://tools/connectors"
        );
        let popup = world.entity(popup);
        assert!(popup.contains::<WindowOverlay>() && popup.contains::<PopupWebview>());
        assert_eq!(
            popup.get::<PageMetadata>().unwrap().url,
            "https://github.com/login/device"
        );
    }

    #[test]
    fn closing_requests_cancellation_and_removes_only_the_matching_popup() {
        let mut fixture = Fixture::new();
        fixture.open(3);
        let (entity, id) = fixture.popup();
        fixture.app.world_mut().trigger(UiInput {
            webview: fixture.layout,
            payload: ConnectorPopupCloseRequest { id: "old".into() },
        });
        fixture.app.world_mut().flush();
        assert!(fixture.app.world().resource::<Cancellations>().0.is_empty());
        fixture.app.world_mut().trigger(UiInput {
            webview: fixture.layout,
            payload: ConnectorPopupCloseRequest { id },
        });
        fixture.app.world_mut().flush();
        assert_eq!(
            fixture.app.world().resource::<Cancellations>().0,
            [(fixture.source, 3)]
        );
        assert!(fixture.app.world().get_entity(entity).is_err());
        assert!(fixture.app.world().get_entity(fixture.source).is_ok());
    }

    #[test]
    fn reopening_keeps_the_same_document_and_completion_retires_only_its_generation() {
        let mut fixture = Fixture::new();
        fixture.open(2);
        let (entity, _) = fixture.popup();
        fixture
            .app
            .world_mut()
            .get_mut::<PageMetadata>(entity)
            .unwrap()
            .url = "https://github.com/sessions/two-factor".into();
        fixture.open(2);
        assert_eq!(fixture.popup().0, entity);
        assert_eq!(
            fixture.app.world().get::<PageMetadata>(entity).unwrap().url,
            "https://github.com/sessions/two-factor"
        );
        fixture
            .app
            .world_mut()
            .write_message(ConnectorPopupRequest::Close {
                source: fixture.source,
                generation: 1,
            });
        fixture.app.update();
        assert!(fixture.app.world().get_entity(entity).is_ok());
        fixture
            .app
            .world_mut()
            .write_message(ConnectorPopupRequest::Close {
                source: fixture.source,
                generation: 2,
            });
        fixture.app.update();
        assert!(fixture.app.world().get_entity(entity).is_err());
        assert!(fixture.app.world().get_entity(fixture.source).is_ok());
    }

    #[test]
    fn bounds_from_another_layout_or_an_old_popup_cannot_reveal_authentication() {
        let mut fixture = Fixture::new();
        fixture.open(1);
        let (entity, id) = fixture.popup();
        let other = fixture.app.world_mut().spawn_empty().id();
        for (layout, id) in [(other, id.clone()), (fixture.layout, "old".into())] {
            fixture.app.world_mut().trigger(UiInput {
                webview: layout,
                payload: ConnectorPopupBoundsRequest {
                    id,
                    left: 100.0,
                    top: 50.0,
                    width: 480.0,
                    height: 600.0,
                },
            });
        }
        fixture.app.update();
        assert!(fixture.app.world().get::<PopupBounds>(entity).is_none());
        fixture.app.world_mut().trigger(UiInput {
            webview: fixture.layout,
            payload: ConnectorPopupBoundsRequest {
                id,
                left: 100.0,
                top: 50.0,
                width: 480.0,
                height: 600.0,
            },
        });
        fixture.app.update();
        assert_eq!(
            fixture.app.world().get::<Visibility>(entity),
            Some(&Visibility::Visible)
        );
    }
}
