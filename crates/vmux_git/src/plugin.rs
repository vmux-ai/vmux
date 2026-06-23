use std::sync::{Arc, Mutex};

use bevy_cef::prelude::{BinEventEmitterPlugin, BinHostEmitEvent, BinReceive};

use crate::event::{
    GitCommitRequest, GitDiffRequest, GitDiscardRequest, GitHunkRequest, GitPushRequest,
    GitStageRequest, GitStatusRequest, GitUnstageRequest,
};
use crate::job::{emit_event_name, run_job, Emit, JobKind};

pub type OutboxQueue = Vec<(Entity, Vec<Emit>)>;

#[derive(Resource, Clone, Default)]
pub struct GitOutbox(pub Arc<Mutex<OutboxQueue>>);

pub struct GitPlugin;

impl Plugin for GitPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GitOutbox>()
            .add_plugins(BinEventEmitterPlugin::<(
                GitStatusRequest,
                GitDiffRequest,
                GitStageRequest,
                GitUnstageRequest,
                GitDiscardRequest,
                GitCommitRequest,
                GitPushRequest,
                GitHunkRequest,
            )>::default())
            .add_observer(on_status_request)
            .add_observer(on_diff_request)
            .add_observer(on_stage_request)
            .add_observer(on_unstage_request)
            .add_observer(on_discard_request)
            .add_observer(on_commit_request)
            .add_observer(on_push_request)
            .add_observer(on_hunk_request)
            .add_systems(Update, drain_git_outbox);
    }
}

fn spawn_job(outbox: &GitOutbox, webview: Entity, job: JobKind) {
    let sink = outbox.0.clone();
    std::thread::spawn(move || {
        let emits = run_job(job);
        sink.lock()
            .unwrap_or_else(|p| p.into_inner())
            .push((webview, emits));
    });
}

fn on_status_request(trigger: On<BinReceive<GitStatusRequest>>, outbox: Res<GitOutbox>) {
    spawn_job(&outbox, trigger.event().webview, JobKind::Status {
        path: trigger.event().payload.path.clone().into(),
    });
}

fn on_diff_request(trigger: On<BinReceive<GitDiffRequest>>, outbox: Res<GitOutbox>) {
    let p = &trigger.event().payload;
    spawn_job(&outbox, trigger.event().webview, JobKind::Diff {
        path: p.path.clone().into(),
        top_line: p.top_line,
        rows: p.rows,
    });
}

fn on_stage_request(trigger: On<BinReceive<GitStageRequest>>, outbox: Res<GitOutbox>) {
    spawn_job(&outbox, trigger.event().webview, JobKind::Stage {
        path: trigger.event().payload.path.clone().into(),
    });
}

fn on_unstage_request(trigger: On<BinReceive<GitUnstageRequest>>, outbox: Res<GitOutbox>) {
    spawn_job(&outbox, trigger.event().webview, JobKind::Unstage {
        path: trigger.event().payload.path.clone().into(),
    });
}

fn on_discard_request(trigger: On<BinReceive<GitDiscardRequest>>, outbox: Res<GitOutbox>) {
    spawn_job(&outbox, trigger.event().webview, JobKind::Discard {
        path: trigger.event().payload.path.clone().into(),
    });
}

fn on_commit_request(trigger: On<BinReceive<GitCommitRequest>>, outbox: Res<GitOutbox>) {
    let p = &trigger.event().payload;
    spawn_job(&outbox, trigger.event().webview, JobKind::Commit {
        path: p.path.clone().into(),
        message: p.message.clone(),
    });
}

fn on_push_request(trigger: On<BinReceive<GitPushRequest>>, outbox: Res<GitOutbox>) {
    spawn_job(&outbox, trigger.event().webview, JobKind::Push {
        path: trigger.event().payload.path.clone().into(),
    });
}

fn on_hunk_request(trigger: On<BinReceive<GitHunkRequest>>, outbox: Res<GitOutbox>) {
    let p = &trigger.event().payload;
    spawn_job(&outbox, trigger.event().webview, JobKind::Hunk {
        path: p.path.clone().into(),
        hunk: p.hunk,
        accept: p.accept,
    });
}

fn drain_git_outbox(outbox: Res<GitOutbox>, mut commands: Commands) {
    let drained: OutboxQueue = {
        let mut q = outbox.0.lock().unwrap_or_else(|p| p.into_inner());
        q.drain(..).collect()
    };
    for (webview, emits) in drained {
        for emit in emits {
            let name = emit_event_name(&emit);
            match emit {
                Emit::Status(ev) => commands.trigger(BinHostEmitEvent::from_rkyv(webview, name, &ev)),
                Emit::DiffMeta(ev) => commands.trigger(BinHostEmitEvent::from_rkyv(webview, name, &ev)),
                Emit::DiffViewport(ev) => commands.trigger(BinHostEmitEvent::from_rkyv(webview, name, &ev)),
                Emit::Result(ev) => commands.trigger(BinHostEmitEvent::from_rkyv(webview, name, &ev)),
                Emit::Error(ev) => commands.trigger(BinHostEmitEvent::from_rkyv(webview, name, &ev)),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::GitErrorEvent;

    #[test]
    fn drain_empties_outbox() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<GitOutbox>()
            .add_systems(Update, drain_git_outbox);

        let webview = app.world_mut().spawn_empty().id();
        app.world().resource::<GitOutbox>().0.lock().unwrap().push((
            webview,
            vec![Emit::Error(GitErrorEvent { message: "boom".into() })],
        ));

        app.update();

        assert!(app.world().resource::<GitOutbox>().0.lock().unwrap().is_empty());
    }
}
