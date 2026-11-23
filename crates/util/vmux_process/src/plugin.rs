use std::path::Path;
use std::time::{Duration, Instant};

use alacritty_terminal::term::TermMode;
use bevy::platform::cell::SyncCell;
use bevy::prelude::*;
use tokio::sync::broadcast;
use vmux_api::ProcessId;
use vmux_api::protocol::{AgentCommandExit, AgentRunCompletion, ProcessInfo};

use crate::emulator::{ProcessEmulator, TerminalEmulator};
use crate::pty::{NativePty, ProcessPty, PtyInputWriter};
use crate::run_marker::RunMarker;
use crate::runtime_driver::{self as runtime, ProcessCreated, ProcessWake, RuntimeInbox};
use crate::selection::{ProcessCopyMode, ProcessSelection};
use crate::shell_events::ShellEvent;
use crate::shell_integration::ShellIntegration;
use crate::viewport::{
    ProcessCommandLifecycle, ProcessTerminalMode, ProcessUpdate, ProcessViewport,
};

pub struct ProcessPlugin;

impl Plugin for ProcessPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                create,
                ApplyDeferred,
                (
                    subscribe,
                    input,
                    change_directory,
                    mouse_wheel,
                    scroll_window,
                    resize,
                    list,
                    kill,
                    snapshot,
                    set_selection,
                    extend_selection,
                    select_word,
                    select_line,
                    selection_text,
                )
                    .chain(),
                (
                    enter_copy_mode,
                    exit_copy_mode,
                    copy_mode_key,
                    count,
                    output,
                    transcript,
                    command_exit,
                    run_completion,
                    exit_code,
                    #[cfg(any(test, debug_assertions))]
                    feed_output,
                    #[cfg(any(test, debug_assertions))]
                    is_copy_mode,
                )
                    .chain(),
                remove,
                shutdown,
                ApplyDeferred,
                poll,
            )
                .chain(),
        )
        .add_systems(vmux_ecs::service::ShutdownComplete, finish_shutdown);
    }
}

#[derive(Component, Clone)]
pub(crate) struct ProcessShell(String);

#[derive(Component, Clone)]
struct ProcessDirectory(String);

impl ProcessDirectory {
    fn publish(&self, input: &ProcessInput, updates: &ProcessUpdates) {
        let _ = updates.0.send(ProcessUpdate::Directory {
            cwd: self.0.clone(),
            ready: input.0.is_prompt_ready(),
        });
    }

    #[allow(clippy::too_many_arguments)]
    fn apply_change(
        &self,
        exited: bool,
        shell: &str,
        expected_cwd: &str,
        target: &Path,
        copy_mode: &ProcessCopyMode,
        emulator: &TerminalEmulator,
        pty: &NativePty,
        input: &ProcessInput,
        updates: &ProcessUpdates,
    ) -> Result<(), String> {
        if exited || copy_mode.is_active() || emulator.mode().contains(TermMode::ALT_SCREEN) {
            return Err("terminal is not at an empty shell prompt".into());
        }
        if self.0 != expected_cwd {
            return Err("terminal directory has changed".into());
        }
        if !target.is_absolute() || !target.is_dir() {
            return Err("terminal directory is unavailable".into());
        }
        let command = ShellIntegration::directory_command(shell, target)?;
        let drained = !pty.has_queued_output() && pty.reader_drained();
        input.0.write_directory_command(&command, drained)?;
        self.publish(input, updates);
        Ok(())
    }
}

#[derive(Component, Clone, Copy)]
struct ProcessSize {
    cols: u16,
    rows: u16,
}

#[derive(Component, Clone, Copy)]
struct ProcessPid(u32);

#[derive(Component, Clone, Copy)]
struct ProcessStartedAt(Instant);

#[derive(Component, Clone)]
struct ProcessUpdates(broadcast::Sender<ProcessUpdate>);

#[derive(Component, Clone)]
struct ProcessInput(PtyInputWriter);

#[derive(Component, Default, Clone, Copy)]
struct ProcessCommandStatus {
    ended_seq: u64,
    last_exit: Option<i32>,
}

impl ProcessCommandStatus {
    fn record(&mut self, exit_code: Option<i32>) {
        self.ended_seq = self.ended_seq.wrapping_add(1);
        self.last_exit = exit_code;
    }
}

#[derive(Component, Default, Clone)]
struct ProcessRunCompletion(Option<RunMarker>);

#[derive(Component, Default, Clone, Copy)]
struct ProcessExit {
    code: Option<i32>,
    reported: bool,
    drain_quiet_since: Option<Instant>,
}

impl ProcessExit {
    const MAX_PTY_CHUNKS_PER_POLL: usize = 64;
    const PTY_EXIT_QUIET_INTERVAL: Duration = Duration::from_millis(50);

    #[allow(clippy::too_many_arguments)]
    fn poll_process(
        &mut self,
        size: ProcessSize,
        input: &ProcessInput,
        updates: &ProcessUpdates,
        pty: &mut NativePty,
        emulator: &mut TerminalEmulator,
        selection: &mut ProcessSelection,
        copy_mode: &ProcessCopyMode,
        viewport: &mut ProcessViewport,
        terminal_mode: &mut ProcessTerminalMode,
        directory: &mut ProcessDirectory,
        command_status: &mut ProcessCommandStatus,
        run_completion: &mut ProcessRunCompletion,
    ) -> bool {
        let (got_data, pty_closed) = self.drain_chunks(
            input,
            updates,
            pty,
            emulator,
            directory,
            command_status,
            run_completion,
        );
        if got_data {
            selection.clear_on_output(copy_mode.is_active());
            viewport.mark_output_dirty();
            self.drain_quiet_since = None;
        }
        if self.code.is_none()
            && let Some(code) = pty.try_wait()
        {
            self.code = Some(code);
        }
        viewport.maybe_sync_output(
            size.rows,
            pty_closed || self.code.is_some(),
            got_data,
            &input.0,
            emulator,
            selection,
            copy_mode,
            &updates.0,
        );
        terminal_mode.maybe_broadcast(emulator.mode(), copy_mode.is_active(), &updates.0);
        self.maybe_report_exit(pty_closed, pty, updates);
        pty_closed && self.code.is_some()
    }

    #[allow(clippy::too_many_arguments)]
    fn drain_chunks(
        &mut self,
        input: &ProcessInput,
        updates: &ProcessUpdates,
        pty: &mut NativePty,
        emulator: &mut TerminalEmulator,
        directory: &mut ProcessDirectory,
        command_status: &mut ProcessCommandStatus,
        run_completion: &mut ProcessRunCompletion,
    ) -> (bool, bool) {
        let mut got_data = false;
        let mut pty_closed = false;
        let mut remaining = Self::MAX_PTY_CHUNKS_PER_POLL;
        let mut drained_after_exit = false;
        loop {
            if remaining == 0 {
                if self.code.is_none()
                    && let Some(code) = pty.try_wait()
                {
                    self.code = Some(code);
                }
                if self.code.is_some() && !drained_after_exit {
                    remaining = pty.queued_chunks();
                    drained_after_exit = true;
                    if remaining == 0 {
                        break;
                    }
                } else {
                    break;
                }
            }
            let data = match pty.try_recv_chunk() {
                Ok(data) => data,
                Err(tokio::sync::mpsc::error::TryRecvError::Empty) => break,
                Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                    pty_closed = true;
                    break;
                }
            };
            remaining -= 1;
            let (events, markers) = emulator.advance(&data);
            for event in events {
                match event {
                    ShellEvent::Directory(cwd) => {
                        directory.0 = cwd;
                        directory.publish(input, updates);
                    }
                    ShellEvent::CommandStart => {
                        input.0.notify_command_start();
                        directory.publish(input, updates);
                        let _ = updates.0.send(ProcessUpdate::CommandLifecycle(
                            ProcessCommandLifecycle::Started,
                        ));
                    }
                    ShellEvent::CommandEnd(exit_code) => {
                        input.0.notify_command_end();
                        directory.publish(input, updates);
                        command_status.record(exit_code);
                        let _ = updates.0.send(ProcessUpdate::CommandLifecycle(
                            ProcessCommandLifecycle::Ended { exit_code },
                        ));
                    }
                }
            }
            for marker in markers {
                run_completion.0 = Some(marker);
            }
            got_data = true;
        }
        (got_data, pty_closed)
    }

    fn maybe_report_exit(&mut self, pty_closed: bool, pty: &NativePty, updates: &ProcessUpdates) {
        if self.reported {
            return;
        }
        let Some(code) = self.code else {
            return;
        };
        let reader_drained = pty_closed || pty.reader_drained();
        if reader_drained && !pty.has_queued_output() {
            let now = Instant::now();
            let quiet_since = *self.drain_quiet_since.get_or_insert(now);
            if pty_closed
                || now.saturating_duration_since(quiet_since) >= Self::PTY_EXIT_QUIET_INTERVAL
            {
                self.reported = true;
                let _ = updates.0.send(ProcessUpdate::Exited {
                    exit_code: Some(code),
                });
            }
        } else {
            self.drain_quiet_since = None;
        }
    }
}

#[derive(Component)]
struct KeepAfterExit;

#[derive(Component)]
struct Running;

#[derive(Component)]
struct Exited;

#[cfg(test)]
#[derive(Resource)]
struct ReaderSendDelay(Duration);

