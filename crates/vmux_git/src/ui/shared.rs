#![allow(non_snake_case)]

use std::collections::HashMap;

use dioxus::prelude::*;
use vmux_ui::components::button::{Button, ButtonVariant};
use vmux_ui::components::skeleton::Skeleton;
use vmux_ui::diff::DiffTone;
use vmux_ui::hooks::send;
use vmux_ui::i18n::{TranslationValue, translate, translate_with};
use vmux_ui::icon::{LineIcon, LineIconView};

use super::diff_projection::{DiffViewRow, EditorDiffMarker, diff_view_rows, editor_diff_markers};
use crate::event::*;

const DIFF_WINDOW_ROWS: u32 = 200_000;

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

fn diff_tone(kind: DiffKind) -> Option<DiffTone> {
    match kind {
        DiffKind::Add => Some(DiffTone::Added),
        DiffKind::Remove => Some(DiffTone::Deleted),
        DiffKind::Staged => Some(DiffTone::Staged),
        DiffKind::Context | DiffKind::Hunk => None,
    }
}

fn sign(kind: DiffKind) -> &'static str {
    diff_tone(kind).map(DiffTone::sign).unwrap_or(" ")
}

fn row_class(kind: DiffKind) -> &'static str {
    diff_tone(kind).map(DiffTone::row_class).unwrap_or("")
}

fn text_class(kind: DiffKind) -> &'static str {
    diff_tone(kind)
        .map(DiffTone::text_class)
        .unwrap_or("text-muted-foreground")
}

#[component]
pub fn GitFooter(
    git_state: ReadSignal<FileGitState>,
    leading: Element,
    always_visible: bool,
    children: Element,
) -> Element {
    let mut commit_msg = use_signal(String::new);
    let mut pending_commit_msg = use_signal(String::new);
    let mut result_sequence = use_signal(|| 0u64);
    use_effect(move || {
        let state = git_state();
        if state.result_sequence == 0 || state.result_sequence == result_sequence() {
            return;
        }
        result_sequence.set(state.result_sequence);
        let Some(result) = state.result else {
            return;
        };
        if result.action != "commit" {
            return;
        }
        if result.ok && commit_msg().trim() == pending_commit_msg() {
            commit_msg.set(String::new());
        }
        pending_commit_msg.set(String::new());
    });

    let state = git_state();
    let has_branch = !state.branch.is_empty();
    if !has_branch && !always_visible {
        return rsx! {};
    }
    let can_commit = has_branch && state.staged_count > 0;
    let can_push = has_branch && state.ahead > 0;

    rsx! {
        div {
            class: "flex h-7 min-w-0 shrink-0 items-center gap-3 overflow-hidden border-t border-white/[0.07] bg-black/20 px-4 font-sans text-xs text-muted-foreground",

            {leading}
            if has_branch {
                span {
                    class: "flex min-w-0 max-w-[35%] shrink items-center gap-1.5 text-term-fg",
                    title: "{state.branch}",
                    LineIconView { icon: LineIcon::GitBranch, class: "h-3.5 w-3.5 shrink-0 opacity-80" }
                    span { class: "truncate", "{state.branch}" }
                }
                if state.ahead > 0 || state.behind > 0 {
                    span { class: "flex shrink-0 items-center gap-2 opacity-70",
                        span { class: "flex items-center gap-0.5",
                            LineIconView { icon: LineIcon::ArrowUp, class: "h-3 w-3" }
                            "{state.ahead}"
                        }
                        span { class: "flex items-center gap-0.5",
                            LineIconView { icon: LineIcon::ArrowDown, class: "h-3 w-3" }
                            "{state.behind}"
                        }
                    }
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
                    Button {
                        variant: ButtonVariant::Ghost,
                        class: "h-auto shrink-0 px-2 py-0.5 text-xs hover:bg-white/10 disabled:opacity-40",
                        disabled: commit_msg().trim().is_empty() || !pending_commit_msg().is_empty(),
                        onclick: move |_| {
                            let m = commit_msg().trim().to_string();
                            if !m.is_empty()
                                && send(&GitCommitRequest {
                                    path: git_state().path,
                                    message: m.clone(),
                                })
                                .is_ok()
                            {
                                pending_commit_msg.set(m);
                            }
                        },
                        {translate_with(
                            "git-commit",
                            &[("count", TranslationValue::Number(state.staged_count as i64))],
                        )}
                    }
                }
                if !state.message.is_empty() {
                    span {
                        class: "min-w-0 flex-1 truncate text-ansi-1",
                        title: "{state.message}",
                        "{state.message}"
                    }
                }
            }

            if can_push {
                Button {
                    variant: ButtonVariant::Ghost,
                    class: "h-auto shrink-0 gap-1 px-2 py-0.5 text-xs hover:bg-white/10",
                    onclick: move |_| {
                        let _ = send(&GitPushRequest { path: git_state().path });
                    },
                    LineIconView { icon: LineIcon::Upload, class: "h-3 w-3" }
                    {translate("git-push")}
                }
            }
            {children}
        }
    }
}

