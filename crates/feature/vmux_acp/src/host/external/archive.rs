use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
#[cfg(test)]
use std::time::Duration;
use std::time::Instant;

use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use bevy::winit::{EventLoopProxyWrapper, WinitUserEvent};
use vmux_session::{
    ArchiveState, ProviderArchiveCompleted, ProviderArchiveWriteRequest, ProviderSession,
};

use super::{Runtime, SourceKind, SourcePaths, SourceSnapshot, codex};

pub(super) struct Plugin;

impl bevy::prelude::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Queue>()
            .init_resource::<Overlay>()
            .add_systems(
                Update,
                (write, finish_write)
                    .chain()
                    .after(super::activate)
                    .before(super::watch),
            )
            .add_systems(
                Update,
                (refresh, finish_refresh)
                    .chain()
                    .after(super::watch)
                    .before(super::scan),
            );
    }
}

#[derive(Component)]
pub(super) struct RefreshRequested;

#[derive(Component)]
struct RefreshTask {
    started_at: Instant,
    task: Task<Result<Option<HashMap<String, ArchiveState>>, String>>,
}

impl RefreshTask {
    fn spawn(root: PathBuf, wake: Option<bevy::winit::EventLoopProxy<WinitUserEvent>>) -> Self {
        let started_at = Instant::now();
        let task = IoTaskPool::get().spawn(async move {
            let result = codex::Reader::at(&root).archives();
            if let Some(wake) = wake {
                let _ = wake.send_event(WinitUserEvent::WakeUp);
            }
            result
        });
        Self { started_at, task }
    }
}

#[derive(Component)]
struct WriteTask {
    request: ProviderArchiveWriteRequest,
    task: Task<Result<ArchiveState, String>>,
}

impl WriteTask {
    fn spawn(
        request: ProviderArchiveWriteRequest,
        root: PathBuf,
        wake: Option<bevy::winit::EventLoopProxy<WinitUserEvent>>,
    ) -> Self {
        let sid = request.id.0.clone();
        let archived = request.archived;
        let task = IoTaskPool::get().spawn(async move {
            let result = codex::Reader::at(&root).set_archive(&sid, archived);
            if let Some(wake) = wake {
                let _ = wake.send_event(WinitUserEvent::WakeUp);
            }
            result
        });
        Self { request, task }
    }
}

#[derive(Resource, Default)]
struct Queue(VecDeque<ProviderArchiveWriteRequest>);

#[derive(Resource, Default)]
pub(super) struct Overlay(HashMap<SourceKind, HashMap<String, OverlayEntry>>);

struct OverlayEntry {
    state: ArchiveState,
    completed_at: Instant,
}

impl Overlay {
    fn record(&mut self, source: SourceKind, id: String, state: ArchiveState) {
        self.0.entry(source).or_default().insert(
            id,
            OverlayEntry {
                state,
                completed_at: Instant::now(),
            },
        );
    }

    pub(super) fn apply(
        &mut self,
        source: SourceKind,
        scan_started_at: Instant,
        sessions: &mut [ProviderSession],
    ) {
        let Some(entries) = self.0.get_mut(&source) else {
            return;
        };
        for session in sessions {
            let Some(entry) = entries.get(&session.sid) else {
                continue;
            };
            if entry.completed_at > scan_started_at {
                session.archive = Some(entry.state);
            }
        }
        entries.retain(|_, entry| entry.completed_at > scan_started_at);
        if entries.is_empty() {
            self.0.remove(&source);
        }
    }
}

fn write(
    paths: Single<&SourcePaths>,
    mut requests: MessageReader<ProviderArchiveWriteRequest>,
    tasks: Query<(), With<WriteTask>>,
    mut queue: ResMut<Queue>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut completed: MessageWriter<ProviderArchiveCompleted>,
    mut commands: Commands,
) {
    for request in requests.read().cloned() {
        if request.kind.0 != "acp:codex" {
            completed.write(ProviderArchiveCompleted {
                kind: request.kind,
                id: request.id,
                archived: request.archived,
                revision: request.revision,
                success: false,
            });
            continue;
        }
        queue.0.push_back(request);
    }
    if !tasks.is_empty() {
        return;
    }
    let Some(request) = queue.0.pop_front() else {
        return;
    };
    commands.spawn(WriteTask::spawn(
        request,
        paths.codex.clone(),
        proxy.as_deref().map(|proxy| (**proxy).clone()),
    ));
}