fn create(
    mut requests: Single<&mut RuntimeInbox<runtime::CreateRequest>>,
    wake: Single<&ProcessWake>,
    existing: Query<&ProcessId, With<ProcessPty>>,
    #[cfg(test)] reader_delay: Option<Res<ReaderSendDelay>>,
    mut commands: Commands,
) {
    #[cfg(test)]
    let delay = reader_delay.map_or(Duration::ZERO, |delay| delay.0);
    #[cfg(not(test))]
    let delay = Duration::ZERO;
    let mut created_ids = Vec::new();
    while let Ok(request) = requests.0.try_recv() {
        let runtime::CreateRequest { launch, reply } = request;
        let exists = existing.iter().any(|id| *id == launch.id) || created_ids.contains(&launch.id);
        let result = if exists {
            Err(format!("process already exists: {}", launch.id))
        } else {
            NativePty::spawn(
                launch.id,
                &launch.command,
                launch.args,
                &launch.cwd,
                launch.env,
                launch.cols,
                launch.rows,
                wake.0.clone(),
                delay,
            )
            .map(|spawned| {
                let (update_tx, _) = broadcast::channel(256);
                let emulator = TerminalEmulator::new(
                    launch.cols,
                    launch.rows,
                    spawned.writer.clone(),
                    update_tx.clone(),
                );
                let created = ProcessCreated {
                    id: launch.id,
                    pid: spawned.pid,
                    updates: update_tx.subscribe(),
                };
                created_ids.push(launch.id);
                let mut entity = commands.spawn((
                    (
                        Name::new(format!("Process {}", launch.id)),
                        launch.id,
                        ProcessShell(launch.command),
                        ProcessDirectory(launch.cwd),
                        ProcessSize {
                            cols: launch.cols,
                            rows: launch.rows,
                        },
                        ProcessPid(spawned.pid),
                        ProcessStartedAt(Instant::now()),
                        ProcessUpdates(update_tx),
                        ProcessInput(spawned.writer),
                        Running,
                    ),
                    (
                        ProcessPty(SyncCell::new(spawned.pty)),
                        ProcessEmulator(SyncCell::new(emulator)),
                        ProcessSelection::default(),
                        ProcessCopyMode::default(),
                        ProcessViewport::default(),
                        ProcessTerminalMode::default(),
                        ProcessCommandStatus::default(),
                        ProcessRunCompletion::default(),
                        ProcessExit::default(),
                    ),
                ));
                if launch.keep_after_exit {
                    entity.insert(KeepAfterExit);
                }
                created
            })
        };
        let _ = reply.send(result);
    }
}

fn subscribe(
    mut requests: Single<&mut RuntimeInbox<runtime::SubscribeRequest>>,
    processes: Query<(
        &ProcessId,
        &ProcessDirectory,
        &ProcessInput,
        &ProcessUpdates,
    )>,
) {
    while let Ok(request) = requests.0.try_recv() {
        let found = processes
            .iter()
            .find(|(id, _, _, _)| **id == request.process_id);
        let result = match found {
            Some((_, directory, input, updates)) => {
                let receiver = updates.0.subscribe();
                directory.publish(input, updates);
                Ok(receiver)
            }
            None => Err(format!("process not found: {}", request.process_id)),
        };
        let _ = request.reply.send(result);
    }
}

fn input(
    mut requests: Single<&mut RuntimeInbox<runtime::InputRequest>>,
    processes: Query<(
        &ProcessId,
        &ProcessCopyMode,
        &ProcessInput,
        &ProcessDirectory,
        &ProcessUpdates,
    )>,
) {
    while let Ok(request) = requests.0.try_recv() {
        let found = processes
            .iter()
            .find(|(id, _, _, _, _)| **id == request.process_id);
        let result = match found {
            Some((_, copy_mode, input, directory, updates)) => {
                if !copy_mode.is_active() {
                    input.0.write_bytes(&request.data);
                    directory.publish(input, updates);
                }
                Ok(())
            }
            None => Err(format!("process not found: {}", request.process_id)),
        };
        let _ = request.reply.send(result);
    }
}

#[allow(clippy::type_complexity)]
fn change_directory(
    mut requests: Single<&mut RuntimeInbox<runtime::DirectoryRequest>>,
    mut processes: Query<(
        (
            &ProcessId,
            &ProcessShell,
            &ProcessSize,
            &ProcessInput,
            &ProcessUpdates,
            Option<&Exited>,
        ),
        (&mut ProcessPty, &mut ProcessEmulator),
        (
            &mut ProcessSelection,
            &ProcessCopyMode,
            &mut ProcessViewport,
            &mut ProcessTerminalMode,
        ),
        (
            &mut ProcessDirectory,
            &mut ProcessCommandStatus,
            &mut ProcessRunCompletion,
            &mut ProcessExit,
        ),
    )>,
) {
    while let Ok(request) = requests.0.try_recv() {
        let found = processes
            .iter_mut()
            .find(|((id, _, _, _, _, _), _, _, _)| **id == request.process_id);
        let result = match found {
            None => Err(format!("process not found: {}", request.process_id)),
            Some((
                (_, shell, size, input, updates, exited),
                (mut pty, mut emulator),
                (mut selection, copy_mode, mut viewport, mut terminal_mode),
                (mut directory, mut command_status, mut run_completion, mut exit),
            )) => {
                let pty = pty.0.get();
                let emulator = emulator.0.get();
                let became_exited = if exited.is_none() {
                    exit.poll_process(
                        *size,
                        input,
                        updates,
                        pty,
                        emulator,
                        &mut selection,
                        copy_mode,
                        &mut viewport,
                        &mut terminal_mode,
                        &mut directory,
                        &mut command_status,
                        &mut run_completion,
                    )
                } else {
                    true
                };
                directory.apply_change(
                    exited.is_some() || became_exited,
                    &shell.0,
                    &request.cwd,
                    Path::new(&request.path),
                    copy_mode,
                    emulator,
                    pty,
                    input,
                    updates,
                )
            }
        };
        let _ = request.reply.send(result);
    }
}

#[allow(clippy::type_complexity)]
fn mouse_wheel(
    mut requests: Single<&mut RuntimeInbox<runtime::MouseWheelRequest>>,
    mut processes: Query<(
        &ProcessId,
        &ProcessSize,
        &ProcessInput,
        &ProcessUpdates,
        &mut ProcessEmulator,
        &mut ProcessSelection,
        &mut ProcessCopyMode,
        &mut ProcessViewport,
    )>,
) {
    while let Ok(request) = requests.0.try_recv() {
        let found = processes
            .iter_mut()
            .find(|(id, _, _, _, _, _, _, _)| **id == request.process_id);
        let result = match found {
            None => Err(format!("process not found: {}", request.process_id)),
            Some((
                _,
                size,
                input,
                updates,
                mut emulator,
                mut selection,
                mut copy_mode,
                mut viewport,
            )) => {
                let emulator = emulator.0.get();
                let delta = if request.up { 1 } else { -1 };
                if copy_mode.is_active() {
                    if copy_mode.scroll_viewport(
                        delta,
                        emulator,
                        &input.0,
                        size.cols,
                        size.rows,
                        &mut viewport,
                    ) != 0
                    {
                        viewport.sync(emulator, &mut selection, &copy_mode, &updates.0);
                    }
                } else {
                    let mode = emulator.mode();
                    if mode.intersects(TermMode::MOUSE_MODE) {
                        let bytes = ProcessViewport::sgr_mouse_wheel_bytes(
                            request.up,
                            request.col,
                            request.row,
                            request.modifiers,
                        );
                        input.0.write_bytes(&bytes);
                    } else if mode.contains(TermMode::ALT_SCREEN)
                        && mode.contains(TermMode::ALTERNATE_SCROLL)
                    {
                        let bytes = ProcessViewport::alternate_scroll_bytes(
                            request.up,
                            mode.contains(TermMode::APP_CURSOR),
                        );
                        input.0.write_bytes(bytes);
                    }
                }
                Ok(())
            }
        };
        let _ = request.reply.send(result);
    }
}

#[allow(clippy::type_complexity)]
fn scroll_window(
    mut requests: Single<&mut RuntimeInbox<runtime::ScrollWindowRequest>>,
    mut processes: Query<(
        &ProcessId,
        &ProcessUpdates,
        &mut ProcessEmulator,
        &mut ProcessSelection,
        &ProcessCopyMode,
        &mut ProcessViewport,
    )>,
) {
    while let Ok(request) = requests.0.try_recv() {
        let found = processes
            .iter_mut()
            .find(|(id, _, _, _, _, _)| **id == request.process_id);
        let result = match found {
            None => Err(format!("process not found: {}", request.process_id)),
            Some((_, updates, mut emulator, mut selection, copy_mode, mut viewport)) => {
                viewport.scroll_window(request.top_row, request.follow);
                viewport.sync(emulator.0.get(), &mut selection, copy_mode, &updates.0);
                Ok(())
            }
        };
        let _ = request.reply.send(result);
    }
}

