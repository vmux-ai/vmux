use bevy::prelude::*;
use bevy::winit::{EventLoopProxyWrapper, WinitUserEvent};
use bevy_cef::prelude::{BinReceive, Browsers, UiEventPlugin, WebviewSize};
use vmux_core::page::PageReady;
use vmux_layout::stack::StackRequest;
use vmux_service::client::ServiceClient;
use vmux_service::protocol::{ClientMessage, ProcessId};

use crate::Terminal;
use crate::event::{TermLinkOpenRequest, TermResizeEvent, TermScrollEvent};

use super::plugin::ServiceMessageSet;

pub(super) struct ViewPlugin;

impl Plugin for ViewPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(UiEventPlugin::<(
            TermResizeEvent,
            TermScrollEvent,
            TermLinkOpenRequest,
        )>::default())
            .add_systems(
                Update,
                resend_the_screen_a_page_missed.after(ServiceMessageSet),
            )
            .add_observer(on_term_ready)
            .add_observer(on_term_resize)
            .add_observer(on_term_scroll)
            .add_observer(on_term_link_open);
    }
}

#[derive(Component, Debug, Clone, Copy)]
pub struct TerminalGridSize {
    pub cols: u16,
    pub rows: u16,
}

impl Default for TerminalGridSize {
    fn default() -> Self {
        Self { cols: 80, rows: 24 }
    }
}

#[derive(Component)]
pub(super) struct OwedSnapshot;

fn on_term_ready(
    trigger: On<BinReceive<PageReady>>,
    terminals: Query<&ProcessId, With<Terminal>>,
    service: Option<Res<ServiceClient>>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let Ok(process_id) = terminals.get(entity).copied() else {
        return;
    };
    let Some(service) = service else {
        commands.entity(entity).insert(OwedSnapshot);
        return;
    };
    service
        .0
        .send(ClientMessage::RequestSnapshot { process_id });
}

fn resend_the_screen_a_page_missed(
    owed: Query<(Entity, &ProcessId), (With<Terminal>, With<OwedSnapshot>)>,
    browsers: NonSend<Browsers>,
    service: Option<Res<ServiceClient>>,
    mut commands: Commands,
) {
    let Some(service) = service else { return };
    for (entity, process_id) in &owed {
        if !browsers.can_emit_to(&entity) {
            continue;
        }
        service.0.send(ClientMessage::RequestSnapshot {
            process_id: *process_id,
        });
        commands.entity(entity).remove::<OwedSnapshot>();
    }
}

fn on_term_resize(
    trigger: On<BinReceive<TermResizeEvent>>,
    webviews: Query<&WebviewSize, With<Terminal>>,
    terminals: Query<&ProcessId, With<Terminal>>,
    mut grids: Query<&mut TerminalGridSize, With<Terminal>>,
    service: Option<Res<ServiceClient>>,
) {
    let entity = trigger.event_target();
    let event = &trigger.payload;
    let Ok(webview_size) = webviews.get(entity) else {
        return;
    };
    if event.char_width <= 0.0 || event.char_height <= 0.0 {
        return;
    }

    let viewport_width = if event.viewport_width > 0.0 {
        event.viewport_width
    } else {
        webview_size.0.x
    };
    let viewport_height = if event.viewport_height > 0.0 {
        event.viewport_height
    } else {
        webview_size.0.y
    };
    let cols = (viewport_width / event.char_width).floor().max(1.0) as u16;
    let rows = (viewport_height / event.char_height).floor().max(1.0) as u16;

    if let Ok(mut grid) = grids.get_mut(entity) {
        grid.cols = cols;
        grid.rows = rows;
    }

    let Some(service) = service else { return };
    let Ok(process_id) = terminals.get(entity).copied() else {
        return;
    };
    service.0.send(ClientMessage::ResizeProcess {
        process_id,
        cols,
        rows,
    });
}

fn on_term_scroll(
    trigger: On<BinReceive<TermScrollEvent>>,
    terminals: Query<&ProcessId, With<Terminal>>,
    service: Option<Res<ServiceClient>>,
) {
    let entity = trigger.event_target();
    let event = &trigger.payload;
    let Some(service) = service else { return };
    let Ok(process_id) = terminals.get(entity).copied() else {
        return;
    };
    service.0.send(ClientMessage::ScrollWindow {
        process_id,
        top_row: event.top_row,
        follow: event.follow,
    });
}

fn on_term_link_open(
    trigger: On<BinReceive<TermLinkOpenRequest>>,
    mut stack_requests: MessageWriter<StackRequest>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
) {
    let url = trigger.payload.url.clone();
    if url.is_empty() {
        return;
    }
    stack_requests.write(StackRequest::Open { url: Some(url) });
    if let Some(proxy) = proxy.as_ref() {
        let _ = (**proxy).send_event(WinitUserEvent::WakeUp);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn link_open_emits_stack_open_request() {
        #[derive(Resource, Default)]
        struct Captured(Vec<StackRequest>);

        fn capture(mut requests: MessageReader<StackRequest>, mut captured: ResMut<Captured>) {
            for request in requests.read() {
                captured.0.push(request.clone());
            }
        }

        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<StackRequest>()
            .init_resource::<Captured>()
            .add_observer(on_term_link_open)
            .add_systems(Update, capture);
        let webview = app.world_mut().spawn(vmux_core::team::User).id();

        app.world_mut().trigger(BinReceive::<TermLinkOpenRequest> {
            webview,
            payload: TermLinkOpenRequest {
                url: "https://vmux.ai".into(),
            },
        });
        app.update();

        let captured = app.world().resource::<Captured>();
        assert!(captured.0.iter().any(|request| matches!(
            request,
            StackRequest::Open { url: Some(url) } if url == "https://vmux.ai"
        )));
    }

    #[test]
    fn page_ready_without_service_owes_a_snapshot() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).add_observer(on_term_ready);
        let webview = app.world_mut().spawn((Terminal, ProcessId::new())).id();

        app.world_mut().trigger(BinReceive::<PageReady> {
            webview,
            payload: PageReady {},
        });
        app.update();

        assert!(app.world().get::<OwedSnapshot>(webview).is_some());
    }
}
