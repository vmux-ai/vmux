use std::path::PathBuf;

use crate::event::*;
use crate::{parse, runner};

#[derive(Debug, Clone)]
pub enum JobKind {
    Status {
        path: PathBuf,
        dirty: bool,
    },
    Diff {
        path: PathBuf,
        top_line: u32,
        rows: u32,
        content: Option<String>,
    },
    Stage {
        path: PathBuf,
    },
    Unstage {
        path: PathBuf,
    },
    Discard {
        path: PathBuf,
    },
    Commit {
        path: PathBuf,
        message: String,
    },
    Push {
        path: PathBuf,
    },
    Hunk {
        path: PathBuf,
        hunk: u32,
        accept: bool,
    },
}

#[derive(Debug, Clone)]
pub enum Emit {
    Status(GitStatusEvent),
    DiffMeta(GitDiffMetaEvent),
    DiffViewport(GitDiffViewportEvent),
    Result(GitResultEvent),
    Error(GitErrorEvent),
}

pub fn emit_event_name(e: &Emit) -> &'static str {
    match e {
        Emit::Status(_) => GIT_STATUS_EVENT,
        Emit::DiffMeta(_) => GIT_DIFF_META_EVENT,
        Emit::DiffViewport(_) => GIT_DIFF_VIEWPORT_EVENT,
        Emit::Result(_) => GIT_RESULT_EVENT,
        Emit::Error(_) => GIT_ERROR_EVENT,
    }
}

fn result_then_status(path: &std::path::Path, action: &str, message: &str) -> Vec<Emit> {
    let result = Emit::Result(GitResultEvent {
        action: action.to_string(),
        ok: true,
        message: message.to_string(),
    });
    match runner::status(path) {
        Ok(ev) => vec![result, Emit::Status(ev)],
        Err(e) => vec![result, Emit::Error(GitErrorEvent { message: e.0 })],
    }
}

fn mutate(
    path: &std::path::Path,
    action: &str,
    op: fn(&std::path::Path) -> Result<(), runner::GitError>,
) -> Vec<Emit> {
    match op(path) {
        Ok(()) => result_then_status(path, action, "ok"),
        Err(e) => vec![Emit::Result(GitResultEvent {
            action: action.to_string(),
            ok: false,
            message: e.0,
        })],
    }
}

pub fn run_job(job: JobKind) -> Vec<Emit> {
    match job {
        JobKind::Status { path, .. } if !runner::has_repository(&path) => {
            vec![Emit::Status(runner::non_repository_status())]
        }
        JobKind::Status { path, dirty } => match runner::status(&path) {
            Ok(mut ev) => {
                if dirty {
                    ev.file_status = match ev.file_status {
                        FileStatus::Clean => FileStatus::Modified,
                        FileStatus::Staged => FileStatus::StagedModified,
                        status => status,
                    };
                }
                vec![Emit::Status(ev)]
            }
            Err(e) => vec![Emit::Error(GitErrorEvent { message: e.0 })],
        },
        JobKind::Diff { path, top_line, .. } if !runner::has_repository(&path) => vec![
            Emit::DiffMeta(GitDiffMetaEvent { total_lines: 0 }),
            Emit::DiffViewport(GitDiffViewportEvent {
                first_line: top_line,
                total_lines: 0,
                lines: Vec::new(),
            }),
        ],
        JobKind::Diff {
            path,
            top_line,
            rows,
            content,
        } => match content
            .as_deref()
            .map(|content| runner::diff_lines_with_content(&path, content))
            .unwrap_or_else(|| runner::diff_lines(&path))
        {
            Ok(lines) => {
                let (total, win) = parse::window(&lines, top_line, rows);
                vec![
                    Emit::DiffMeta(GitDiffMetaEvent { total_lines: total }),
                    Emit::DiffViewport(GitDiffViewportEvent {
                        first_line: top_line.min(total),
                        total_lines: total,
                        lines: win,
                    }),
                ]
            }
            Err(e) => vec![Emit::Error(GitErrorEvent { message: e.0 })],
        },
        JobKind::Stage { path } => mutate(&path, "stage", runner::stage),
        JobKind::Unstage { path } => mutate(&path, "unstage", runner::unstage),
        JobKind::Discard { path } => mutate(&path, "discard", runner::discard),
        JobKind::Commit { path, message } => match runner::commit(&path, &message) {
            Ok(()) => result_then_status(&path, "commit", "committed"),
            Err(e) => vec![Emit::Result(GitResultEvent {
                action: "commit".into(),
                ok: false,
                message: e.0,
            })],
        },
        JobKind::Push { path } => match runner::push(&path) {
            Ok(()) => result_then_status(&path, "push", "pushed"),
            Err(e) => vec![Emit::Result(GitResultEvent {
                action: "push".into(),
                ok: false,
                message: e.0,
            })],
        },
        JobKind::Hunk { path, hunk, accept } => match runner::apply_hunk(&path, hunk, accept) {
            Ok(()) => result_then_status(&path, if accept { "accept" } else { "reject" }, "ok"),
            Err(e) => vec![Emit::Result(GitResultEvent {
                action: "hunk".into(),
                ok: false,
                message: e.0,
            })],
        },
    }
}

#[cfg(test)]
#[path = "job.test.rs"]
mod tests;
