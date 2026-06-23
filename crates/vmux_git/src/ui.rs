#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_ui::components::icon::Icon;
use vmux_ui::hooks::{try_cef_bin_emit_rkyv, use_bin_event_listener};

use crate::event::*;

const DIFF_WINDOW_ROWS: u32 = 200_000;

fn is_dirty(s: FileStatus) -> bool {
    matches!(
        s,
        FileStatus::Modified
            | FileStatus::Staged
            | FileStatus::StagedModified
            | FileStatus::Conflicted
            | FileStatus::Deleted
    )
}

fn status_label(s: FileStatus) -> &'static str {
    match s {
        FileStatus::Clean => "clean",
        FileStatus::Modified => "modified",
        FileStatus::Staged => "staged",
        FileStatus::StagedModified => "staged*",
        FileStatus::Untracked => "untracked",
        FileStatus::Deleted => "deleted",
        FileStatus::Conflicted => "conflict",
    }
}

fn status_dot_class(s: FileStatus) -> &'static str {
    match s {
        FileStatus::Clean => "text-muted-foreground",
        FileStatus::Staged | FileStatus::StagedModified => "text-ansi-2",
        FileStatus::Conflicted => "text-ansi-1",
        _ => "text-ansi-3",
    }
}

fn repo_display_path(input: &str, repo_root: &str) -> String {
    let root = repo_root.trim_end_matches('/');
    if root.is_empty() {
        return input.trim_start_matches('/').to_string();
    }
    let name = root.rsplit('/').next().unwrap_or(root);
    let rel = if let Some(stripped) = input.strip_prefix(root) {
        stripped.trim_start_matches('/')
    } else if input.starts_with('/') {
        return input.to_string();
    } else {
        input
    };
    if rel.is_empty() {
        name.to_string()
    } else {
        format!("{name}/{rel}")
    }
}

fn span_style(span: &StyledSpan) -> String {
    let [r, g, b] = span.fg;
    let mut s = format!("color:rgb({r},{g},{b});");
    if span.bold {
        s.push_str("font-weight:700;");
    }
    if span.italic {
        s.push_str("font-style:italic;");
    }
    s
}

fn opt_no(n: Option<u32>) -> String {
    n.map(|v| v.to_string()).unwrap_or_default()
}

fn row_bg(kind: DiffKind) -> &'static str {
    match kind {
        DiffKind::Add => "background:rgba(80,200,120,0.13);",
        DiffKind::Remove => "background:rgba(220,80,80,0.13);",
        DiffKind::Staged => "background:rgba(80,200,120,0.05);",
        _ => "",
    }
}

fn sign(kind: DiffKind) -> &'static str {
    match kind {
        DiffKind::Add => "+",
        DiffKind::Remove => "-",
        DiffKind::Staged => "\u{258e}",
        _ => " ",
    }
}

fn sign_style(kind: DiffKind) -> &'static str {
    match kind {
        DiffKind::Add => "color:rgb(80,200,120);",
        DiffKind::Remove => "color:rgb(220,80,80);",
        DiffKind::Staged => "color:rgb(80,200,120);",
        _ => "opacity:0.25;",
    }
}

