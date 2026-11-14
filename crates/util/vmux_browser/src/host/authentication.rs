use bevy::{
    ecs::system::{Commands as EcsCommands, SystemParam, SystemState},
    prelude::*,
};
use bevy_cef::prelude::{Browsers, HostWindow, JsEmitEventPlugin, Receive, UiEventPlugin, UiInput};
use vmux_api::PageIcon;
use vmux_api::browser::{BrowserModalBoundsRequest, BrowserModalCloseRequest, BrowserModalState};
use vmux_ecs::{EntityTarget, PageMetadata, UiStatePlugin, UiStateWriter};
use vmux_flex::prelude::Visibility;
use vmux_layout::state::LayoutUiState;
use vmux_session::host::{SessionView, SessionViews};
use vmux_session::{RunState, Session, SetupPending};

pub(super) struct Plugin;

impl bevy::prelude::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<crate::PopupWebviewsPlugin>() {
            app.add_plugins(crate::PopupWebviewsPlugin);
        }
        app.add_message::<Open>()
            .add_message::<crate::WebviewLoadCompleted>()
            .init_resource::<bevy_cef::prelude::IpcEventRawBuffer>()
            .add_plugins(UiStatePlugin::<LayoutUiState>::default())
            .add_plugins(UiEventPlugin::<(
                BrowserModalBoundsRequest,
                BrowserModalCloseRequest,
            )>::default())
            .add_plugins(JsEmitEventPlugin::<Dismiss>::default())
            .add_observer(bounds)
            .add_observer(close)
            .add_observer(dismiss)
            .add_systems(
                Update,
                (open, dispose, publish)
                    .chain()
                    .after(vmux_ecs::agent::AgentRequestApplySet),
            )
            .add_systems(Update, install_escape.after(crate::BrowserLoadSet));
    }
}

#[derive(Message, Clone)]
pub(super) struct Open {
    pub id: String,
    pub owner: Entity,
    pub window: Entity,
    pub session: Entity,
    pub view: Entity,
    pub url: String,
    pub title: String,
    pub icon: PageIcon,
}

#[derive(Component)]
struct AuthenticationPopup {
    id: String,
}

impl AuthenticationPopup {
    fn state(&self, metadata: &PageMetadata) -> BrowserModalState {
        let origin = url::Url::parse(&metadata.url)
            .ok()
            .and_then(|url| url.host_str().map(str::to_string))
            .unwrap_or_default();
        let icon = metadata.icon.favicon_url();
        BrowserModalState {
            id: self.id.clone(),
            title: metadata.title.clone(),
            origin,
            icon: (!icon.is_empty()).then(|| icon.to_string()),
        }
    }
}