#[allow(clippy::type_complexity)]
fn resize(
    mut requests: Single<&mut RuntimeInbox<runtime::ResizeRequest>>,
    mut processes: Query<(
        &ProcessId,
        &mut ProcessSize,
        &mut ProcessPty,
        &mut ProcessEmulator,
        &mut ProcessSelection,
        &mut ProcessCopyMode,
        &mut ProcessViewport,
    )>,
) {
    while let Ok(request) = requests.0.try_recv() {
        let found = processes
            .iter_mut()
            .find(|(id, _, _, _, _, _, _)| **id == request.process_id);
        let result = match found {
            None => Err(format!("process not found: {}", request.process_id)),
            Some((
                _,
                mut size,
                mut pty,
                mut emulator,
                mut selection,
                mut copy_mode,
                mut viewport,
            )) => {
                size.cols = request.cols;
                size.rows = request.rows;
                pty.0.get().resize(request.cols, request.rows);
                emulator.0.get().resize(request.cols, request.rows);
                viewport.invalidate_on_resize();
                copy_mode.clamp_on_resize(request.cols, request.rows);
                selection.clamp_on_resize(request.cols, request.rows);
                Ok(())
            }
        };
        let _ = request.reply.send(result);
    }
}

fn list(
    mut requests: Single<&mut RuntimeInbox<runtime::ListRequest>>,
    processes: Query<(
        &ProcessId,
        &ProcessShell,
        &ProcessDirectory,
        &ProcessSize,
        &ProcessPid,
        &ProcessStartedAt,
    )>,
) {
    while let Ok(request) = requests.0.try_recv() {
        let mut values = Vec::new();
        for (id, shell, directory, size, pid, started_at) in &processes {
            values.push(ProcessInfo {
                id: *id,
                shell: shell.0.clone(),
                cwd: directory.0.clone(),
                cols: size.cols,
                rows: size.rows,
                pid: pid.0,
                created_at_secs: started_at.0.elapsed().as_secs(),
            });
        }
        let _ = request.reply.send(values);
    }
}

fn remove(
    mut requests: Single<&mut RuntimeInbox<runtime::RemoveRequest>>,
    mut processes: Query<(Entity, &ProcessId, &mut ProcessPty, &mut ProcessExit)>,
    mut commands: Commands,
) {
    while let Ok(request) = requests.0.try_recv() {
        if let Some((entity, _, mut pty, mut exit)) = processes
            .iter_mut()
            .find(|(_, id, _, _)| **id == request.process_id)
        {
            if let Some(code) = pty.0.get().kill() {
                exit.code.get_or_insert(code);
            }
            commands.entity(entity).despawn();
        }
        let _ = request.reply.send(());
    }
}

fn kill(
    mut requests: Single<&mut RuntimeInbox<runtime::KillRequest>>,
    mut processes: Query<(&ProcessId, &mut ProcessPty, &mut ProcessExit)>,
) {
    while let Ok(request) = requests.0.try_recv() {
        let found = processes
            .iter_mut()
            .find(|(id, _, _)| **id == request.process_id);
        let result = match found {
            None => Err(format!("process not found: {}", request.process_id)),
            Some((_, mut pty, mut exit)) => {
                if let Some(code) = pty.0.get().kill() {
                    exit.code.get_or_insert(code);
                }
                Ok(())
            }
        };
        let _ = request.reply.send(result);
    }
}

fn snapshot(
    mut requests: Single<&mut RuntimeInbox<runtime::SnapshotRequest>>,
    mut processes: Query<(&ProcessId, &mut ProcessEmulator)>,
) {
    while let Ok(request) = requests.0.try_recv() {
        let result = processes
            .iter_mut()
            .find(|(id, _)| **id == request.process_id)
            .map(|(_, mut emulator)| emulator.0.get().snapshot())
            .ok_or_else(|| format!("process not found: {}", request.process_id));
        let _ = request.reply.send(result);
    }
}

#[allow(clippy::type_complexity)]
fn set_selection(
    mut requests: Single<&mut RuntimeInbox<runtime::SetSelectionRequest>>,
    mut processes: Query<(
        &ProcessId,
        &ProcessSize,
        &ProcessUpdates,
        &mut ProcessEmulator,
        &mut ProcessSelection,
        &ProcessCopyMode,
        &mut ProcessViewport,
    )>,
) {
    while let Ok(request) = requests.0.try_recv() {
        let found = processes
            .iter_mut()
            .find(|(id, _, _, _, _, _, _)| **id == request.process_id);
        let result = match found {
            None => Err(format!("process not found: {}", request.process_id)),
            Some((_, size, updates, mut emulator, mut selection, copy_mode, mut viewport)) => {
                selection.set(request.range, size.cols, size.rows);
                viewport.sync(emulator.0.get(), &mut selection, copy_mode, &updates.0);
                Ok(())
            }
        };
        let _ = request.reply.send(result);
    }
}

#[allow(clippy::type_complexity)]
fn extend_selection(
    mut requests: Single<&mut RuntimeInbox<runtime::ExtendSelectionRequest>>,
    mut processes: Query<(
        &ProcessId,
        &ProcessSize,
        &ProcessUpdates,
        &mut ProcessEmulator,
        &mut ProcessSelection,
        &ProcessCopyMode,
        &mut ProcessViewport,
    )>,
) {
    while let Ok(request) = requests.0.try_recv() {
        let found = processes
            .iter_mut()
            .find(|(id, _, _, _, _, _, _)| **id == request.process_id);
        let result = match found {
            None => Err(format!("process not found: {}", request.process_id)),
            Some((_, size, updates, mut emulator, mut selection, copy_mode, mut viewport)) => {
                selection.extend_to(request.col, request.row, size.cols, size.rows);
                viewport.sync(emulator.0.get(), &mut selection, copy_mode, &updates.0);
                Ok(())
            }
        };
        let _ = request.reply.send(result);
    }
}

#[allow(clippy::type_complexity)]
fn select_word(
    mut requests: Single<&mut RuntimeInbox<runtime::SelectWordRequest>>,
    mut processes: Query<(
        &ProcessId,
        &ProcessUpdates,
        &mut ProcessEmulator,
        &mut ProcessSelection,
        &ProcessCopyMode,
        &mut ProcessViewport,
    )>,
) {
    while let Ok(request) = requests.0.try_recv() {
        let found = processes
            .iter_mut()
            .find(|(id, _, _, _, _, _)| **id == request.process_id);
        let result = match found {
            None => Err(format!("process not found: {}", request.process_id)),
            Some((_, updates, mut emulator, mut selection, copy_mode, mut viewport)) => {
                let emulator = emulator.0.get();
                if selection.select_word_at(emulator, request.col, request.row) {
                    viewport.sync(emulator, &mut selection, copy_mode, &updates.0);
                }
                Ok(())
            }
        };
        let _ = request.reply.send(result);
    }
}

#[allow(clippy::type_complexity)]
fn select_line(
    mut requests: Single<&mut RuntimeInbox<runtime::SelectLineRequest>>,
    mut processes: Query<(
        &ProcessId,
        &ProcessUpdates,
        &mut ProcessEmulator,
        &mut ProcessSelection,
        &ProcessCopyMode,
        &mut ProcessViewport,
    )>,
) {
    while let Ok(request) = requests.0.try_recv() {
        let found = processes
            .iter_mut()
            .find(|(id, _, _, _, _, _)| **id == request.process_id);
        let result = match found {
            None => Err(format!("process not found: {}", request.process_id)),
            Some((_, updates, mut emulator, mut selection, copy_mode, mut viewport)) => {
                let emulator = emulator.0.get();
                if selection.select_line_at(emulator, request.row) {
                    viewport.sync(emulator, &mut selection, copy_mode, &updates.0);
                }
                Ok(())
            }
        };
        let _ = request.reply.send(result);
    }
}

fn selection_text(
    mut requests: Single<&mut RuntimeInbox<runtime::SelectionTextRequest>>,
    mut processes: Query<(&ProcessId, &ProcessSelection, &mut ProcessEmulator)>,
) {
    while let Ok(request) = requests.0.try_recv() {
        let result = processes
            .iter_mut()
            .find(|(id, _, _)| **id == request.process_id)
            .map(|(_, selection, mut emulator)| {
                selection.text(emulator.0.get()).unwrap_or_default()
            })
            .ok_or_else(|| format!("process not found: {}", request.process_id));
        let _ = request.reply.send(result);
    }
}

#[allow(clippy::type_complexity)]
fn enter_copy_mode(
    mut requests: Single<&mut RuntimeInbox<runtime::EnterCopyModeRequest>>,
    mut processes: Query<(
        &ProcessId,
        &ProcessSize,
        &ProcessUpdates,
        &mut ProcessEmulator,
        &mut ProcessSelection,
        &mut ProcessCopyMode,
        &mut ProcessViewport,
        &mut ProcessTerminalMode,
    )>,
) {
    while let Ok(request) = requests.0.try_recv() {
        let found = processes
            .iter_mut()
            .find(|(id, _, _, _, _, _, _, _)| **id == request.process_id);
        let result = match found {
            None => Err(format!("process not found: {}", request.process_id)),
            Some((
                _,
                size,
                updates,
                mut emulator,
                mut selection,
                mut copy_mode,
                mut viewport,
                mut terminal_mode,
            )) => {
                let emulator = emulator.0.get();
                copy_mode.enter(emulator, size.cols, size.rows, &mut selection);
                terminal_mode.maybe_broadcast(emulator.mode(), copy_mode.is_active(), &updates.0);
                viewport.sync(emulator, &mut selection, &copy_mode, &updates.0);
                Ok(())
            }
        };
        let _ = request.reply.send(result);
    }
}