#[component]
pub fn GitBar(
    path: ReadSignal<String>,
    show_diff: Signal<bool>,
    nonce: Signal<u32>,
    display_path: Signal<String>,
    branch: Signal<String>,
    ahead: Signal<u32>,
    behind: Signal<u32>,
    staged_count: Signal<u32>,
    message: Signal<String>,
) -> Element {
    let mut file_status = use_signal(|| FileStatus::Clean);
    let mut confirming = use_signal(|| false);

    let _status = use_bin_event_listener::<GitStatusEvent, _>(GIT_STATUS_EVENT, move |s| {
        branch.set(s.branch);
        ahead.set(s.ahead);
        behind.set(s.behind);
        staged_count.set(s.staged_count);
        show_diff.set(is_dirty(s.file_status));
        file_status.set(s.file_status);
        display_path.set(repo_display_path(&path(), &s.repo_root));
    });
    let _result = use_bin_event_listener::<GitResultEvent, _>(GIT_RESULT_EVENT, move |r| {
        message.set(if r.ok { String::new() } else { r.message });
        nonce.set(nonce() + 1);
    });
    let _error = use_bin_event_listener::<GitErrorEvent, _>(GIT_ERROR_EVENT, move |e| {
        message.set(e.message);
    });

    use_effect(move || {
        let p = path();
        let _ = nonce();
        if !p.is_empty() {
            let _ = try_cef_bin_emit_rkyv(&GitStatusRequest { path: p });
        }
    });

    let fs = file_status();
    if fs == FileStatus::Clean {
        return rsx! {};
    }
    let can_stage = matches!(
        fs,
        FileStatus::Modified
            | FileStatus::Untracked
            | FileStatus::StagedModified
            | FileStatus::Deleted
    );
    let can_unstage = matches!(fs, FileStatus::Staged | FileStatus::StagedModified);
    let can_discard = matches!(
        fs,
        FileStatus::Modified | FileStatus::StagedModified | FileStatus::Deleted
    );

    rsx! {
        div {
            class: "flex h-8 shrink-0 items-center gap-2 border-b border-white/[0.07] bg-black/10 px-4 font-sans text-xs text-muted-foreground",

            span { class: "shrink-0 {status_dot_class(fs)}", "\u{25cf} {status_label(fs)}" }

            if can_stage {
                button {
                    class: "shrink-0 rounded px-2 py-0.5 text-ansi-2 hover:bg-ansi-2/15",
                    onclick: move |_| {
                        let _ = try_cef_bin_emit_rkyv(&GitStageRequest { path: path() });
                    },
                    "\u{2713} accept all"
                }
            }
            if can_unstage {
                button {
                    class: "shrink-0 rounded px-2 py-0.5 hover:bg-white/10",
                    onclick: move |_| {
                        let _ = try_cef_bin_emit_rkyv(&GitUnstageRequest { path: path() });
                    },
                    "Unstage"
                }
            }
            if can_discard {
                if confirming() {
                    button {
                        class: "shrink-0 rounded bg-ansi-1/20 px-2 py-0.5 text-ansi-1 hover:bg-ansi-1/30",
                        onclick: move |_| {
                            let _ = try_cef_bin_emit_rkyv(&GitDiscardRequest { path: path() });
                            confirming.set(false);
                        },
                        "Confirm deny all"
                    }
                    button {
                        class: "shrink-0 rounded px-2 py-0.5 hover:bg-white/10",
                        onclick: move |_| confirming.set(false),
                        "Cancel"
                    }
                } else {
                    button {
                        class: "shrink-0 rounded px-2 py-0.5 text-ansi-1 hover:bg-ansi-1/15",
                        onclick: move |_| confirming.set(true),
                        "\u{2717} deny all"
                    }
                }
            }
        }
    }
}

#[component]
pub fn GitFooter(
    path: ReadSignal<String>,
    branch: ReadSignal<String>,
    ahead: ReadSignal<u32>,
    behind: ReadSignal<u32>,
    staged_count: ReadSignal<u32>,
    message: ReadSignal<String>,
) -> Element {
    let mut commit_msg = use_signal(String::new);

    if branch().is_empty() {
        return rsx! {};
    }
    let can_commit = staged_count() > 0;
    let can_push = ahead() > 0;

    rsx! {
        div {
            class: "flex h-7 shrink-0 items-center gap-3 border-t border-white/[0.07] bg-black/20 px-4 font-sans text-xs text-muted-foreground",

            span { class: "flex shrink-0 items-center gap-1.5 text-term-fg",
                Icon { class: "h-3.5 w-3.5 shrink-0 opacity-80",
                    line { x1: "6", x2: "6", y1: "3", y2: "15" }
                    circle { cx: "18", cy: "6", r: "3" }
                    circle { cx: "6", cy: "18", r: "3" }
                    path { d: "M18 9a9 9 0 0 1-9 9" }
                }
                span { "{branch}" }
            }
            if ahead() > 0 || behind() > 0 {
                span { class: "shrink-0 opacity-70", "\u{2191}{ahead} \u{2193}{behind}" }
            }

            if can_commit {
                input {
                    class: "min-w-0 flex-1 rounded border border-white/15 bg-transparent px-2 py-0.5 text-term-fg outline-none placeholder:text-muted-foreground",
                    r#type: "text",
                    placeholder: "commit message",
                    value: "{commit_msg}",
                    oninput: move |e| commit_msg.set(e.value()),
                }
                button {
                    class: "shrink-0 rounded px-2 py-0.5 hover:bg-white/10 disabled:opacity-40",
                    disabled: commit_msg().is_empty(),
                    onclick: move |_| {
                        let m = commit_msg();
                        if !m.is_empty() {
                            let _ = try_cef_bin_emit_rkyv(&GitCommitRequest { path: path(), message: m });
                            commit_msg.set(String::new());
                        }
                    },
                    "Commit ({staged_count})"
                }
            } else {
                span { class: "flex-1" }
            }

            if can_push {
                button {
                    class: "shrink-0 rounded px-2 py-0.5 hover:bg-white/10",
                    onclick: move |_| {
                        let _ = try_cef_bin_emit_rkyv(&GitPushRequest { path: path() });
                    },
                    "\u{2191} Push"
                }
            }
            if !message().is_empty() {
                span { class: "shrink-0 truncate text-ansi-1", "{message}" }
            }
        }
    }
}

