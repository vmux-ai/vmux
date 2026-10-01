use bevy::prelude::*;
use vmux_api::protocol::{CommandLifecycleKind, ProcessId, ProcessInfo};
use vmux_api::{TermCursor, TermLine, TermSelectionRange};
use vmux_core::event::TermViewportPatch;
use vmux_core::service::{ServiceMessageAppExt, ServiceMessageSet};

use super::input_queue::TerminalProcessIndex;
use super::plugin::{CommandLifecycleEvent, OscTitleChanged, ProcessExitedEvent};
use super::state::{TerminalCopyMode, TerminalMode};
use crate::Terminal;

#[vmux_core::service_message(ProcessCreated)]
pub(crate) struct TerminalProcessCreated {
    pub process_id: ProcessId,
    pub pid: u32,
}

#[vmux_core::service_message(ProcessCreateFailed)]
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

#[vmux_core::service_message(Error)]
pub(crate) struct TerminalServiceError {
    pub message: String,
}

#[vmux_core::service_message(SelectionText)]
pub(crate) struct TerminalSelectionText {
    pub process_id: ProcessId,
    pub text: String,
}

#[vmux_core::service_message(ViewportPatch)]
struct ProcessViewportPatch {
    process_id: ProcessId,
    changed_lines: Vec<(u32, TermLine)>,
    cursor: TermCursor,
    cols: u16,
    rows: u16,
    selection: Option<TermSelectionRange>,
    copy_mode: bool,
    full: bool,
    first_row: u32,
    total_rows: u32,
    alt: bool,
    mouse: bool,
    evicted_total: u64,
}

#[vmux_core::service_message(Snapshot)]
struct ProcessSnapshot {
    process_id: ProcessId,
    lines: Vec<TermLine>,
    cursor: TermCursor,
    cols: u16,
    rows: u16,
}

#[vmux_core::service_message(ProcessExited)]
struct ProcessExitedInput {
    process_id: ProcessId,
}

#[vmux_core::service_message(ProcessTitle)]
struct ProcessTitleInput {
    process_id: ProcessId,
    title: String,
}

#[vmux_core::service_message(CommandLifecycle)]
struct ProcessCommandLifecycle {
    process_id: ProcessId,
    kind: CommandLifecycleKind,
}

#[vmux_core::service_message(ProcessList)]
struct ProcessListInput {
    processes: Vec<ProcessInfo>,
}

#[vmux_core::service_message(Bell)]
struct ProcessBell {
    process_id: ProcessId,
}

#[vmux_core::service_message(TerminalMode)]
struct ProcessTerminalMode {
    process_id: ProcessId,
    mouse_capture: bool,
    copy_mode: bool,
}

pub(crate) struct ServiceIngressPlugin;

impl Plugin for ServiceIngressPlugin {
    fn build(&self, app: &mut App) {
        app.add_service_message::<TerminalProcessCreated>()
            .add_service_message::<TerminalProcessCreateFailed>()
            .add_service_message::<ProcessViewportPatch>()
            .add_service_message::<ProcessSnapshot>()
            .add_service_message::<ProcessExitedInput>()
            .add_service_message::<ProcessTitleInput>()
            .add_service_message::<ProcessCommandLifecycle>()
            .add_service_message::<ProcessListInput>()
            .add_service_message::<TerminalServiceError>()
            .add_service_message::<TerminalSelectionText>()
            .add_service_message::<ProcessBell>()
            .add_service_message::<ProcessTerminalMode>()
            .add_message::<TerminalViewportUpdate>()
            .add_systems(
                Update,
                (
                    project_viewport_patches,
                    project_snapshots,
                    project_process_exits,
                    project_process_titles,
                    project_command_lifecycle,
                    project_process_list,
                    project_bells,
                    project_modes,
                )
                    .in_set(ServiceMessageSet)
                    .chain(),
            );
    }
}

fn project_viewport_patches(
    mut patches: MessageReader<ProcessViewportPatch>,
    mut updates: MessageWriter<TerminalViewportUpdate>,
) {
    for patch in patches.read() {
        updates.write(TerminalViewportUpdate {
            process_id: patch.process_id,
            patch: TermViewportPatch {
                changed_lines: patch.changed_lines.clone(),
                cursor: patch.cursor.clone(),
                cols: patch.cols,
                rows: patch.rows,
                selection: patch.selection,
                copy_mode: patch.copy_mode,
                full: patch.full,
                first_row: patch.first_row,
                total_rows: patch.total_rows,
                alt: patch.alt,
                mouse: patch.mouse,
                evicted_total: patch.evicted_total,
            },
            request_snapshot_if_hidden: true,
        });
    }
}