#[allow(clippy::type_complexity)]
fn exit_copy_mode(
    mut requests: Single<&mut RuntimeInbox<runtime::ExitCopyModeRequest>>,
    mut processes: Query<(
        &ProcessId,
        &ProcessUpdates,
        &mut ProcessEmulator,
        &mut ProcessSelection,
        &mut ProcessCopyMode,
        &mut ProcessViewport,
        &mut ProcessTerminalMode,
    )>,
) {
    while let Ok(request) = requests.0.try_recv() {
        let found = processes
            .iter_mut()
            .find(|(id, _, _, _, _, _, _)| **id == request.process_id);
        let result = match found {
            None => Err(format!("process not found: {}", request.process_id)),
            Some((
                _,
                updates,
                mut emulator,
                mut selection,
                mut copy_mode,
                mut viewport,
                mut terminal_mode,
            )) => {
                let emulator = emulator.0.get();
                copy_mode.exit();
                terminal_mode.maybe_broadcast(emulator.mode(), copy_mode.is_active(), &updates.0);
                viewport.sync(emulator, &mut selection, &copy_mode, &updates.0);
                Ok(())
            }
        };
        let _ = request.reply.send(result);
    }
}

#[allow(clippy::type_complexity)]
fn copy_mode_key(
    mut requests: Single<&mut RuntimeInbox<runtime::CopyModeRequest>>,
    mut processes: Query<(
        (&ProcessId, &ProcessSize, &ProcessInput, &ProcessUpdates),
        &mut ProcessEmulator,
        &mut ProcessSelection,
        &mut ProcessCopyMode,
        &mut ProcessViewport,
        &mut ProcessTerminalMode,
    )>,
) {
    while let Ok(request) = requests.0.try_recv() {
        let found = processes
            .iter_mut()
            .find(|((id, _, _, _), _, _, _, _, _)| **id == request.process_id);
        let result = match found {
            None => Err(format!("process not found: {}", request.process_id)),
            Some((
                (_, size, input, updates),
                mut emulator,
                mut selection,
                mut copy_mode,
                mut viewport,
                mut terminal_mode,
            )) => {
                let emulator = emulator.0.get();
                let was_active = copy_mode.is_active();
                let (mode_changed, copied) = copy_mode.apply_key(
                    request.key,
                    emulator,
                    &input.0,
                    size.cols,
                    size.rows,
                    &mut selection,
                    &mut viewport,
                );
                if was_active {
                    if mode_changed {
                        terminal_mode.maybe_broadcast(
                            emulator.mode(),
                            copy_mode.is_active(),
                            &updates.0,
                        );
                    }
                    viewport.sync(emulator, &mut selection, &copy_mode, &updates.0);
                }
                Ok(copied)
            }
        };
        let _ = request.reply.send(result);
    }
}

fn shutdown(
    mut requests: Single<&mut RuntimeInbox<runtime::ShutdownRequest>>,
    mut processes: Query<(Entity, &mut ProcessPty)>,
    mut commands: Commands,
) {
    while let Ok(request) = requests.0.try_recv() {
        for (entity, mut pty) in &mut processes {
            pty.0.get().kill();
            commands.entity(entity).despawn();
        }
        let _ = request.reply.send(());
    }
}

fn finish_shutdown(mut processes: Query<(Entity, &mut ProcessPty)>, mut commands: Commands) {
    for (entity, mut pty) in &mut processes {
        pty.0.get().kill();
        commands.entity(entity).despawn();
    }
}

fn count(
    mut requests: Single<&mut RuntimeInbox<runtime::CountRequest>>,
    processes: Query<&ProcessId, With<ProcessPty>>,
) {
    while let Ok(request) = requests.0.try_recv() {
        let _ = request.reply.send(processes.iter().count() as u32);
    }
}

fn output(
    mut requests: Single<&mut RuntimeInbox<runtime::OutputRequest>>,
    mut processes: Query<(&ProcessId, &mut ProcessEmulator)>,
) {
    while let Ok(request) = requests.0.try_recv() {
        let result = processes
            .iter_mut()
            .find(|(id, _)| **id == request.process_id)
            .map(|(_, mut emulator)| emulator.0.get().visible_text())
            .ok_or_else(|| format!("process not found: {}", request.process_id));
        let _ = request.reply.send(result);
    }
}

fn transcript(
    mut requests: Single<&mut RuntimeInbox<runtime::TranscriptRequest>>,
    mut processes: Query<(&ProcessId, &mut ProcessEmulator)>,
) {
    while let Ok(request) = requests.0.try_recv() {
        let result = processes
            .iter_mut()
            .find(|(id, _)| **id == request.process_id)
            .map(|(_, mut emulator)| emulator.0.get().full_text())
            .ok_or_else(|| format!("process not found: {}", request.process_id));
        let _ = request.reply.send(result);
    }
}

fn command_exit(
    mut requests: Single<&mut RuntimeInbox<runtime::CommandExitRequest>>,
    processes: Query<(&ProcessId, &ProcessCommandStatus)>,
) {
    while let Ok(request) = requests.0.try_recv() {
        let result = processes
            .iter()
            .find(|(id, _)| **id == request.process_id)
            .map(|(_, status)| AgentCommandExit {
                sequence: status.ended_seq,
                exit: status.last_exit,
            })
            .ok_or_else(|| format!("process not found: {}", request.process_id));
        let _ = request.reply.send(result);
    }
}

fn run_completion(
    mut requests: Single<&mut RuntimeInbox<runtime::RunCompletionRequest>>,
    processes: Query<(&ProcessId, &ProcessRunCompletion)>,
) {
    while let Ok(request) = requests.0.try_recv() {
        let result = processes
            .iter()
            .find(|(id, _)| **id == request.process_id)
            .map(|(_, completion)| match &completion.0 {
                Some(marker) => AgentRunCompletion {
                    token: Some(marker.token.clone()),
                    exit: Some(marker.exit),
                },
                None => AgentRunCompletion {
                    token: None,
                    exit: None,
                },
            })
            .ok_or_else(|| format!("process not found: {}", request.process_id));
        let _ = request.reply.send(result);
    }
}

fn exit_code(
    mut requests: Single<&mut RuntimeInbox<runtime::ExitCodeRequest>>,
    processes: Query<(&ProcessId, &ProcessExit)>,
) {
    while let Ok(request) = requests.0.try_recv() {
        let result = processes
            .iter()
            .find(|(id, _)| **id == request.process_id)
            .map(|(_, exit)| exit.code)
            .ok_or_else(|| format!("process not found: {}", request.process_id));
        let _ = request.reply.send(result);
    }
}

#[cfg(any(test, debug_assertions))]
#[allow(clippy::type_complexity)]
fn feed_output(
    mut requests: Single<&mut RuntimeInbox<runtime::FeedOutputRequest>>,
    mut processes: Query<(
        &ProcessId,
        &ProcessUpdates,
        &mut ProcessEmulator,
        &mut ProcessSelection,
        &ProcessCopyMode,
        &mut ProcessViewport,
        &mut ProcessTerminalMode,
    )>,
) {
    while let Ok(request) = requests.0.try_recv() {
        let found = processes
            .iter_mut()
            .find(|(id, _, _, _, _, _, _)| **id == request.process_id);
        let result = match found {
            None => Err(format!("process not found: {}", request.process_id)),
            Some((
                _,
                updates,
                mut emulator,
                mut selection,
                copy_mode,
                mut viewport,
                mut terminal_mode,
            )) => {
                let emulator = emulator.0.get();
                emulator.advance_term_only(&request.data);
                selection.clear_on_output(copy_mode.is_active());
                viewport.sync(emulator, &mut selection, copy_mode, &updates.0);
                terminal_mode.maybe_broadcast(emulator.mode(), copy_mode.is_active(), &updates.0);
                Ok(())
            }
        };
        let _ = request.reply.send(result);
    }
}

#[cfg(any(test, debug_assertions))]
fn is_copy_mode(
    mut requests: Single<&mut RuntimeInbox<runtime::IsCopyModeRequest>>,
    processes: Query<(&ProcessId, &ProcessCopyMode)>,
) {
    while let Ok(request) = requests.0.try_recv() {
        let result = processes
            .iter()
            .find(|(id, _)| **id == request.process_id)
            .map(|(_, copy_mode)| copy_mode.is_active())
            .ok_or_else(|| format!("process not found: {}", request.process_id));
        let _ = request.reply.send(result);
    }
}

#[allow(clippy::type_complexity)]
fn poll(
    mut processes: Query<
        (
            Entity,
            (&ProcessSize, &ProcessInput, &ProcessUpdates),
            (&mut ProcessPty, &mut ProcessEmulator),
            (
                &mut ProcessSelection,
                &ProcessCopyMode,
                &mut ProcessViewport,
                &mut ProcessTerminalMode,
            ),
            (
                &mut ProcessDirectory,
                &mut ProcessCommandStatus,
                &mut ProcessRunCompletion,
                &mut ProcessExit,
            ),
            Option<&KeepAfterExit>,
        ),
        Without<Exited>,
    >,
    mut commands: Commands,
) {
    for (
        entity,
        (size, input, updates),
        (mut pty, mut emulator),
        (mut selection, copy_mode, mut viewport, mut terminal_mode),
        (mut directory, mut command_status, mut run_completion, mut exit),
        keep_after_exit,
    ) in &mut processes
    {
        let pty = pty.0.get();
        let exited = exit.poll_process(
            *size,
            input,
            updates,
            pty,
            emulator.0.get(),
            &mut selection,
            copy_mode,
            &mut viewport,
            &mut terminal_mode,
            &mut directory,
            &mut command_status,
            &mut run_completion,
        );
        if !exited {
            continue;
        }
        if keep_after_exit.is_some() {
            pty.drain_reader();
            commands.entity(entity).remove::<Running>().insert(Exited);
            continue;
        }
        pty.kill();
        commands.entity(entity).despawn();
    }
}