#[component]
pub fn DiffView(path: ReadSignal<String>, nonce: ReadSignal<u32>) -> Element {
    let mut lines = use_signal(Vec::<DiffLine>::new);

    let _vp =
        use_bin_event_listener::<GitDiffViewportEvent, _>(GIT_DIFF_VIEWPORT_EVENT, move |p| {
            lines.set(p.lines);
        });

    use_effect(move || {
        let p = path();
        let _ = nonce();
        if !p.is_empty() {
            let _ = try_cef_bin_emit_rkyv(&GitDiffRequest {
                path: p,
                top_line: 0,
                rows: DIFF_WINDOW_ROWS,
            });
        }
    });

    let rows = lines();
    let maxno = rows
        .iter()
        .flat_map(|l| [l.old_no, l.new_no])
        .flatten()
        .max()
        .unwrap_or(0);
    let gw = maxno.max(1).to_string().len().max(3);
    let ends: Vec<Option<u32>> = rows
        .iter()
        .enumerate()
        .map(|(i, l)| match l.hunk {
            Some(h) if i + 1 == rows.len() || rows[i + 1].hunk != Some(h) => Some(h),
            _ => None,
        })
        .collect();

    rsx! {
        div {
            class: "min-h-0 flex-1 overflow-auto",

            if rows.is_empty() {
                div { class: "p-3 text-xs text-muted-foreground", "No changes to show" }
            }

            for (i, line) in rows.iter().enumerate() {
                div { key: "{line.kind:?}-{line.old_no:?}-{line.new_no:?}",
                div { class: "flex whitespace-pre", style: "{row_bg(line.kind)}",
                    span {
                        class: "shrink-0 select-none px-1 text-right tabular-nums opacity-30",
                        style: "width:calc(var(--cw, 1ch) * {gw});",
                        "{opt_no(line.old_no)}"
                    }
                    span {
                        class: "shrink-0 select-none px-1 text-right tabular-nums opacity-30",
                        style: "width:calc(var(--cw, 1ch) * {gw});",
                        "{opt_no(line.new_no)}"
                    }
                    span {
                        class: "shrink-0 select-none px-1 text-center",
                        style: "{sign_style(line.kind)}",
                        "{sign(line.kind)}"
                    }
                    span { class: "pr-6",
                        for (j, s) in line.spans.iter().enumerate() {
                            span { key: "{j}", style: "{span_style(s)}", "{s.text}" }
                        }
                    }
                }
                if let Some(h) = ends[i] {
                    div {
                        class: "flex items-center justify-end gap-2 px-2 pr-6 py-0.5 font-sans text-xs select-none",
                        button {
                            class: "rounded px-1.5 py-0.5 text-ansi-2 hover:bg-ansi-2/15",
                            onclick: move |_| {
                                let _ = try_cef_bin_emit_rkyv(&GitHunkRequest { path: path(), hunk: h, accept: true });
                            },
                            "\u{2713} accept"
                        }
                        button {
                            class: "rounded px-1.5 py-0.5 text-ansi-1 hover:bg-ansi-1/15",
                            onclick: move |_| {
                                let _ = try_cef_bin_emit_rkyv(&GitHunkRequest { path: path(), hunk: h, accept: false });
                            },
                            "\u{2717} deny"
                        }
                    }
                }
                }
            }
        }
    }
}
