use bevy::prelude::*;
use bevy_cef::prelude::{Browsers, UiEventPlugin, UiInput, WebviewSize};
use vmux_api::protocol::{ClientMessage, ProcessId};
use vmux_ecs::page::PageReady;
use vmux_ecs::service::{ServiceConnected, ServiceMessageSet, ServiceRequest};

use crate::Terminal;
use crate::event::{TermResizeEvent, TermScrollEvent};

pub(super) struct ProcessControlPlugin;

impl Plugin for ProcessControlPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ServiceRequest>()
            .add_plugins(UiEventPlugin::<(TermResizeEvent, TermScrollEvent)>::default())
            .add_systems(Update, request_snapshot.after(ServiceMessageSet))
            .add_observer(term_ready)
            .add_observer(term_resize)
            .add_observer(term_scroll);
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

fn term_ready(
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

fn request_snapshot(
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

fn term_resize(
    trigger: On<UiInput<TermResizeEvent>>,
    webviews: Query<&WebviewSize, With<Terminal>>,
    terminals: Query<&ProcessId, With<Terminal>>,
    mut grids: Query<&mut TerminalGridSize, With<Terminal>>,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    let entity = trigger.event_target();
    let event = &trigger.payload;
    let webview_size = webviews.get(entity).ok();
    if !event.char_width.is_finite()
        || !event.char_height.is_finite()
        || event.char_width <= 0.0
        || event.char_height <= 0.0
    {
        return;
    }

    let viewport_width = if event.viewport_width > 0.0 {
        event.viewport_width
    } else {
        webview_size.map_or(0.0, |size| size.0.x)
    };
    let viewport_height = if event.viewport_height > 0.0 {
        event.viewport_height
    } else {
        webview_size.map_or(0.0, |size| size.0.y)
    };
    if !viewport_width.is_finite()
        || !viewport_height.is_finite()
        || viewport_width < event.char_width
        || viewport_height < event.char_height
    {
        return;
    }
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

fn term_scroll(
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
    fn resize_before_process_creation_is_retained_and_sent_when_bound() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<ServiceRequest>()
            .add_observer(term_resize);
        let entity = app
            .world_mut()
            .spawn((
                Terminal,
                WebviewSize(Vec2::new(1000.0, 768.0)),
                TerminalGridSize::default(),
            ))
            .id();
        let payload = TermResizeEvent {
            char_width: 8.0,
            char_height: 16.0,
            viewport_width: 1000.0,
            viewport_height: 768.0,
        };
        app.world_mut().trigger(UiInput {
            webview: entity,
            payload: payload.clone(),
        });
        let grid = app.world().get::<TerminalGridSize>(entity).unwrap();
        assert_eq!((grid.cols, grid.rows), (125, 48));
        let process_id = ProcessId::new();
        app.world_mut().entity_mut(entity).insert(process_id);
        app.world_mut().trigger(UiInput {
            webview: entity,
            payload,
        });
        let messages: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<ServiceRequest>>()
            .drain()
            .collect();
        assert!(
            matches!(messages.as_slice(), [ServiceRequest(ClientMessage::ResizeProcess { process_id: id, cols: 125, rows: 48 })] if *id == process_id)
        );
    }

    #[test]
    fn page_ready_without_service_owes_a_snapshot() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<ServiceRequest>()
            .add_observer(term_ready);
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

    #[test]
    fn native_page_dimensions_resize_the_pty_without_a_cef_size_component() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<ServiceRequest>()
            .add_observer(term_resize);
        let process_id = ProcessId::new();
        let entity = app
            .world_mut()
            .spawn((Terminal, process_id, TerminalGridSize::default()))
            .id();
        app.world_mut().trigger(UiInput {
            webview: entity,
            payload: TermResizeEvent {
                char_width: 8.0,
                char_height: 16.0,
                viewport_width: 1280.0,
                viewport_height: 1000.0,
            },
        });
        let grid = app.world().get::<TerminalGridSize>(entity).unwrap();
        assert_eq!((grid.cols, grid.rows), (160, 62));
        let requests = app
            .world_mut()
            .resource_mut::<Messages<ServiceRequest>>()
            .drain()
            .collect::<Vec<_>>();
        assert!(
            matches!(requests.as_slice(), [ServiceRequest(ClientMessage::ResizeProcess { process_id: id, cols: 160, rows: 62 })] if *id == process_id)
        );
    }
}