fn project_snapshots(
    mut snapshots: MessageReader<ProcessSnapshot>,
    mut updates: MessageWriter<TerminalViewportUpdate>,
) {
    for snapshot in snapshots.read() {
        updates.write(TerminalViewportUpdate {
            process_id: snapshot.process_id,
            patch: TermViewportPatch {
                changed_lines: snapshot
                    .lines
                    .iter()
                    .cloned()
                    .enumerate()
                    .map(|(index, line)| (index as u32, line))
                    .collect(),
                cursor: snapshot.cursor.clone(),
                cols: snapshot.cols,
                rows: snapshot.rows,
                selection: None,
                copy_mode: false,
                full: true,
                first_row: 0,
                total_rows: u32::from(snapshot.rows),
                alt: false,
                mouse: false,
                evicted_total: 0,
            },
            request_snapshot_if_hidden: false,
        });
    }
}

fn project_process_exits(
    mut inbound: MessageReader<ProcessExitedInput>,
    mut exited: MessageWriter<ProcessExitedEvent>,
) {
    for message in inbound.read() {
        exited.write(ProcessExitedEvent {
            process_id: message.process_id,
        });
    }
}

fn project_process_titles(
    mut inbound: MessageReader<ProcessTitleInput>,
    mut titles: MessageWriter<OscTitleChanged>,
) {
    for message in inbound.read() {
        titles.write(OscTitleChanged {
            process_id: message.process_id,
            title: message.title.clone(),
        });
    }
}

fn project_command_lifecycle(
    mut inbound: MessageReader<ProcessCommandLifecycle>,
    mut lifecycle: MessageWriter<CommandLifecycleEvent>,
) {
    for message in inbound.read() {
        lifecycle.write(CommandLifecycleEvent {
            process_id: message.process_id,
            kind: message.kind.clone(),
        });
    }
}

fn project_process_list(
    mut inbound: MessageReader<ProcessListInput>,
    mut snapshots: MessageWriter<super::process_monitor::ServiceProcessSnapshot>,
) {
    for message in inbound.read() {
        snapshots.write(super::process_monitor::ServiceProcessSnapshot(
            message.processes.clone(),
        ));
    }
}

fn project_bells(
    mut inbound: MessageReader<ProcessBell>,
    mut bells: MessageWriter<vmux_core::notify::BellReceived>,
) {
    for message in inbound.read() {
        bells.write(vmux_core::notify::BellReceived {
            process_id: message.process_id,
        });
    }
}

fn project_modes(
    mut inbound: MessageReader<ProcessTerminalMode>,
    process_index: Single<&TerminalProcessIndex>,
    mut terminals: Query<(&mut TerminalMode, &mut TerminalCopyMode), With<Terminal>>,
) {
    for inbound in inbound.read() {
        let Some(entity) = process_index.get(&inbound.process_id) else {
            continue;
        };
        let Ok((mut mode, mut copy_mode_state)) = terminals.get_mut(entity) else {
            continue;
        };
        *mode = TerminalMode {
            mouse_capture: inbound.mouse_capture,
            copy_mode: inbound.copy_mode,
        };
        copy_mode_state.set(inbound.copy_mode);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::message::Messages;
    use vmux_api::protocol::ServiceMessage;
    use vmux_core::service::ServiceInbound;

    #[test]
    fn transport_envelopes_become_terminal_messages() {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            super::super::input_queue::InputQueuePlugin,
            ServiceIngressPlugin,
        ))
        .add_message::<ServiceInbound>()
        .add_message::<ProcessExitedEvent>()
        .add_message::<crate::process_monitor::ServiceProcessSnapshot>()
        .add_message::<CommandLifecycleEvent>()
        .add_message::<OscTitleChanged>()
        .add_message::<vmux_core::notify::BellReceived>();
        let process_id = ProcessId([7; 16]);
        let terminal = app
            .world_mut()
            .spawn((
                Terminal,
                process_id,
                TerminalMode::default(),
                TerminalCopyMode::default(),
            ))
            .id();
        app.world_mut()
            .write_message(ServiceInbound(ServiceMessage::ProcessCreated {
                process_id,
                pid: 4242,
            }));
        app.world_mut()
            .write_message(ServiceInbound(ServiceMessage::TerminalMode {
                process_id,
                mouse_capture: true,
                copy_mode: true,
                alt_screen: true,
                focus_reporting: true,
            }));

        app.update();

        let created = app
            .world_mut()
            .resource_mut::<Messages<TerminalProcessCreated>>()
            .drain()
            .collect::<Vec<_>>();
        let mode = app.world().get::<TerminalMode>(terminal).unwrap();
        let copy_mode = app.world().get::<TerminalCopyMode>(terminal).unwrap();
        assert_eq!(created.len(), 1);
        assert_eq!(created[0].process_id, process_id);
        assert_eq!(created[0].pid, 4242);
        assert!(mode.mouse_capture);
        assert!(mode.copy_mode);
        assert!(copy_mode.active);
    }
}
