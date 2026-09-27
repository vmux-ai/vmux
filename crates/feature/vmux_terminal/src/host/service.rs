use bevy::prelude::*;
use vmux_api::protocol::{ProcessId, ServiceMessage};
use vmux_core::event::TermViewportPatch;
use vmux_service::client::ServiceInbound;

use super::plugin::{
    CommandLifecycleEvent, OscTitleChanged, ProcessExitedEvent, ServiceMessageSet,
};

#[derive(Message)]
pub(crate) struct TerminalProcessCreated {
    pub process_id: ProcessId,
    pub pid: u32,
}

#[derive(Message)]
pub(crate) struct TerminalProcessCreateFailed {
    pub process_id: ProcessId,
    pub reason: String,
}

#[derive(Message)]
pub(crate) struct TerminalViewportUpdate {
    pub process_id: ProcessId,
    pub patch: TermViewportPatch,
    pub request_snapshot_if_hidden: bool,
}

#[derive(Message)]
pub(crate) struct TerminalServiceError {
    pub message: String,
}

#[derive(Message)]
pub(crate) struct TerminalModeUpdate {
    pub process_id: ProcessId,
    pub mouse_capture: bool,
    pub copy_mode: bool,
    pub alt_screen: bool,
    pub focus_reporting: bool,
}

#[derive(Message)]
pub(crate) struct TerminalSelectionText {
    pub process_id: ProcessId,
    pub text: String,
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct ServiceIngressSet;

pub(crate) struct ServiceIngressPlugin;

impl Plugin for ServiceIngressPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<TerminalProcessCreated>()
            .add_message::<TerminalProcessCreateFailed>()
            .add_message::<TerminalViewportUpdate>()
            .add_message::<TerminalServiceError>()
            .add_message::<TerminalModeUpdate>()
            .add_message::<TerminalSelectionText>()
            .add_systems(
                Update,
                route_service_messages
                    .in_set(ServiceIngressSet)
                    .in_set(ServiceMessageSet),
            );
    }
}

#[derive(bevy::ecs::system::SystemParam)]
struct IngressWriters<'w> {
    created: MessageWriter<'w, TerminalProcessCreated>,
    create_failed: MessageWriter<'w, TerminalProcessCreateFailed>,
    viewport: MessageWriter<'w, TerminalViewportUpdate>,
    exited: MessageWriter<'w, ProcessExitedEvent>,
    process_snapshot: MessageWriter<'w, crate::processes_monitor::ServiceProcessSnapshot>,
    service_error: MessageWriter<'w, TerminalServiceError>,
    mode: MessageWriter<'w, TerminalModeUpdate>,
    selection: MessageWriter<'w, TerminalSelectionText>,
    lifecycle: MessageWriter<'w, CommandLifecycleEvent>,
    title: MessageWriter<'w, OscTitleChanged>,
    bell: MessageWriter<'w, vmux_core::notify::BellReceived>,
}