fn finish_write(
    mut tasks: Query<(Entity, &mut WriteTask)>,
    mut snapshot: ResMut<SourceSnapshot>,
    mut overlay: ResMut<Overlay>,
    runtime: Single<Entity, With<Runtime>>,
    mut completed: MessageWriter<ProviderArchiveCompleted>,
    mut commands: Commands,
) {
    for (entity, mut task) in &mut tasks {
        let Some(result) = future::block_on(future::poll_once(&mut task.task)) else {
            continue;
        };
        let success = result.is_ok();
        match result {
            Ok(archive) => {
                overlay.record(SourceKind::Codex, task.request.id.0.clone(), archive);
                if let Some(session) = snapshot.0.get_mut(&SourceKind::Codex).and_then(|sessions| {
                    sessions
                        .iter_mut()
                        .find(|session| session.sid == task.request.id.0)
                }) {
                    session.archive = Some(archive);
                }
                commands.entity(*runtime).insert(RefreshRequested);
            }
            Err(error) => {
                bevy::log::warn!("failed to archive Codex session: {error}");
            }
        }
        completed.write(ProviderArchiveCompleted {
            kind: task.request.kind.clone(),
            id: task.request.id.clone(),
            archived: task.request.archived,
            revision: task.request.revision,
            success,
        });
        commands.entity(entity).despawn();
    }
}

fn refresh(
    runtime: Single<
        (Entity, &SourcePaths),
        (With<Runtime>, With<RefreshRequested>, Without<RefreshTask>),
    >,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let (entity, paths) = *runtime;
    let task = RefreshTask::spawn(
        paths.codex.clone(),
        proxy.as_deref().map(|proxy| (**proxy).clone()),
    );
    commands
        .entity(entity)
        .remove::<RefreshRequested>()
        .insert(task);
}

fn finish_refresh(
    task: Single<(Entity, &mut RefreshTask)>,
    mut snapshot: ResMut<SourceSnapshot>,
    mut overlay: ResMut<Overlay>,
    mut commands: Commands,
) {
    let (entity, mut task) = task.into_inner();
    let Some(result) = future::block_on(future::poll_once(&mut task.task)) else {
        return;
    };
    match result {
        Ok(Some(archives)) => {
            if let Some(current) = snapshot.0.get(&SourceKind::Codex) {
                let mut sessions = current.clone();
                for session in &mut sessions {
                    if let Some(archive) = archives.get(&session.sid) {
                        session.archive = Some(*archive);
                    }
                }
                overlay.apply(SourceKind::Codex, task.started_at, &mut sessions);
                if current != &sessions {
                    snapshot.0.insert(SourceKind::Codex, sessions);
                }
            }
        }
        Ok(None) => {}
        Err(error) => bevy::log::warn!("failed to refresh Codex archives: {error}"),
    }
    commands.entity(entity).remove::<RefreshTask>();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archive_overlay_keeps_a_write_newer_than_the_scan() {
        let scan_started_at = Instant::now();
        let mut overlay = Overlay::default();
        overlay.record(
            SourceKind::Codex,
            "provider-session".into(),
            ArchiveState::Archived(300),
        );
        overlay
            .0
            .get_mut(&SourceKind::Codex)
            .unwrap()
            .get_mut("provider-session")
            .unwrap()
            .completed_at = scan_started_at + Duration::from_nanos(1);
        let mut scanned = vec![ProviderSession {
            sid: "provider-session".into(),
            archive: Some(ArchiveState::Active(200)),
            ..Default::default()
        }];

        overlay.apply(SourceKind::Codex, scan_started_at, &mut scanned);

        assert_eq!(scanned[0].archive, Some(ArchiveState::Archived(300)));
    }
}
