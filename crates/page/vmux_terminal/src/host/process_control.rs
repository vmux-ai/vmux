use bevy::prelude::*;
use bevy_cef::prelude::{Browsers, UiEventPlugin, UiInput, WebviewSize};
use vmux_core::page::PageReady;
use vmux_service::client::ServiceRequest;
use vmux_service::plugin::ServiceConnected;
use vmux_service::protocol::{ClientMessage, ProcessId};

use crate::Terminal;
use crate::event::{TermResizeEvent, TermScrollEvent};

use super::plugin::ServiceMessageSet;

pub(super) struct ProcessControlPlugin;

impl Plugin for ProcessControlPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ServiceRequest>()
            .add_plugins(UiEventPlugin::<(TermResizeEvent, TermScrollEvent)>::default())
            .add_systems(
                Update,
                request_pending_terminal_snapshot.after(ServiceMessageSet),
            )
            .add_observer(on_term_ready)
            .add_observer(on_term_resize)
            .add_observer(on_term_scroll);
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
pub(super) struct PendingTerminalSnapshot;

fn on_term_ready(
    trigger: On<UiInput<PageReady>>,
    terminals: Query<&ProcessId, With<Terminal>>,
    connected: Option<Single<(), With<ServiceConnected>>>,
    mut commands: Commands,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    let entity = trigger.event().webview;
    let Ok(process_id) = terminals.get(entity).copied() else {
        return;
    };
    if connected.is_none() {
        commands.entity(entity).insert(PendingTerminalSnapshot);
        return;
    }
    service_requests.write(ServiceRequest(ClientMessage::RequestSnapshot {
        process_id,
    }));
}

fn request_pending_terminal_snapshot(
    pending: Query<(Entity, &ProcessId), (With<Terminal>, With<PendingTerminalSnapshot>)>,
    browsers: NonSend<Browsers>,
    connected: Option<Single<(), With<ServiceConnected>>>,
    mut commands: Commands,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    if connected.is_none() {
        return;
    }
    for (entity, process_id) in &pending {
        if !browsers.can_emit_to(&entity) {
            continue;
        }
        service_requests.write(ServiceRequest(ClientMessage::RequestSnapshot {
            process_id: *process_id,
        }));
        commands.entity(entity).remove::<PendingTerminalSnapshot>();
    }
}

fn on_term_resize(
    trigger: On<UiInput<TermResizeEvent>>,
    webviews: Query<&WebviewSize, With<Terminal>>,
    terminals: Query<&ProcessId, With<Terminal>>,
    mut grids: Query<&mut TerminalGridSize, With<Terminal>>,
    mut service_requests: MessageWriter<ServiceRequest>,
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

    let Ok(process_id) = terminals.get(entity).copied() else {
        return;
    };
    service_requests.write(ServiceRequest(ClientMessage::ResizeProcess {
        process_id,
        cols,
        rows,
    }));
}

fn on_term_scroll(
    trigger: On<UiInput<TermScrollEvent>>,
    terminals: Query<&ProcessId, With<Terminal>>,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    let entity = trigger.event_target();
    let event = &trigger.payload;
    let Ok(process_id) = terminals.get(entity).copied() else {
        return;
    };
    service_requests.write(ServiceRequest(ClientMessage::ScrollWindow {
        process_id,
        top_row: event.top_row,
        follow: event.follow,
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_ready_without_service_owes_a_snapshot() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<ServiceRequest>()
            .add_observer(on_term_ready);
        let webview = app.world_mut().spawn((Terminal, ProcessId::new())).id();

        app.world_mut().trigger(UiInput::<PageReady> {
            webview,
            payload: PageReady {},
        });
        app.update();

        assert!(
            app.world()
                .get::<PendingTerminalSnapshot>(webview)
                .is_some()
        );
    }
}