fn route_service_messages(mut inbound: MessageReader<ServiceInbound>, mut writers: IngressWriters) {
    for inbound in inbound.read() {
        match &inbound.0 {
            ServiceMessage::ProcessCreated { process_id, pid } => {
                writers.created.write(TerminalProcessCreated {
                    process_id: *process_id,
                    pid: *pid,
                });
            }
            ServiceMessage::ProcessCreateFailed { process_id, reason } => {
                writers.create_failed.write(TerminalProcessCreateFailed {
                    process_id: *process_id,
                    reason: reason.clone(),
                });
            }
            ServiceMessage::ViewportPatch {
                process_id,
                changed_lines,
                cursor,
                cols,
                rows,
                selection,
                copy_mode,
                full,
                first_row,
                total_rows,
                alt,
                mouse,
                evicted_total,
            } => {
                writers.viewport.write(TerminalViewportUpdate {
                    process_id: *process_id,
                    patch: TermViewportPatch {
                        changed_lines: changed_lines.clone(),
                        cursor: cursor.clone(),
                        cols: *cols,
                        rows: *rows,
                        selection: *selection,
                        copy_mode: *copy_mode,
                        full: *full,
                        first_row: *first_row,
                        total_rows: *total_rows,
                        alt: *alt,
                        mouse: *mouse,
                        evicted_total: *evicted_total,
                    },
                    request_snapshot_if_hidden: true,
                });
            }
            ServiceMessage::ProcessExited { process_id, .. } => {
                writers.exited.write(ProcessExitedEvent {
                    process_id: *process_id,
                });
            }
            ServiceMessage::ProcessTitle { process_id, title } => {
                writers.title.write(OscTitleChanged {
                    process_id: *process_id,
                    title: title.clone(),
                });
            }
            ServiceMessage::CommandLifecycle { process_id, kind } => {
                writers.lifecycle.write(CommandLifecycleEvent {
                    process_id: *process_id,
                    kind: kind.clone(),
                });
            }
            ServiceMessage::ProcessList { processes } => {
                writers
                    .process_snapshot
                    .write(crate::processes_monitor::ServiceProcessSnapshot(
                        processes.clone(),
                    ));
            }
            ServiceMessage::Snapshot {
                process_id,
                lines,
                cursor,
                cols,
                rows,
            } => {
                writers.viewport.write(TerminalViewportUpdate {
                    process_id: *process_id,
                    patch: TermViewportPatch {
                        changed_lines: lines
                            .iter()
                            .cloned()
                            .enumerate()
                            .map(|(index, line)| (index as u32, line))
                            .collect(),
                        cursor: cursor.clone(),
                        cols: *cols,
                        rows: *rows,
                        selection: None,
                        copy_mode: false,
                        full: true,
                        first_row: 0,
                        total_rows: u32::from(*rows),
                        alt: false,
                        mouse: false,
                        evicted_total: 0,
                    },
                    request_snapshot_if_hidden: false,
                });
            }
            ServiceMessage::Error { message } => {
                writers.service_error.write(TerminalServiceError {
                    message: message.clone(),
                });
            }
            ServiceMessage::SelectionText { process_id, text } => {
                writers.selection.write(TerminalSelectionText {
                    process_id: *process_id,
                    text: text.clone(),
                });
            }
            ServiceMessage::TerminalMode {
                process_id,
                mouse_capture,
                copy_mode,
                alt_screen,
                focus_reporting,
            } => {
                writers.mode.write(TerminalModeUpdate {
                    process_id: *process_id,
                    mouse_capture: *mouse_capture,
                    copy_mode: *copy_mode,
                    alt_screen: *alt_screen,
                    focus_reporting: *focus_reporting,
                });
            }
            ServiceMessage::Bell { process_id } => {
                writers.bell.write(vmux_core::notify::BellReceived {
                    process_id: *process_id,
                });
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::message::Messages;

    #[test]
    fn transport_envelopes_become_terminal_messages() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, ServiceIngressPlugin))
            .add_message::<ServiceInbound>()
            .add_message::<ProcessExitedEvent>()
            .add_message::<crate::processes_monitor::ServiceProcessSnapshot>()
            .add_message::<CommandLifecycleEvent>()
            .add_message::<OscTitleChanged>()
            .add_message::<vmux_core::notify::BellReceived>();
        let process_id = ProcessId([7; 16]);
        app.world_mut()
            .write_message(ServiceInbound(ServiceMessage::ProcessCreated {
                process_id,
                pid: 4242,
            }));
        app.world_mut()
            .write_message(ServiceInbound(ServiceMessage::TerminalMode {
                process_id,
                mouse_capture: true,
                copy_mode: false,
                alt_screen: true,
                focus_reporting: true,
            }));

        app.update();

        let created = app
            .world_mut()
            .resource_mut::<Messages<TerminalProcessCreated>>()
            .drain()
            .collect::<Vec<_>>();
        let modes = app
            .world_mut()
            .resource_mut::<Messages<TerminalModeUpdate>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(created.len(), 1);
        assert_eq!(created[0].process_id, process_id);
        assert_eq!(created[0].pid, 4242);
        assert_eq!(modes.len(), 1);
        assert_eq!(modes[0].process_id, process_id);
        assert!(modes[0].mouse_capture);
        assert!(modes[0].alt_screen);
        assert!(modes[0].focus_reporting);
    }
}