#[derive(SystemParam)]
struct AuthenticationHost<'w, 's> {
    popups: Query<
        'w,
        's,
        (
            Entity,
            &'static AuthenticationPopup,
            &'static ChildOf,
            &'static EntityTarget<Session>,
            &'static EntityTarget<SessionView>,
            &'static PageMetadata,
        ),
    >,
    sessions: Query<'w, 's, (Has<SetupPending>, Option<&'static RunState>), With<Session>>,
    views: SessionViews<'w, 's>,
    commands: EcsCommands<'w, 's>,
}

impl AuthenticationHost<'_, '_> {
    fn retire(&mut self, entity: Entity) {
        self.commands
            .trigger(crate::PopupHideRequest { webview: entity });
        self.commands.entity(entity).try_despawn();
    }
}

fn open(world: &mut World, state: &mut SystemState<(MessageReader<Open>, AuthenticationHost)>) {
    let requests: Vec<_> = {
        let Ok((mut requests, _)) = state.get_mut(world) else {
            return;
        };
        requests.read().cloned().collect()
    };
    for request in requests {
        {
            let Ok((_, mut host)) = state.get_mut(world) else {
                continue;
            };
            if host.views.session(request.view) != Some(request.session)
                || !host
                    .sessions
                    .get(request.session)
                    .is_ok_and(|(pending, _)| pending)
            {
                continue;
            }
            let previous: Vec<_> = host
                .popups
                .iter()
                .filter(|(_, _, parent, ..)| parent.parent() == request.owner)
                .map(|(entity, ..)| entity)
                .collect();
            for entity in previous {
                host.retire(entity);
            }
            let metadata = PageMetadata {
                url: request.url.clone(),
                title: request.title.clone(),
                icon: request.icon.clone(),
                ..Default::default()
            };
            let popup = AuthenticationPopup {
                id: request.id.clone(),
            };
            host.commands
                .spawn(vmux_layout::Browser::new_with_title(
                    &request.url,
                    &request.title,
                ))
                .insert((
                    metadata,
                    Name::new(format!("Authentication: {}", request.title)),
                    popup,
                    ChildOf(request.owner),
                    HostWindow(request.window),
                    EntityTarget::<Session>::new(request.session),
                    EntityTarget::<SessionView>::new(request.view),
                    vmux_ecs::WindowOverlay,
                    vmux_ecs::FixedWindowOverlay,
                    crate::PopupWebview,
                    Visibility::Hidden,
                ));
        }
        state.apply(world);
    }
}

fn bounds(
    input: On<UiInput<BrowserModalBoundsRequest>>,
    popups: Query<(Entity, &AuthenticationPopup, &ChildOf)>,
    mut commands: EcsCommands,
) {
    let request = &input.event().payload;
    let Some(bounds) =
        crate::PopupBounds::new(request.left, request.top, request.width, request.height)
    else {
        return;
    };
    for (entity, popup, parent) in &popups {
        if popup.id == request.id && parent.parent() == input.event().webview {
            commands
                .entity(entity)
                .insert((bounds, Visibility::Visible, vmux_ecs::KeyboardOwner));
        }
    }
}

fn close(input: On<UiInput<BrowserModalCloseRequest>>, mut host: AuthenticationHost) {
    let target = host
        .popups
        .iter()
        .find(|(_, popup, parent, ..)| {
            popup.id == input.event().payload.id && parent.parent() == input.event().webview
        })
        .map(|(entity, _, parent, _, view, _)| (entity, parent.parent(), view.entity()));
    let Some((entity, _owner, view)) = target else {
        return;
    };
    host.commands.trigger(UiInput {
        webview: view,
        payload: vmux_session::onboarding::ChatCancelSetup,
    });
    host.retire(entity);
}

fn dispose(mut host: AuthenticationHost) {
    let stale: Vec<_> = host
        .popups
        .iter()
        .filter(|(_, _, _, session, view, _)| {
            host.views.session(view.entity()) != Some(session.entity())
                || !host
                    .sessions
                    .get(session.entity())
                    .is_ok_and(|(pending, state)| {
                        pending && matches!(state, Some(RunState::AuthenticationRequired { .. }))
                    })
        })
        .map(|(entity, _, parent, ..)| (entity, parent.parent()))
        .collect();
    for (entity, _owner) in stale {
        host.retire(entity);
    }
}

fn publish(
    popups: Query<(&AuthenticationPopup, &ChildOf, &PageMetadata)>,
    layouts: Query<Entity, With<vmux_layout::LayoutCef>>,
    mut writer: UiStateWriter<LayoutUiState>,
    mut published: Local<std::collections::HashMap<Entity, BrowserModalState>>,
) {
    published.retain(|owner, _| layouts.contains(*owner));
    for owner in &layouts {
        let next = popups
            .iter()
            .find(|(_, parent, _)| parent.parent() == owner)
            .map(|(popup, _, metadata)| popup.state(metadata))
            .unwrap_or_default();
        if published.get(&owner) == Some(&next) {
            continue;
        }
        published.insert(owner, next.clone());
        writer.push(owner, next);
    }
}

#[vmux_api::contract]
struct Dismiss {
    channel: String,
}

fn dismiss(input: On<Receive<Dismiss>>, mut host: AuthenticationHost) {
    if input.payload.channel != "vmux-authentication-popup-close" {
        return;
    }
    let target = host
        .popups
        .get(input.webview)
        .ok()
        .map(|(entity, _, parent, _, view, _)| (entity, parent.parent(), view.entity()));
    let Some((entity, _owner, view)) = target else {
        return;
    };
    host.commands.trigger(UiInput {
        webview: view,
        payload: vmux_session::onboarding::ChatCancelSetup,
    });
    host.retire(entity);
}

fn install_escape(
    mut loaded: MessageReader<crate::WebviewLoadCompleted>,
    popups: Query<(), With<AuthenticationPopup>>,
    browsers: NonSend<Browsers>,
) {
    for event in loaded.read() {
        if popups.contains(event.webview) {
            browsers.execute_js(&event.webview, r#"document.addEventListener('keydown', event => { if (event.key === 'Escape') { event.preventDefault(); cef.emit({channel:'vmux-authentication-popup-close'}); } }, true);"#);
        }
    }
}