#[cfg(test)]
mod tests {
    use std::io::{self, Write};
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use alacritty_terminal::event::{Event as TermEvent, EventListener as TermEventListener};
    use tokio::sync::{broadcast, mpsc};
    use vmux_api::protocol::CopyModeKey;

    use super::*;
    use crate::emulator::ServiceEventProxy;
    use crate::viewport::ViewportPace;
    use crate::{ProcessLaunch, ProcessRuntime, ProcessSnapshot};

    #[derive(Clone)]
    struct CapturingWriter(Arc<Mutex<Vec<u8>>>);

    impl Write for CapturingWriter {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    struct ProcessFixture {
        app: App,
        runtime: ProcessRuntime,
        rt: tokio::runtime::Runtime,
        wake_rx: mpsc::UnboundedReceiver<()>,
    }

    impl ProcessFixture {
        fn new() -> Self {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("tokio test runtime");
            let (wake, wake_rx) = mpsc::unbounded_channel();
            let (runtime, inboxes) = ProcessRuntime::new(wake);
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, ProcessPlugin));
            app.world_mut()
                .spawn((Name::new("vmux process runtime"), inboxes));
            Self {
                app,
                runtime,
                rt,
                wake_rx,
            }
        }

        fn with_reader_delay(delay: Duration) -> Self {
            let mut fixture = Self::new();
            fixture.app.insert_resource(ReaderSendDelay(delay));
            fixture
        }

