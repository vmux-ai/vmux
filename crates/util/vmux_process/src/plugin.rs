use bevy::ecs::system::SystemParam;
use bevy::platform::cell::SyncCell;
use bevy::prelude::*;
use tokio::sync::broadcast;
use vmux_api::ProcessId;
use vmux_api::protocol::{AgentCommandExit, AgentRunCompletion, ProcessInfo};

use crate::runtime_driver::{self as runtime, ProcessCreated, ProcessWake, RuntimeInbox};
use crate::{Process, ProcessUpdate, PtyInputWriter};

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
                )
                    .chain(),
                remove,
                shutdown,
                ApplyDeferred,
                poll,
            )
                .chain(),
        );
    }
}

#[derive(Component)]
struct ProcessDriver(SyncCell<Process>);

#[derive(Component, Clone)]
struct ProcessShell(String);

#[derive(Component, Clone)]
struct ProcessDirectory(String);

#[derive(Component, Clone, Copy)]
struct ProcessSize {
    cols: u16,
    rows: u16,
}

#[derive(Component, Clone, Copy)]
struct ProcessPid(u32);

#[derive(Component, Clone, Copy)]
struct ProcessStartedAt(std::time::Instant);

#[derive(Component, Clone)]
struct ProcessUpdates(broadcast::Sender<ProcessUpdate>);

#[derive(Component, Clone)]
struct ProcessInput(PtyInputWriter);

#[derive(Component, Clone, Copy)]
struct ProcessExit(Option<i32>);

#[derive(Component)]
struct KeepAfterExit;

#[derive(Component)]
struct Running;

#[derive(Component)]
struct Exited;

#[derive(SystemParam)]
struct Processes<'w, 's> {
    values: Query<'w, 's, (Entity, &'static ProcessId, &'static mut ProcessDriver)>,
}

impl Processes<'_, '_> {
    fn contains(&mut self, process_id: ProcessId) -> bool {
        self.values
            .iter_mut()
            .any(|(_, process, _)| *process == process_id)
    }

    fn entity(&mut self, process_id: ProcessId) -> Option<Entity> {
        self.values
            .iter_mut()
            .find(|(_, process, _)| **process == process_id)
            .map(|(entity, _, _)| entity)
    }

    fn read<T>(
        &mut self,
        process_id: ProcessId,
        read: impl FnOnce(&Process) -> T,
    ) -> Result<T, String> {
        let (_, _, mut driver) = self
            .values
            .iter_mut()
            .find(|(_, process, _)| **process == process_id)
            .ok_or_else(|| format!("process not found: {process_id}"))?;
        Ok(read(driver.0.get()))
    }

    fn mutate<T>(
        &mut self,
        process_id: ProcessId,
        mutate: impl FnOnce(&mut Process) -> T,
    ) -> Result<T, String> {
        let (_, _, mut driver) = self
            .values
            .iter_mut()
            .find(|(_, process, _)| **process == process_id)
            .ok_or_else(|| format!("process not found: {process_id}"))?;
        Ok(mutate(driver.0.get()))
    }
}