#[component]
pub fn DiffView(
    repo_root: ReadSignal<String>,
    path: ReadSignal<String>,
    #[props(default)] path_bytes: Vec<u8>,
    #[props(default)] reference: String,
    nonce: ReadSignal<u32>,
    viewport: ReadSignal<Option<GitDiffViewportEvent>>,
    visible: bool,
    markers: Signal<HashMap<u32, EditorDiffMarker>>,
) -> Element {
    let initial_path_bytes = path_bytes.clone();
    let initial_reference = reference.clone();
    let mut observed_path_bytes = use_signal(move || initial_path_bytes);
    let mut observed_reference = use_signal(move || initial_reference);
    let mut lines = use_signal(Vec::<DiffLine>::new);
    let mut expanded = use_signal(Vec::<(usize, usize)>::new);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(String::new);
    let mut requested_path = use_signal(String::new);
    let mut request_generation = use_signal(|| 0u64);

    use_effect(move || {
        let Some(p) = viewport() else {
            return;
        };
        if p.generation != request_generation() {
            return;
        }
        markers.set(editor_diff_markers(&p.lines));
        lines.set(p.lines);
        expanded.set(Vec::new());
        loading.set(false);
        error.set(p.error);
    });

    use_effect(use_reactive!(|(path_bytes, reference)| {
        observed_path_bytes.set(path_bytes);
        observed_reference.set(reference);
    }));

    use_effect(move || {
        let root = repo_root();
        let p = path();
        let raw_path = observed_path_bytes();
        let reference = observed_reference();
        let _ = nonce();
        if !root.is_empty() && (!p.is_empty() || !reference.is_empty()) {
            let request_key = format!("{root}\0{p}\0{raw_path:?}\0{reference}");
            let path_changed = *requested_path.peek() != request_key;
            requested_path.set(request_key);
            let generation = request_generation.peek().wrapping_add(1);
            request_generation.set(generation);
            if path_changed || lines.peek().is_empty() {
                loading.set(true);
            }
            error.set(String::new());
            if path_changed {
                lines.set(Vec::new());
                expanded.set(Vec::new());
            }
            let _ = send(&GitDiffRequest {
                repo_root: root,
                path: p,
                path_bytes: raw_path,
                reference,
                generation,
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
            class: if visible { "min-h-0 flex-1 overflow-auto bg-background/35 font-mono text-xs leading-5" } else { "hidden" },

            if loading() {
                div { class: "flex flex-col gap-2 p-3",
                    for width in ["w-10/12", "w-full", "w-8/12", "w-11/12", "w-7/12"] {
                        Skeleton { class: "h-4 {width} bg-foreground/[0.045]" }
                    }
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
                                div { class: "group flex min-w-max whitespace-pre transition-colors {row_class(line.kind)}",
                                    span { class: "sticky left-0 z-[1] flex shrink-0 select-none border-r border-foreground/[0.08] bg-background/95 shadow-[4px_0_10px_-8px_rgba(0,0,0,0.8)] backdrop-blur-sm",
                                        span {
                                            class: "flex shrink-0 items-center justify-end px-2 text-right tabular-nums text-muted-foreground/45 group-hover:text-muted-foreground/75",
                                            style: "width:calc(var(--cw, 1ch) * {gw} + 1rem);",
                                            "{opt_no(line.old_no)}"
                                        }
                                        span {
                                            class: "flex shrink-0 items-center justify-end border-l border-foreground/[0.045] px-2 text-right tabular-nums text-muted-foreground/55 group-hover:text-muted-foreground/85",
                                            style: "width:calc(var(--cw, 1ch) * {gw} + 1rem);",
                                            "{opt_no(line.new_no)}"
                                        }
                                        span {
                                            class: "flex w-6 shrink-0 items-center justify-center border-l border-foreground/[0.045] font-semibold {text_class(line.kind)}",
                                            "{sign(line.kind)}"
                                        }
                                    }
                                    span { class: "min-w-0 pr-8",
                                        for (j, styled) in line.spans.iter().enumerate() {
                                            span { key: "{j}", style: "{span_style(styled)}", "{styled.text}" }
                                        }
                                    }
                                }
                                if let Some(h) = ends[i] {
                                    div {
                                        class: "flex min-w-full items-center justify-end gap-1.5 border-y border-foreground/[0.06] bg-background/70 px-3 py-1 font-sans text-[11px] select-none backdrop-blur-sm",
                                        Button {
                                            variant: ButtonVariant::Ghost,
                                            class: "h-6 gap-1 rounded-md border border-ansi-2/15 bg-ansi-2/[0.045] px-2 text-[11px] text-ansi-2 hover:bg-ansi-2/10 hover:text-ansi-2",
                                            onclick: move |_| {
                                                let _ = send(&GitHunkRequest {
                                                    repo_root: repo_root(),
                                                    path: path(),
                                                    path_bytes: observed_path_bytes(),
                                                    hunk: h,
                                                    accept: true,
                                                });
                                            },
                                            LineIconView { icon: LineIcon::Plus, class: "h-3 w-3" }
                                            {translate("git-stage-hunk")}
                                        }
                                        Button {
                                            variant: ButtonVariant::Ghost,
                                            class: "h-6 gap-1 rounded-md border border-foreground/[0.08] bg-foreground/[0.025] px-2 text-[11px] text-muted-foreground hover:bg-ansi-1/10 hover:text-ansi-1",
                                            onclick: move |_| {
                                                let _ = send(&GitHunkRequest {
                                                    repo_root: repo_root(),
                                                    path: path(),
                                                    path_bytes: observed_path_bytes(),
                                                    hunk: h,
                                                    accept: false,
                                                });
                                            },
                                            LineIconView { icon: LineIcon::RotateCcw, class: "h-3 w-3" }
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
                        let reveal = if hidden <= super::diff_projection::GAP_REVEAL_CHUNK {
                            (start, end)
                        } else if upward {
                            (end - super::diff_projection::GAP_REVEAL_CHUNK, end)
                        } else {
                            (start, start + super::diff_projection::GAP_REVEAL_CHUNK)
                        };
                        rsx! {
                            div {
                                key: "gap-{start}-{end}",
                                class: "flex min-w-full items-center border-y border-foreground/[0.055] bg-foreground/[0.018] px-3 py-1 font-sans",
                                div { class: "h-px min-w-4 flex-1 bg-foreground/[0.06]" }
                                Button {
                                    variant: ButtonVariant::Ghost,
                                    class: "mx-2 h-6 shrink-0 gap-1.5 rounded-full border border-foreground/[0.08] bg-background/70 px-3 text-[10px] text-muted-foreground hover:bg-foreground/[0.055] hover:text-foreground",
                                    title: translate_with(
                                        "git-show-unchanged-lines",
                                        &[("count", TranslationValue::Number(hidden as i64))],
                                    ),
                                    onclick: move |_| {
                                        expanded.write().push(reveal);
                                    },
                                    span { "⋯" }
                                    span { {translate_with(
                                        "git-show-unchanged-lines",
                                        &[("count", TranslationValue::Number(hidden as i64))],
                                    )} }
                                }
                                div { class: "h-px min-w-4 flex-1 bg-foreground/[0.06]" }
                            }
                        }
                    }
                }
            }
        }
    }
}