        fn run<T: Send + 'static>(
            &mut self,
            future: impl std::future::Future<Output = T> + Send + 'static,
        ) -> T {
            let handle = self.rt.spawn(future);
            let deadline = Instant::now() + Duration::from_secs(5);
            while !handle.is_finished() {
                assert!(
                    Instant::now() < deadline,
                    "process runtime request timed out"
                );
                self.rt.block_on(tokio::task::yield_now());
                self.app.update();
            }
            self.rt.block_on(handle).unwrap()
        }

        fn spawn(&mut self, launch: ProcessLaunch) -> Result<ProcessCreated, String> {
            let runtime = self.runtime.clone();
            self.run(async move { runtime.create(launch).await })
        }

        fn spawn_capturing(
            &mut self,
            cols: u16,
            rows: u16,
        ) -> (
            ProcessId,
            broadcast::Receiver<ProcessUpdate>,
            Arc<Mutex<Vec<u8>>>,
        ) {
            let id = ProcessId::new();
            let created = self
                .spawn(ProcessLaunch {
                    id,
                    command: "/bin/sh".into(),
                    args: Vec::new(),
                    cwd: String::new(),
                    env: Vec::new(),
                    cols,
                    rows,
                    keep_after_exit: false,
                })
                .expect("process should spawn");
            let captured = Arc::new(Mutex::new(Vec::new()));
            let entity = self.entity(id).expect("process entity");
            self.app
                .world_mut()
                .entity_mut(entity)
                .insert(ProcessInput(PtyInputWriter::new(Box::new(
                    CapturingWriter(Arc::clone(&captured)),
                ))));
            (id, created.updates, captured)
        }

        fn entity(&mut self, id: ProcessId) -> Option<Entity> {
            let mut query = self.app.world_mut().query::<(Entity, &ProcessId)>();
            query
                .iter(self.app.world())
                .find(|(_, process_id)| **process_id == id)
                .map(|(entity, _)| entity)
        }

        fn drain_output(&mut self, duration: Duration) {
            let deadline = Instant::now() + duration;
            while Instant::now() < deadline {
                self.app.update();
                std::thread::sleep(Duration::from_millis(10));
            }
        }

        fn wait_for_snapshot_text(&mut self, id: ProcessId, needle: &str) -> String {
            let deadline = Instant::now() + Duration::from_secs(2);
            loop {
                self.app.update();
                let runtime = self.runtime.clone();
                let snapshot = self
                    .run(async move { runtime.snapshot(id).await })
                    .expect("snapshot");
                let text = Self::snapshot_text(snapshot);
                if text.contains(needle) || Instant::now() >= deadline {
                    return text;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }

        fn snapshot_text(snapshot: ProcessSnapshot) -> String {
            snapshot
                .lines
                .iter()
                .map(|line| {
                    line.spans
                        .iter()
                        .map(|span| span.text.as_str())
                        .collect::<String>()
                })
                .collect::<Vec<_>>()
                .join("\n")
        }

        fn shutdown(&mut self) {
            self.app
                .world_mut()
                .run_schedule(vmux_ecs::service::ShutdownComplete);
            self.app.update();
        }
    }

    impl Drop for ProcessFixture {
        fn drop(&mut self) {
            self.shutdown();
        }
    }

    struct DirectoryFixture {
        fixture: ProcessFixture,
        id: ProcessId,
        root: PathBuf,
        target: PathBuf,
    }

    impl DirectoryFixture {
        fn start() -> Self {
            let root =
                std::env::temp_dir().join(format!("vmux-directory-pty-{}", ProcessId::new()));
            let target = root.join("space ' $name `false` % # ?");
            std::fs::create_dir_all(&target).unwrap();
            let root = root.canonicalize().unwrap();
            let target = target.canonicalize().unwrap();
            let mut fixture = ProcessFixture::new();
            let id = ProcessId::new();
            fixture
                .spawn(ProcessLaunch {
                    id,
                    command: "/bin/bash".into(),
                    args: vec!["--noprofile".into(), "-i".into()],
                    cwd: root.to_string_lossy().into_owned(),
                    env: vec![("PATH".into(), "/usr/bin:/bin".into())],
                    cols: 160,
                    rows: 24,
                    keep_after_exit: false,
                })
                .unwrap();
            Self {
                fixture,
                id,
                root,
                target,
            }
        }

        fn ready(&mut self, target: &Path) {
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                self.fixture.app.update();
                let entity = self.fixture.entity(self.id).unwrap();
                let cwd = self
                    .fixture
                    .app
                    .world()
                    .get::<ProcessDirectory>(entity)
                    .unwrap()
                    .0
                    .clone();
                let ready = self
                    .fixture
                    .app
                    .world()
                    .get::<ProcessInput>(entity)
                    .unwrap()
                    .0
                    .is_prompt_ready();
                let drained = self
                    .fixture
                    .app
                    .world_mut()
                    .get_mut::<ProcessPty>(entity)
                    .unwrap()
                    .0
                    .get()
                    .reader_drained();
                if Path::new(&cwd) == target && ready && drained {
                    return;
                }
                if Instant::now() >= deadline {
                    let runtime = self.fixture.runtime.clone();
                    let id = self.id;
                    let text = self
                        .fixture
                        .run(async move { runtime.transcript(id).await })
                        .unwrap();
                    panic!("shell directory or prompt did not settle: {text}");
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    }

    impl Drop for DirectoryFixture {
        fn drop(&mut self) {
            self.fixture.shutdown();
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn missing_process_query_is_answered_by_ecs() {
        let mut fixture = ProcessFixture::new();
        let process_id = ProcessId::new();
        let runtime = fixture.runtime.clone();
        let result = fixture.run(async move { runtime.output(process_id).await });
        assert_eq!(result, Err(format!("process not found: {process_id}")));
    }

    #[test]
    fn terminal_directory_changes_use_the_shell_and_preserve_unsubmitted_input() {
        let mut dir = DirectoryFixture::start();
        let root = dir.root.clone();
        let target = dir.target.clone();
        dir.ready(&root);

        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let runtime = dir.fixture.runtime.clone();
            let id = dir.id;
            let cwd = root.to_string_lossy().into_owned();
            let next = target.to_string_lossy().into_owned();
            match dir
                .fixture
                .run(async move { runtime.change_directory(id, cwd, next).await })
            {
                Ok(()) => break,
                Err(error) => {
                    assert_eq!(error, "terminal is not at an empty shell prompt");
                    assert!(
                        Instant::now() < deadline,
                        "shell directory change was never admitted"
                    );
                }
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        dir.ready(&target);

        let runtime = dir.fixture.runtime.clone();
        let id = dir.id;
        dir.fixture
            .run(async move { runtime.input(id, b"draft stays here".to_vec()).await })
            .unwrap();
        let runtime = dir.fixture.runtime.clone();
        let cwd = target.to_string_lossy().into_owned();
        let next = root.to_string_lossy().into_owned();
        assert!(
            dir.fixture
                .run(async move { runtime.change_directory(id, cwd, next).await })
                .is_err()
        );
        let entity = dir.fixture.entity(id).unwrap();
        assert_eq!(
            dir.fixture
                .app
                .world()
                .get::<ProcessDirectory>(entity)
                .unwrap()
                .0,
            target.to_string_lossy()
        );
        let text = dir.fixture.wait_for_snapshot_text(id, "draft stays here");
        assert!(text.contains("draft stays here"));
    }

    #[test]
    fn heavy_output_waits_for_frame_interval_but_sparse_and_final_output_do_not() {
        let start = Instant::now();
        let mut heavy = ViewportPace::default();
        heavy.sent(start, 50, 50);

        assert!(!heavy.admits(
            start + ViewportPace::HEAVY_OUTPUT_FRAME_INTERVAL - Duration::from_millis(1),
            false,
            false,
        ));
        assert!(heavy.admits(
            start + ViewportPace::HEAVY_OUTPUT_FRAME_INTERVAL,
            false,
            false
        ));
        assert!(heavy.admits(start, true, false));

        let mut sparse = ViewportPace::default();
        sparse.sent(start, 1, 50);
        assert!(sparse.admits(start, false, false));
    }

    #[test]
    fn a_keystroke_against_a_screen_that_just_repainted_does_not_wait() {
        let start = Instant::now();
        let mut pace = ViewportPace::default();
        pace.sent(start, 50, 50);
        let now = start + Duration::from_millis(10);

        assert!(!pace.admits(now, false, false));
        assert!(pace.admits(now, false, true));
    }

    #[test]
    fn holding_a_key_is_paced_by_the_frame_interval() {
        let start = Instant::now();
        let repeat = Duration::from_millis(33);
        let mut pace = ViewportPace::default();

        let mut sent = 0;
        for beat in 0..30 {
            let now = start + repeat * beat;
            if pace.admits(now, false, true) {
                sent += 1;
                pace.sent(now, 50, 50);
            }
        }

        assert!(
            sent <= 10,
            "a second of 30Hz key repeat sent {sent} viewports; the frame interval caps it near 10"
        );
    }

    #[test]
    fn input_priority_waits_for_fresh_pty_output() {
        let writer = PtyInputWriter::new(Box::new(io::sink()));
        writer.mark_input_pending();

        assert!(!writer.take_keystroke(false));
        assert!(writer.is_input_pending());
        assert!(writer.take_keystroke(true));
        assert!(!writer.is_input_pending());
    }

    #[test]
    fn write_input_to_writer_does_not_need_process_lock() {
        let captured = Arc::new(Mutex::new(Vec::new()));
        let writer = PtyInputWriter::new(Box::new(CapturingWriter(Arc::clone(&captured))));

        writer.write_bytes(b"abc");

        assert_eq!(*captured.lock().unwrap(), b"abc".to_vec());
        assert!(writer.is_input_pending());
    }

    #[test]
    fn pty_reader_notifies_when_output_arrives() {
        let mut fixture = ProcessFixture::new();
        let id = ProcessId::new();
        fixture
            .spawn(ProcessLaunch {
                id,
                command: "sh".into(),
                args: Vec::new(),
                cwd: String::new(),
                env: Vec::new(),
                cols: 80,
                rows: 24,
                keep_after_exit: false,
            })
            .expect("process should spawn");
        while fixture.wake_rx.try_recv().is_ok() {}
        let runtime = fixture.runtime.clone();
        fixture
            .run(async move { runtime.input(id, b"printf vmux-wake-test\r".to_vec()).await })
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if fixture.wake_rx.try_recv().is_ok() {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "timed out waiting for PTY wake notification"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn process_starts_in_requested_cwd_without_typing_cd() {
        let temp = std::env::temp_dir().join(format!("vmux-process-cwd-{}", std::process::id()));
        std::fs::create_dir_all(&temp).unwrap();
        let cwd = temp.to_string_lossy().into_owned();
        let home = temp.join("home-marker").to_string_lossy().into_owned();
        let mut fixture = ProcessFixture::new();
        let id = ProcessId::new();
        fixture
            .spawn(ProcessLaunch {
                id,
                command: "/bin/sh".into(),
                args: Vec::new(),
                cwd: cwd.clone(),
                env: vec![("HOME".into(), home.clone())],
                cols: 120,
                rows: 24,
                keep_after_exit: false,
            })
            .expect("process should spawn");

        fixture.drain_output(Duration::from_millis(300));
        let runtime = fixture.runtime.clone();
        fixture
            .run(async move {
                runtime
                    .input(id, b"printf 'HOME=%s\\n' \"$HOME\"; pwd\r".to_vec())
                    .await
            })
            .unwrap();
        let text = fixture.wait_for_snapshot_text(id, &cwd);
        fixture.shutdown();
        let _ = std::fs::remove_dir_all(&temp);

        assert!(text.contains(&cwd));
        assert!(text.contains(&format!("HOME={home}")));
        assert!(!text.contains(&format!("cd {cwd}")));
    }

    #[test]
    fn process_rejects_invalid_cwd_at_spawn() {
        let mut fixture = ProcessFixture::new();
        let cwd = std::env::temp_dir().join(format!(
            "vmux-process-missing-cwd-{}-{}",
            std::process::id(),
            ProcessId::new()
        ));
        let result = fixture.spawn(ProcessLaunch {
            id: ProcessId::new(),
            command: "/bin/sh".into(),
            args: vec!["-c".into(), "exit 0".into()],
            cwd: cwd.to_string_lossy().into_owned(),
            env: Vec::new(),
            cols: 80,
            rows: 24,
            keep_after_exit: false,
        });
        assert!(result.is_err());
        let runtime = fixture.runtime.clone();
        assert_eq!(fixture.run(async move { runtime.count().await }), Ok(0));

        let file = std::env::temp_dir().join(format!(
            "vmux-process-file-cwd-{}-{}",
            std::process::id(),
            ProcessId::new()
        ));
        std::fs::write(&file, b"not a directory").expect("write cwd file");
        let result = fixture.spawn(ProcessLaunch {
            id: ProcessId::new(),
            command: "/bin/sh".into(),
            args: vec!["-c".into(), "exit 0".into()],
            cwd: file.to_string_lossy().into_owned(),
            env: Vec::new(),
            cols: 80,
            rows: 24,
            keep_after_exit: false,
        });
        let _ = std::fs::remove_file(file);
        assert!(result.is_err());
        let runtime = fixture.runtime.clone();
        assert_eq!(fixture.run(async move { runtime.count().await }), Ok(0));
    }

    #[test]
    fn process_with_cwd_rejects_missing_executable_at_spawn() {
        let mut fixture = ProcessFixture::new();
        let cwd = std::env::temp_dir().join(format!(
            "vmux-process-command-cwd-{}-{}",
            std::process::id(),
            ProcessId::new()
        ));
        std::fs::create_dir_all(&cwd).expect("create cwd");

        let result = fixture.spawn(ProcessLaunch {
            id: ProcessId::new(),
            command: "/definitely/missing/vmux-command".into(),
            args: Vec::new(),
            cwd: cwd.to_string_lossy().into_owned(),
            env: Vec::new(),
            cols: 80,
            rows: 24,
            keep_after_exit: false,
        });
        assert!(result.is_err());
        let runtime = fixture.runtime.clone();
        assert_eq!(fixture.run(async move { runtime.count().await }), Ok(0));

        let command = cwd.join("not-executable");
        std::fs::write(&command, b"#!/bin/sh\nexit 0\n").expect("write command");
        let result = fixture.spawn(ProcessLaunch {
            id: ProcessId::new(),
            command: command.to_string_lossy().into_owned(),
            args: Vec::new(),
            cwd: cwd.to_string_lossy().into_owned(),
            env: Vec::new(),
            cols: 80,
            rows: 24,
            keep_after_exit: false,
        });
        let _ = std::fs::remove_dir_all(cwd);
        assert!(result.is_err());
        let runtime = fixture.runtime.clone();
        assert_eq!(fixture.run(async move { runtime.count().await }), Ok(0));
    }

    #[test]
    fn full_text_includes_scrolled_off_history() {
        let mut fixture = ProcessFixture::new();
        let id = ProcessId::new();
        fixture
            .spawn(ProcessLaunch {
                id,
                command: "/bin/sh".into(),
                args: Vec::new(),
                cwd: String::new(),
                env: Vec::new(),
                cols: 80,
                rows: 24,
                keep_after_exit: false,
            })
            .expect("process should spawn");

        fixture.drain_output(Duration::from_millis(300));
        let runtime = fixture.runtime.clone();
        fixture
            .run(async move {
                runtime
                    .input(
                        id,
                        b"echo FIRSTLINE; for i in $(seq 1 60); do echo pad_$i; done; echo LASTLINE\r"
                            .to_vec(),
                    )
                    .await
            })
            .unwrap();
        let _ = fixture.wait_for_snapshot_text(id, "LASTLINE");
        fixture.drain_output(Duration::from_millis(200));

        let runtime = fixture.runtime.clone();
        let visible = ProcessFixture::snapshot_text(
            fixture
                .run(async move { runtime.snapshot(id).await })
                .unwrap(),
        );
        let runtime = fixture.runtime.clone();
        let full = fixture
            .run(async move { runtime.transcript(id).await })
            .unwrap();

        assert!(
            full.contains("LASTLINE"),
            "full_text should include last line"
        );
        assert!(
            full.contains("FIRSTLINE"),
            "full_text should include scrolled-off first line; full=\n{full}"
        );
        assert!(
            !visible.contains("FIRSTLINE"),
            "visible snapshot should not include scrolled-off line; visible=\n{visible}"
        );
    }

    #[test]
    fn poll_broadcasts_command_lifecycle_from_osc133() {
        let mut fixture = ProcessFixture::new();
        let id = ProcessId::new();
        let mut rx = fixture
            .spawn(ProcessLaunch {
                id,
                command: "/bin/sh".into(),
                args: vec!["-c".into(), "printf '\\033]133;D;0\\007'".into()],
                cwd: String::new(),
                env: Vec::new(),
                cols: 80,
                rows: 24,
                keep_after_exit: true,
            })
            .expect("spawn")
            .updates;

        fixture.drain_output(Duration::from_secs(2));

        let mut saw_end = false;
        while let Ok(msg) = rx.try_recv() {
            if let ProcessUpdate::CommandLifecycle(ProcessCommandLifecycle::Ended { exit_code }) =
                msg
            {
                assert_eq!(exit_code, Some(0));
                saw_end = true;
            }
        }
        assert!(
            saw_end,
            "expected a CommandLifecycle Ended broadcast from OSC 133;D;0"
        );
        let runtime = fixture.runtime.clone();
        let status = fixture
            .run(async move { runtime.command_exit(id).await })
            .unwrap();
        assert_eq!((status.sequence, status.exit), (1, Some(0)));
    }

    #[test]
    fn copy_mode_up_at_alt_screen_top_uses_mouse_wheel_scroll() {
        let mut fixture = ProcessFixture::new();
        let (id, _, captured) = fixture.spawn_capturing(12, 8);

        let runtime = fixture.runtime.clone();
        fixture
            .run(async move {
                runtime
                    .feed_output(id, b"\x1b[?1049h\x1b[Hone\r\ntwo\r\nthree\x1b[H".to_vec())
                    .await
            })
            .unwrap();
        let runtime = fixture.runtime.clone();
        fixture
            .run(async move { runtime.enter_copy_mode(id).await })
            .unwrap();
        let runtime = fixture.runtime.clone();
        fixture
            .run(async move {
                runtime
                    .copy_mode_key(id, CopyModeKey::StartLineSelection)
                    .await
            })
            .unwrap();
        let runtime = fixture.runtime.clone();
        fixture
            .run(async move { runtime.copy_mode_key(id, CopyModeKey::Up).await })
            .unwrap();

        let entity = fixture.entity(id).unwrap();
        assert_eq!(
            fixture
                .app
                .world()
                .get::<ProcessCopyMode>(entity)
                .unwrap()
                .cursor(),
            Some((0, 1))
        );
        let runtime = fixture.runtime.clone();
        fixture
            .run(async move { runtime.copy_mode_key(id, CopyModeKey::Up).await })
            .unwrap();

        assert_eq!(*captured.lock().unwrap(), b"\x1b[<64;7;5M".to_vec());
    }

    #[test]
    fn mouse_wheel_in_mouse_mode_forwards_sgr() {
        let mut fixture = ProcessFixture::new();
        let (id, _, captured) = fixture.spawn_capturing(12, 8);
        let runtime = fixture.runtime.clone();
        fixture
            .run(async move { runtime.feed_output(id, b"\x1b[?1000h".to_vec()).await })
            .unwrap();
        let runtime = fixture.runtime.clone();
        fixture
            .run(async move { runtime.mouse_wheel(id, true, 6, 4, 0).await })
            .unwrap();
        let runtime = fixture.runtime.clone();
        fixture
            .run(async move { runtime.mouse_wheel(id, false, 0, 0, 0).await })
            .unwrap();
        assert_eq!(
            *captured.lock().unwrap(),
            b"\x1b[<64;7;5M\x1b[<65;1;1M".to_vec()
        );
    }

    #[test]
    fn mouse_wheel_in_alt_screen_sends_arrow_keys() {
        let mut fixture = ProcessFixture::new();
        let (id, _, captured) = fixture.spawn_capturing(12, 8);
        let runtime = fixture.runtime.clone();
        fixture
            .run(async move { runtime.feed_output(id, b"\x1b[?1049h".to_vec()).await })
            .unwrap();
        let runtime = fixture.runtime.clone();
        fixture
            .run(async move { runtime.mouse_wheel(id, true, 0, 0, 0).await })
            .unwrap();
        let runtime = fixture.runtime.clone();
        fixture
            .run(async move { runtime.mouse_wheel(id, false, 0, 0, 0).await })
            .unwrap();
        assert_eq!(*captured.lock().unwrap(), b"\x1b[A\x1b[B".to_vec());
    }

    #[test]
    fn mouse_wheel_in_alt_screen_app_cursor_sends_ss3_arrows() {
        let mut fixture = ProcessFixture::new();
        let (id, _, captured) = fixture.spawn_capturing(12, 8);
        let runtime = fixture.runtime.clone();
        fixture
            .run(async move {
                runtime
                    .feed_output(id, b"\x1b[?1049h\x1b[?1h".to_vec())
                    .await
            })
            .unwrap();
        let runtime = fixture.runtime.clone();
        fixture
            .run(async move { runtime.mouse_wheel(id, true, 0, 0, 0).await })
            .unwrap();
        assert_eq!(*captured.lock().unwrap(), b"\x1bOA".to_vec());
    }

    #[test]
    fn mouse_wheel_in_alt_screen_without_alternate_scroll_is_inert() {
        let mut fixture = ProcessFixture::new();
        let (id, _, captured) = fixture.spawn_capturing(12, 8);
        let runtime = fixture.runtime.clone();
        fixture
            .run(async move {
                runtime
                    .feed_output(id, b"\x1b[?1049h\x1b[?1007l".to_vec())
                    .await
            })
            .unwrap();
        let runtime = fixture.runtime.clone();
        fixture
            .run(async move { runtime.mouse_wheel(id, true, 0, 0, 0).await })
            .unwrap();
        assert!(captured.lock().unwrap().is_empty());
    }

    #[test]
    fn scroll_window_serves_document_row_window() {
        let mut fixture = ProcessFixture::new();
        let (id, mut patches, captured) = fixture.spawn_capturing(12, 4);
        let mut feed = Vec::new();
        for i in 0..40 {
            feed.extend_from_slice(format!("line{i}\r\n").as_bytes());
        }
        let runtime = fixture.runtime.clone();
        fixture
            .run(async move { runtime.feed_output(id, feed).await })
            .unwrap();
        while patches.try_recv().is_ok() {}

        let entity = fixture.entity(id).unwrap();
        let total = fixture
            .app
            .world_mut()
            .get_mut::<ProcessEmulator>(entity)
            .unwrap()
            .0
            .get()
            .dimensions()
            .2;
        assert!(
            total >= 40,
            "expected scrollback to accumulate, got {total}"
        );

        let runtime = fixture.runtime.clone();
        fixture
            .run(async move { runtime.scroll_window(id, 0, false).await })
            .unwrap();

        let offset = fixture
            .app
            .world_mut()
            .get_mut::<ProcessEmulator>(entity)
            .unwrap()
            .0
            .get()
            .display_offset();
        assert_eq!(offset, 0);
        assert!(
            captured.lock().unwrap().is_empty(),
            "native scroll must not write to the pty"
        );

        let (changed_lines, first_row, total_rows) = std::iter::from_fn(|| patches.try_recv().ok())
            .find_map(|msg| match msg {
                ProcessUpdate::Viewport(patch) => {
                    Some((patch.changed_lines, patch.first_row, patch.total_rows))
                }
                _ => None,
            })
            .expect("scroll must broadcast a viewport patch");
        assert_eq!(first_row, 0);
        assert_eq!(total_rows, total);
        assert!(changed_lines.iter().any(|(r, _)| *r == 0));
    }

    #[test]
    fn following_patch_contains_only_visible_rows() {
        let mut fixture = ProcessFixture::new();
        let (id, mut patches, _) = fixture.spawn_capturing(12, 4);
        let mut feed = Vec::new();
        for i in 0..40 {
            feed.extend_from_slice(format!("line{i}\r\n").as_bytes());
        }
        let runtime = fixture.runtime.clone();
        fixture
            .run(async move { runtime.feed_output(id, feed).await })
            .unwrap();

        let changed_lines = std::iter::from_fn(|| patches.try_recv().ok())
            .find_map(|message| match message {
                ProcessUpdate::Viewport(patch) => Some(patch.changed_lines),
                _ => None,
            })
            .expect("output must broadcast a viewport patch");
        assert!(changed_lines.len() <= 4);
    }

    #[test]
    fn terminal_mode_broadcasts_alt_screen_toggle() {
        let mut fixture = ProcessFixture::new();
        let id = ProcessId::new();
        let mut rx = fixture
            .spawn(ProcessLaunch {
                id,
                command: "/bin/sh".into(),
                args: Vec::new(),
                cwd: String::new(),
                env: Vec::new(),
                cols: 12,
                rows: 8,
                keep_after_exit: false,
            })
            .expect("process should spawn")
            .updates;

        let runtime = fixture.runtime.clone();
        fixture
            .run(async move { runtime.feed_output(id, b"\x1b[?1049h".to_vec()).await })
            .unwrap();
        let mut alt_on = None;
        while let Ok(msg) = rx.try_recv() {
            if let ProcessUpdate::Mode(mode) = msg {
                alt_on = Some(mode.alt_screen);
            }
        }
        assert_eq!(alt_on, Some(true));

        let runtime = fixture.runtime.clone();
        fixture
            .run(async move { runtime.feed_output(id, b"\x1b[?1049l".to_vec()).await })
            .unwrap();
        let mut alt_off = None;
        while let Ok(msg) = rx.try_recv() {
            if let ProcessUpdate::Mode(mode) = msg {
                alt_off = Some(mode.alt_screen);
            }
        }
        assert_eq!(alt_off, Some(false));
    }

    #[test]
    fn create_process_returns_real_pid() {
        let mut fixture = ProcessFixture::new();
        let id = ProcessId::new();
        let created = fixture
            .spawn(ProcessLaunch {
                id,
                command: "/bin/sh".into(),
                args: Vec::new(),
                cwd: String::new(),
                env: Vec::new(),
                cols: 80,
                rows: 24,
                keep_after_exit: false,
            })
            .expect("spawn");
        assert!(created.pid > 0, "expected real pid, got {}", created.pid);
        assert!(fixture.entity(created.id).is_some());
    }

    #[test]
    fn proxy_broadcasts_process_title_on_term_title_event() {
        let (tx, mut rx) = broadcast::channel::<ProcessUpdate>(8);
        let writer = PtyInputWriter::new(Box::new(io::sink()));
        let proxy = ServiceEventProxy {
            pty_writer: writer,
            update_tx: tx,
        };

        proxy.send_event(TermEvent::Title("hello-osc".into()));

        match rx.try_recv().expect("ProcessTitle should be broadcast") {
            ProcessUpdate::Title(title) => assert_eq!(title, "hello-osc"),
            other => panic!("expected ProcessTitle, got {other:?}"),
        }
    }

    #[test]
    fn proxy_broadcasts_bell_on_term_bell_event() {
        let (tx, mut rx) = broadcast::channel::<ProcessUpdate>(8);
        let writer = PtyInputWriter::new(Box::new(io::sink()));
        let proxy = ServiceEventProxy {
            pty_writer: writer,
            update_tx: tx,
        };

        proxy.send_event(TermEvent::Bell);

        match rx.try_recv().expect("Bell should be broadcast") {
            ProcessUpdate::Bell => {}
            other => panic!("expected Bell, got {other:?}"),
        }
    }

    #[test]
    fn keep_after_exit_retains_process_and_exit_code() {
        let mut fixture = ProcessFixture::new();
        let id = ProcessId::new();
        let mut rx = fixture
            .spawn(ProcessLaunch {
                id,
                command: "/bin/sh".into(),
                args: vec!["-c".into(), "exit 5".into()],
                cwd: String::new(),
                env: Vec::new(),
                cols: 80,
                rows: 24,
                keep_after_exit: true,
            })
            .expect("spawn")
            .updates;

        let deadline = Instant::now() + Duration::from_secs(2);
        let mut saw_exited = false;
        while Instant::now() < deadline {
            fixture.app.update();
            let entity = fixture.entity(id).expect("kept after exit");
            if fixture.app.world().get::<Exited>(entity).is_some() {
                saw_exited = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(saw_exited, "process should have exited");

        let runtime = fixture.runtime.clone();
        assert_eq!(
            fixture.run(async move { runtime.exit_code(id).await }),
            Ok(Some(5))
        );
        fixture.app.update();
        assert!(fixture.entity(id).is_some());

        let mut exits = 0;
        while let Ok(msg) = rx.try_recv() {
            if matches!(msg, ProcessUpdate::Exited { .. }) {
                exits += 1;
            }
        }
        assert_eq!(exits, 1);

        let runtime = fixture.runtime.clone();
        fixture
            .run(async move { runtime.remove(id).await })
            .unwrap();
        assert!(fixture.entity(id).is_none());
    }

    #[test]
    fn process_exit_drains_all_queued_pty_output() {
        let mut fixture = ProcessFixture::new();
        let id = ProcessId::new();
        fixture
            .spawn(ProcessLaunch {
                id,
                command: "/bin/sh".into(),
                args: vec![
                    "-c".into(),
                    "awk 'BEGIN { for (i = 0; i < 70000; i++) print \"abcdefgh\"; print \"TAIL-SENTINEL\" }'"
                        .into(),
                ],
                cwd: String::new(),
                env: Vec::new(),
                cols: 80,
                rows: 24,
                keep_after_exit: true,
            })
            .expect("spawn");
        std::thread::sleep(Duration::from_millis(500));

        let deadline = Instant::now() + Duration::from_secs(5);
        let mut saw_exited = false;
        while Instant::now() < deadline {
            fixture.app.update();
            let entity = fixture.entity(id).expect("process retained");
            if fixture.app.world().get::<Exited>(entity).is_some() {
                saw_exited = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(saw_exited, "process should exit");

        let runtime = fixture.runtime.clone();
        let text = fixture
            .run(async move { runtime.transcript(id).await })
            .unwrap();
        assert!(
            text.contains("TAIL-SENTINEL"),
            "exit must not discard queued PTY output"
        );
    }

    #[test]
    fn process_exit_is_reported_after_queued_pty_output_is_drained() {
        let mut fixture = ProcessFixture::new();
        let id = ProcessId::new();
        let mut rx = fixture
            .spawn(ProcessLaunch {
                id,
                command: "/bin/sh".into(),
                args: vec![
                    "-c".into(),
                    "awk 'BEGIN { for (i = 0; i < 70000; i++) print \"abcdefgh\"; print \"TAIL-SENTINEL\" }'"
                        .into(),
                ],
                cwd: String::new(),
                env: Vec::new(),
                cols: 80,
                rows: 24,
                keep_after_exit: true,
            })
            .expect("spawn")
            .updates;
        std::thread::sleep(Duration::from_millis(500));

        let deadline = Instant::now() + Duration::from_secs(5);
        let mut reported = false;
        while Instant::now() < deadline {
            fixture.app.update();
            while let Ok(message) = rx.try_recv() {
                if matches!(message, ProcessUpdate::Exited { .. }) {
                    reported = true;
                }
            }
            if reported {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(reported, "process exit should be reported");

        let runtime = fixture.runtime.clone();
        let text = fixture
            .run(async move { runtime.transcript(id).await })
            .unwrap();
        assert!(
            text.contains("TAIL-SENTINEL"),
            "exit must not be reported while queued PTY output remains"
        );
    }

    #[test]
    fn process_exit_waits_for_pty_reader_catch_up() {
        let mut fixture = ProcessFixture::with_reader_delay(Duration::from_millis(100));
        let id = ProcessId::new();
        let mut rx = fixture
            .spawn(ProcessLaunch {
                id,
                command: "/bin/sh".into(),
                args: vec!["-c".into(), "printf READER-TAIL".into()],
                cwd: String::new(),
                env: Vec::new(),
                cols: 80,
                rows: 24,
                keep_after_exit: true,
            })
            .expect("spawn")
            .updates;

        let deadline = Instant::now() + Duration::from_secs(2);
        let mut reported = false;
        while Instant::now() < deadline {
            fixture.app.update();
            while let Ok(message) = rx.try_recv() {
                if matches!(message, ProcessUpdate::Exited { .. }) {
                    reported = true;
                }
            }
            if reported {
                break;
            }
            std::thread::yield_now();
        }
        assert!(reported, "process exit should be reported");

        let runtime = fixture.runtime.clone();
        let text = fixture
            .run(async move { runtime.transcript(id).await })
            .unwrap();
        assert!(
            text.contains("READER-TAIL"),
            "exit raced with the PTY reader"
        );
    }

    #[test]
    fn process_exit_is_reported_before_background_descendant_closes_pty() {
        let mut fixture = ProcessFixture::new();
        let id = ProcessId::new();
        let mut rx = fixture
            .spawn(ProcessLaunch {
                id,
                command: "/bin/sh".into(),
                args: vec!["-c".into(), "read dummy; exit 0".into()],
                cwd: String::new(),
                env: Vec::new(),
                cols: 80,
                rows: 24,
                keep_after_exit: true,
            })
            .expect("spawn")
            .updates;
        let (pty_tx, pty_rx) = mpsc::unbounded_channel();
        let entity = fixture.entity(id).expect("process");
        fixture
            .app
            .world_mut()
            .get_mut::<ProcessPty>(entity)
            .unwrap()
            .0
            .get()
            .replace_pty_rx(pty_rx);
        let runtime = fixture.runtime.clone();
        fixture
            .run(async move { runtime.input(id, b"\n".to_vec()).await })
            .expect("signal child exit");

        let exit_deadline = Instant::now() + Duration::from_millis(500);
        while fixture
            .app
            .world()
            .get::<ProcessExit>(entity)
            .unwrap()
            .code
            .is_none()
            && Instant::now() < exit_deadline
        {
            fixture.app.update();
            assert!(
                !matches!(rx.try_recv(), Ok(ProcessUpdate::Exited { .. })),
                "child exit must wait for a quiet PTY interval"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(
            fixture
                .app
                .world()
                .get::<ProcessExit>(entity)
                .unwrap()
                .code
                .is_some(),
            "child should exit"
        );

        let deadline = Instant::now() + Duration::from_millis(500);
        let mut reported = false;
        while Instant::now() < deadline {
            fixture.app.update();
            assert!(
                fixture.app.world().get::<Exited>(entity).is_none(),
                "process must remain until PTY output is drained"
            );
            while let Ok(message) = rx.try_recv() {
                if matches!(message, ProcessUpdate::Exited { .. }) {
                    reported = true;
                }
            }
            if reported {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }

        drop(pty_tx);
        assert!(reported, "child exit must not wait for PTY closure");
    }
}
