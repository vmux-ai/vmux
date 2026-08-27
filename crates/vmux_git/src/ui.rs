#![allow(non_snake_case)]

use std::collections::HashMap;

use dioxus::prelude::*;
use vmux_ui::components::icon::Icon;
use vmux_ui::hooks::{send, use_listener};
use vmux_ui::i18n::{TranslationValue, translate, translate_with};

use crate::event::*;
use crate::view::{DiffViewRow, EditorDiffMarker, diff_view_rows, editor_diff_markers};

const DIFF_WINDOW_ROWS: u32 = 200_000;

fn status_has_diff(s: FileStatus) -> bool {
    matches!(
        s,
        FileStatus::Modified
            | FileStatus::Staged
            | FileStatus::StagedModified
            | FileStatus::Conflicted
            | FileStatus::Deleted
    )
}

fn status_label(s: FileStatus) -> String {
    match s {
        FileStatus::Clean => translate("git-status-clean"),
        FileStatus::Modified => translate("git-status-modified"),
        FileStatus::Staged => translate("git-status-staged"),
        FileStatus::StagedModified => translate("git-status-staged-modified"),
        FileStatus::Untracked => translate("git-status-untracked"),
        FileStatus::Deleted => translate("git-status-deleted"),
        FileStatus::Conflicted => translate("git-status-conflict"),
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
    has_diff: Signal<bool>,
    nonce: Signal<u32>,
    branch: Signal<String>,
    ahead: Signal<u32>,
    behind: Signal<u32>,
    staged_count: Signal<u32>,
    message: Signal<String>,
) -> Element {
    let mut file_status = use_signal(|| FileStatus::Clean);
    let mut confirming = use_signal(|| false);

    let _status = use_listener::<GitStatusEvent, _>(GIT_STATUS_EVENT, move |s| {
        message.set(String::new());
        branch.set(s.branch);
        ahead.set(s.ahead);
        behind.set(s.behind);
        staged_count.set(s.staged_count);
        has_diff.set(status_has_diff(s.file_status));
        file_status.set(s.file_status);
    });
    let _result = use_listener::<GitResultEvent, _>(GIT_RESULT_EVENT, move |r| {
        message.set(if r.ok { String::new() } else { r.message });
        nonce.set(nonce() + 1);
    });
    let _error = use_listener::<GitErrorEvent, _>(GIT_ERROR_EVENT, move |e| {
        message.set(e.message);
    });

    use_effect(move || {
        let p = path();
        let _ = nonce();
        if !p.is_empty() {
            let _ = send(&GitStatusRequest { path: p });
        }
    });

    let fs = file_status();
    if !status_has_diff(fs) {
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
            class: "flex shrink-0 items-center gap-1.5 font-sans text-[11px] text-muted-foreground",

            span { class: "shrink-0 {status_dot_class(fs)}", "\u{25cf} {status_label(fs)}" }

            if can_stage {
                button {
                    class: "shrink-0 rounded px-2 py-0.5 text-ansi-2 hover:bg-ansi-2/15",
                    onclick: move |_| {
                        let _ = send(&GitStageRequest { path: path() });
                    },
                    {translate("git-stage-file")}
                }
            }
            if can_unstage {
                button {
                    class: "shrink-0 rounded px-2 py-0.5 hover:bg-white/10",
                    onclick: move |_| {
                        let _ = send(&GitUnstageRequest { path: path() });
                    },
                    {translate("git-unstage")}
                }
            }
            if can_discard {
                if confirming() {
                    button {
                        class: "shrink-0 rounded bg-ansi-1/20 px-2 py-0.5 text-ansi-1 hover:bg-ansi-1/30",
                        onclick: move |_| {
                            let _ = send(&GitDiscardRequest { path: path() });
                            confirming.set(false);
                        },
                        {translate("git-confirm-discard")}
                    }
                    button {
                        class: "shrink-0 rounded px-2 py-0.5 hover:bg-white/10",
                        onclick: move |_| confirming.set(false),
                        {translate("common-cancel")}
                    }
                } else {
                    button {
                        class: "shrink-0 rounded px-2 py-0.5 text-ansi-1 hover:bg-ansi-1/15",
                        onclick: move |_| confirming.set(true),
                        {translate("git-discard-file")}
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
    leading: Element,
    always_visible: bool,
    children: Element,
) -> Element {
    let mut commit_msg = use_signal(String::new);

    let has_branch = !branch().is_empty();
    if !has_branch && !always_visible {
        return rsx! {};
    }
    let can_commit = has_branch && staged_count() > 0;
    let can_push = has_branch && ahead() > 0;

    rsx! {
        div {
            class: "flex h-7 min-w-0 shrink-0 items-center gap-3 overflow-hidden border-t border-white/[0.07] bg-black/20 px-4 font-sans text-xs text-muted-foreground",

            {leading}
            if has_branch {
                span {
                    class: "flex min-w-0 max-w-[35%] shrink items-center gap-1.5 text-term-fg",
                    title: "{branch}",
                    Icon { class: "h-3.5 w-3.5 shrink-0 opacity-80",
                        line { x1: "6", x2: "6", y1: "3", y2: "15" }
                        circle { cx: "18", cy: "6", r: "3" }
                        circle { cx: "6", cy: "18", r: "3" }
                        path { d: "M18 9a9 9 0 0 1-9 9" }
                    }
                    span { class: "truncate", "{branch}" }
                }
                if ahead() > 0 || behind() > 0 {
                    span { class: "shrink-0 opacity-70", "\u{2191}{ahead} \u{2193}{behind}" }
                }
            }

            div { class: "flex min-w-0 flex-1 items-center gap-3 overflow-hidden",
                if can_commit {
                    input {
                        class: "min-w-0 flex-1 rounded border border-white/15 bg-transparent px-2 py-0.5 text-term-fg outline-none placeholder:text-muted-foreground",
                        r#type: "text",
                        placeholder: translate("git-commit-message"),
                        value: "{commit_msg}",
                        oninput: move |e| commit_msg.set(e.value()),
                    }
                    button {
                        class: "shrink-0 rounded px-2 py-0.5 hover:bg-white/10 disabled:opacity-40",
                        disabled: commit_msg().is_empty(),
                        onclick: move |_| {
                            let m = commit_msg();
                            if !m.is_empty() {
                                let _ = send(&GitCommitRequest { path: path(), message: m });
                                commit_msg.set(String::new());
                            }
                        },
                        {translate_with(
                            "git-commit",
                            &[("count", TranslationValue::Number(staged_count() as i64))],
                        )}
                    }
                }
                if !message().is_empty() {
                    span {
                        class: "min-w-0 flex-1 truncate text-ansi-1",
                        title: "{message}",
                        "{message}"
                    }
                }
            }

            if can_push {
                button {
                    class: "shrink-0 rounded px-2 py-0.5 hover:bg-white/10",
                    onclick: move |_| {
                        let _ = send(&GitPushRequest { path: path() });
                    },
                    {translate("git-push")}
                }
            }
            {children}
        }
    }
}

#[component]
pub fn DiffView(
    path: ReadSignal<String>,
    nonce: ReadSignal<u32>,
    visible: bool,
    markers: Signal<HashMap<u32, EditorDiffMarker>>,
) -> Element {
    let mut lines = use_signal(Vec::<DiffLine>::new);
    let mut expanded = use_signal(Vec::<(usize, usize)>::new);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(String::new);
    let mut requested_path = use_signal(String::new);

    let _vp = use_listener::<GitDiffViewportEvent, _>(GIT_DIFF_VIEWPORT_EVENT, move |p| {
        markers.set(editor_diff_markers(&p.lines));
        lines.set(p.lines);
        expanded.set(Vec::new());
        loading.set(false);
        error.set(String::new());
    });
    let _error = use_listener::<GitErrorEvent, _>(GIT_ERROR_EVENT, move |event| {
        error.set(event.message);
        loading.set(false);
    });

    use_effect(move || {
        let p = path();
        let _ = nonce();
        if !p.is_empty() {
            let path_changed = *requested_path.peek() != p;
            requested_path.set(p.clone());
            if path_changed || lines.peek().is_empty() {
                loading.set(true);
            }
            error.set(String::new());
            if path_changed {
                lines.set(Vec::new());
                expanded.set(Vec::new());
            }
            let _ = send(&GitDiffRequest {
                path: p,
                top_line: 0,
                rows: DIFF_WINDOW_ROWS,
            });
        }
    });

    let rows = lines();
    let display_rows = diff_view_rows(&rows, &expanded());
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
            class: if visible { "min-h-0 flex-1 overflow-auto" } else { "hidden" },

            if loading() {
                div { class: "flex h-20 items-center justify-center font-sans text-xs text-muted-foreground",
                    span { class: "animate-pulse", {translate("git-loading-diff")} }
                }
            } else if !error().is_empty() {
                div { class: "p-3 font-sans text-xs text-ansi-1", "{error}" }
            } else if rows.is_empty() {
                div { class: "p-3 text-xs text-muted-foreground", {translate("git-no-changes")} }
            }

            for display_row in display_rows {
                match display_row {
                    DiffViewRow::Line(i) => {
                        let line = &rows[i];
                        rsx! {
                            div { key: "line-{i}-{line.kind:?}-{line.old_no:?}-{line.new_no:?}",
                                div { class: "flex whitespace-pre", style: "{row_bg(line.kind)}",
                                    span {
                                        class: "shrink-0 select-none border-r border-foreground/[0.06] bg-foreground/[0.025] px-1 text-right tabular-nums opacity-40",
                                        style: "width:calc(var(--cw, 1ch) * {gw});",
                                        "{opt_no(line.old_no)}"
                                    }
                                    span {
                                        class: "shrink-0 select-none border-r border-foreground/[0.06] bg-foreground/[0.025] px-1 text-right tabular-nums opacity-40",
                                        style: "width:calc(var(--cw, 1ch) * {gw});",
                                        "{opt_no(line.new_no)}"
                                    }
                                    span {
                                        class: "shrink-0 select-none px-1 text-center",
                                        style: "{sign_style(line.kind)}",
                                        "{sign(line.kind)}"
                                    }
                                    span { class: "pr-6",
                                        for (j, styled) in line.spans.iter().enumerate() {
                                            span { key: "{j}", style: "{span_style(styled)}", "{styled.text}" }
                                        }
                                    }
                                }
                                if let Some(h) = ends[i] {
                                    div {
                                        class: "flex items-center justify-end gap-2 border-y border-foreground/[0.05] bg-foreground/[0.02] px-2 py-0.5 pr-6 font-sans text-xs select-none",
                                        button {
                                            class: "rounded px-1.5 py-0.5 text-ansi-2 hover:bg-ansi-2/15",
                                            onclick: move |_| {
                                                let _ = send(&GitHunkRequest { path: path(), hunk: h, accept: true });
                                            },
                                            {translate("git-stage-hunk")}
                                        }
                                        button {
                                            class: "rounded px-1.5 py-0.5 text-ansi-1 hover:bg-ansi-1/15",
                                            onclick: move |_| {
                                                let _ = send(&GitHunkRequest { path: path(), hunk: h, accept: false });
                                            },
                                            {translate("git-revert-hunk")}
                                        }
                                    }
                                }
                            }
                        }
                    },
                    DiffViewRow::Gap { start, end } => {
                        let hidden = end - start;
                        let upward = start == 0;
                        let reveal = if hidden <= crate::view::GAP_REVEAL_CHUNK {
                            (start, end)
                        } else if upward {
                            (end - crate::view::GAP_REVEAL_CHUNK, end)
                        } else {
                            (start, start + crate::view::GAP_REVEAL_CHUNK)
                        };
                        rsx! {
                            div {
                                key: "gap-{start}-{end}",
                                class: "border-y border-cyan-400/10 bg-cyan-400/[0.035] font-sans",
                                button {
                                    class: "group flex h-7 w-full items-center gap-2 px-2 text-[11px] text-cyan-700/75 hover:bg-cyan-400/[0.08] hover:text-cyan-700 dark:text-cyan-200/70 dark:hover:text-cyan-100",
                                    title: translate_with(
                                        "git-show-unchanged-lines",
                                        &[("count", TranslationValue::Number(hidden as i64))],
                                    ),
                                    onclick: move |_| {
                                        expanded.write().push(reveal);
                                    },
                                    svg {
                                        class: if upward { "h-3.5 w-3.5 shrink-0 rotate-180 transition-transform group-hover:-translate-y-0.5" } else { "h-3.5 w-3.5 shrink-0 transition-transform group-hover:translate-y-0.5" },
                                        view_box: "0 0 24 24",
                                        fill: "none",
                                        stroke: "currentColor",
                                        stroke_width: "2",
                                        stroke_linecap: "round",
                                        stroke_linejoin: "round",
                                        path { d: "m6 9 6 6 6-6" }
                                    }
                                    span { "Show {hidden} unchanged lines" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