fn create(
    mut requests: Single<&mut RuntimeInbox<runtime::CreateRequest>>,
    wake: Single<&ProcessWake>,
    mut processes: Processes,
    mut commands: Commands,
) {
    while let Ok(request) = requests.0.try_recv() {
        let runtime::CreateRequest { launch, reply } = request;
        let result = if processes.contains(launch.id) {
            Err(format!("process already exists: {}", launch.id))
        } else {
            Process::new_with_wake(
                launch.id,
                launch.command,
                launch.args,
                launch.cwd,
                launch.env,
                launch.cols,
                launch.rows,
                wake.0.clone(),
            )
            .map(|mut process| {
                if launch.keep_after_exit {
                    process.set_keep_after_exit();
                }
                let updates = process.updates();
                let input = process.input_writer();
                let created = ProcessCreated {
                    id: process.id,
                    pid: process.pid,
                    updates: updates.subscribe(),
                };
                let mut entity = commands.spawn((
                    Name::new(format!("Process {}", process.id)),
                    process.id,
                    ProcessShell(process.shell.clone()),
                    ProcessDirectory(process.cwd.clone()),
                    ProcessSize {
                        cols: process.cols,
                        rows: process.rows,
                    },
                    ProcessPid(process.pid),
                    ProcessStartedAt(process.created_at),
                    ProcessUpdates(updates),
                    ProcessInput(input),
                    ProcessExit(process.process_exit()),
                    Running,
                    ProcessDriver(SyncCell::new(process)),
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
    processes: Query<(&ProcessId, &ProcessUpdates)>,
) {
    while let Ok(request) = requests.0.try_recv() {
        let result = processes
            .iter()
            .find(|(process, _)| **process == request.process_id)
            .map(|(_, updates)| updates.0.subscribe())
            .ok_or_else(|| format!("process not found: {}", request.process_id));
        let _ = request.reply.send(result);
    }
}

fn input(
    mut requests: Single<&mut RuntimeInbox<runtime::InputRequest>>,
    mut processes: Processes,
    inputs: Query<(&ProcessId, &ProcessInput)>,
) {
    while let Ok(request) = requests.0.try_recv() {
        let result = processes
            .read(request.process_id, Process::is_copy_mode)
            .and_then(|copy_mode| {
                if copy_mode {
                    return Ok(());
                }
                let input = inputs
                    .iter()
                    .find(|(process, _)| **process == request.process_id)
                    .map(|(_, input)| input)
                    .ok_or_else(|| format!("process not found: {}", request.process_id))?;
                Process::write_input_to_writer(&input.0, &request.data);
                Ok(())
            });
        let _ = request.reply.send(result);
    }
}

fn mouse_wheel(
    mut requests: Single<&mut RuntimeInbox<runtime::MouseWheelRequest>>,
    mut processes: Processes,
) {
    while let Ok(request) = requests.0.try_recv() {
        let result = processes.mutate(request.process_id, |process| {
            process.handle_mouse_wheel(request.up, request.col, request.row, request.modifiers)
        });
        let _ = request.reply.send(result);
    }
}

fn scroll_window(
    mut requests: Single<&mut RuntimeInbox<runtime::ScrollWindowRequest>>,
    mut processes: Processes,
) {
    while let Ok(request) = requests.0.try_recv() {
        let result = processes.mutate(request.process_id, |process| {
            process.handle_scroll_window(request.top_row, request.follow)
        });
        let _ = request.reply.send(result);
    }
}

fn resize(
    mut requests: Single<&mut RuntimeInbox<runtime::ResizeRequest>>,
    mut processes: Processes,
    mut sizes: Query<&mut ProcessSize>,
) {
    while let Ok(request) = requests.0.try_recv() {
        let entity = processes.entity(request.process_id);
        let result = processes.mutate(request.process_id, |process| {
            process.resize(request.cols, request.rows)
        });
        if result.is_ok()
            && let Some(entity) = entity
            && let Ok(mut size) = sizes.get_mut(entity)
        {
            size.cols = request.cols;
            size.rows = request.rows;
        }
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
    mut processes: Processes,
    mut commands: Commands,
) {
    while let Ok(request) = requests.0.try_recv() {
        if let Some(entity) = processes.entity(request.process_id) {
            let _ = processes.mutate(request.process_id, Process::kill);
            commands.entity(entity).despawn();
        }
        let _ = request.reply.send(());
    }
}

fn kill(mut requests: Single<&mut RuntimeInbox<runtime::KillRequest>>, mut processes: Processes) {
    while let Ok(request) = requests.0.try_recv() {
        let result = processes.mutate(request.process_id, Process::kill);
        let _ = request.reply.send(result);
    }
}

fn snapshot(
    mut requests: Single<&mut RuntimeInbox<runtime::SnapshotRequest>>,
    mut processes: Processes,
) {
    while let Ok(request) = requests.0.try_recv() {
        let result = processes.read(request.process_id, Process::snapshot);
        let _ = request.reply.send(result);
    }
}

fn set_selection(
    mut requests: Single<&mut RuntimeInbox<runtime::SetSelectionRequest>>,
    mut processes: Processes,
) {
    while let Ok(request) = requests.0.try_recv() {
        let result = processes.mutate(request.process_id, |process| {
            process.set_selection(request.range)
        });
        let _ = request.reply.send(result);
    }
}

fn extend_selection(
    mut requests: Single<&mut RuntimeInbox<runtime::ExtendSelectionRequest>>,
    mut processes: Processes,
) {
    while let Ok(request) = requests.0.try_recv() {
        let result = processes.mutate(request.process_id, |process| {
            process.extend_selection_to(request.col, request.row)
        });
        let _ = request.reply.send(result);
    }
}

fn select_word(
    mut requests: Single<&mut RuntimeInbox<runtime::SelectWordRequest>>,
    mut processes: Processes,
) {
    while let Ok(request) = requests.0.try_recv() {
        let result = processes.mutate(request.process_id, |process| {
            process.select_word_at(request.col, request.row)
        });
        let _ = request.reply.send(result);
    }
}

fn select_line(
    mut requests: Single<&mut RuntimeInbox<runtime::SelectLineRequest>>,
    mut processes: Processes,
) {
    while let Ok(request) = requests.0.try_recv() {
        let result = processes.mutate(request.process_id, |process| {
            process.select_line_at(request.row)
        });
        let _ = request.reply.send(result);
    }
}

fn selection_text(
    mut requests: Single<&mut RuntimeInbox<runtime::SelectionTextRequest>>,
    mut processes: Processes,
) {
    while let Ok(request) = requests.0.try_recv() {
        let result = processes.read(request.process_id, |process| {
            process.selection_text().unwrap_or_default()
        });
        let _ = request.reply.send(result);
    }
}

fn enter_copy_mode(
    mut requests: Single<&mut RuntimeInbox<runtime::EnterCopyModeRequest>>,
    mut processes: Processes,
) {
    while let Ok(request) = requests.0.try_recv() {
        let result = processes.mutate(request.process_id, Process::enter_copy_mode);
        let _ = request.reply.send(result);
    }
}

fn exit_copy_mode(
    mut requests: Single<&mut RuntimeInbox<runtime::ExitCopyModeRequest>>,
    mut processes: Processes,
) {
    while let Ok(request) = requests.0.try_recv() {
        let result = processes.mutate(request.process_id, Process::exit_copy_mode);
        let _ = request.reply.send(result);
    }
}

fn copy_mode_key(
    mut requests: Single<&mut RuntimeInbox<runtime::CopyModeRequest>>,
    mut processes: Processes,
) {
    while let Ok(request) = requests.0.try_recv() {
        let result = processes.mutate(request.process_id, |process| {
            process.copy_mode_key(request.key)
        });
        let _ = request.reply.send(result);
    }
}

fn shutdown(
    mut requests: Single<&mut RuntimeInbox<runtime::ShutdownRequest>>,
    mut processes: Processes,
    mut commands: Commands,
) {
    while let Ok(request) = requests.0.try_recv() {
        for (entity, _, mut driver) in &mut processes.values {
            driver.0.get().kill();
            commands.entity(entity).despawn();
        }
        let _ = request.reply.send(());
    }
}

fn count(
    mut requests: Single<&mut RuntimeInbox<runtime::CountRequest>>,
    processes: Query<&ProcessId, With<ProcessDriver>>,
) {
    while let Ok(request) = requests.0.try_recv() {
        let _ = request.reply.send(processes.iter().count() as u32);
    }
}

fn output(
    mut requests: Single<&mut RuntimeInbox<runtime::OutputRequest>>,
    mut processes: Processes,
) {
    while let Ok(request) = requests.0.try_recv() {
        let result = processes.read(request.process_id, Process::visible_text);
        let _ = request.reply.send(result);
    }
}

fn transcript(
    mut requests: Single<&mut RuntimeInbox<runtime::TranscriptRequest>>,
    mut processes: Processes,
) {
    while let Ok(request) = requests.0.try_recv() {
        let result = processes.read(request.process_id, Process::full_text);
        let _ = request.reply.send(result);
    }
}

fn command_exit(
    mut requests: Single<&mut RuntimeInbox<runtime::CommandExitRequest>>,
    mut processes: Processes,
) {
    while let Ok(request) = requests.0.try_recv() {
        let result = processes.read(request.process_id, |process| {
            let (sequence, exit) = process.command_status();
            AgentCommandExit { sequence, exit }
        });
        let _ = request.reply.send(result);
    }
}

fn run_completion(
    mut requests: Single<&mut RuntimeInbox<runtime::RunCompletionRequest>>,
    mut processes: Processes,
) {
    while let Ok(request) = requests.0.try_recv() {
        let result = processes.read(request.process_id, |process| {
            let (token, exit) = match process.run_completion() {
                Some((token, exit)) => (Some(token), Some(exit)),
                None => (None, None),
            };
            AgentRunCompletion { token, exit }
        });
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
            .find(|(process, _)| **process == request.process_id)
            .map(|(_, exit)| exit.0)
            .ok_or_else(|| format!("process not found: {}", request.process_id));
        let _ = request.reply.send(result);
    }
}

fn poll(
    mut processes: Query<(
        Entity,
        &mut ProcessDriver,
        &mut ProcessExit,
        Option<&KeepAfterExit>,
    )>,
    mut commands: Commands,
) {
    for (entity, mut driver, mut exit, keep_after_exit) in &mut processes {
        let process = driver.0.get();
        let exited = process.poll();
        exit.0 = process.process_exit();
        if !exited {
            continue;
        }
        if keep_after_exit.is_some() {
            commands.entity(entity).remove::<Running>().insert(Exited);
            continue;
        }
        process.kill();
        commands.entity(entity).despawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::{App, MinimalPlugins, Name};
    use tokio::sync::mpsc;

    use crate::ProcessRuntime;

    #[tokio::test]
    async fn missing_process_query_is_answered_by_ecs() {
        let (wake, _wake_rx) = mpsc::unbounded_channel();
        let (processes, runtime) = ProcessRuntime::new(wake);
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, ProcessPlugin));
        app.world_mut()
            .spawn((Name::new("vmux process runtime"), runtime));
        let process_id = ProcessId::new();
        let query = tokio::spawn(async move { processes.output(process_id).await });
        for _ in 0..4 {
            tokio::task::yield_now().await;
            app.update();
            if query.is_finished() {
                break;
            }
        }

        assert_eq!(
            query.await.unwrap(),
            Err(format!("process not found: {process_id}"))
        );
    }
}
