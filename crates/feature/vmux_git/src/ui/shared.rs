#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_ui::components::button::{Button, ButtonVariant};
use vmux_ui::components::skeleton::Skeleton;
use vmux_ui::diff::DiffTone;
use vmux_ui::hooks::send;
use vmux_ui::i18n::{TranslationValue, translate, translate_with};
use vmux_ui::icon::{LineIcon, LineIconView};

use crate::event::GitDiffRow;
use crate::event::*;

const LIST_PAGE_SIZE: usize = 40;

#[derive(Clone, Copy)]
pub(super) struct ListPager(Signal<usize>);

impl ListPager {
    pub(super) fn use_state() -> Self {
        Self(use_signal(|| LIST_PAGE_SIZE))
    }

    pub(super) fn visible(self, total: usize) -> usize {
        self.0().min(total)
    }

    pub(super) fn load_more(self, event: Event<ScrollData>, total: usize) {
        let current = self.0();
        if current >= total {
            return;
        }
        let remaining = event.scroll_height() - event.scroll_top() as i32 - event.client_height();
        if remaining > 96 {
            return;
        }
        let mut visible = self.0;
        visible.set(current.saturating_add(LIST_PAGE_SIZE).min(total));
    }
}

#[component]
pub(super) fn ProjectKindPill(is_worktree: bool) -> Element {
    let label = match is_worktree {
        true => translate("layout-worktree"),
        false => translate("git-repository"),
    };
    rsx! {
        span { class: "shrink-0 rounded-full bg-foreground/[0.055] px-1.5 py-0.5 text-[8px] font-medium text-muted-foreground", "{label}" }
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

struct DiffPresentation(Option<DiffTone>);

impl From<DiffKind> for DiffPresentation {
    fn from(kind: DiffKind) -> Self {
        Self(match kind {
            DiffKind::Add => Some(DiffTone::Added),
            DiffKind::Remove => Some(DiffTone::Deleted),
            DiffKind::Staged => Some(DiffTone::Staged),
            DiffKind::Context | DiffKind::Hunk => None,
        })
    }
}

impl DiffPresentation {
    fn sign(&self) -> &'static str {
        self.0.map(DiffTone::sign).unwrap_or(" ")
    }

    fn row_class(&self) -> &'static str {
        self.0.map(DiffTone::row_class).unwrap_or("")
    }

    fn text_class(&self) -> &'static str {
        self.0
            .map(DiffTone::text_class)
            .unwrap_or("text-muted-foreground")
    }
}

#[component]
pub fn GitFooter(
    git_state: ReadSignal<FileGitState>,
    leading: Element,
    always_visible: bool,
    children: Element,
) -> Element {
    let state = git_state();
    let has_branch = !state.branch.is_empty();
    if !has_branch && !always_visible {
        return rsx! {};
    }
    let can_commit = has_branch && state.staged_count > 0;
    let can_push = has_branch && state.ahead > 0;

    rsx! {
        div {
            class: "flex h-7 min-w-0 shrink-0 items-center gap-3 overflow-hidden bg-foreground/[0.035] px-4 font-sans text-xs text-muted-foreground",

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
                        value: "{state.commit_message}",
                        oninput: move |event: Event<FormData>| {
                            let _ = send(&GitCommitDraftRequest {
                                message: event.value(),
                            });
                        },
                    }
                    Button {
                        variant: ButtonVariant::Ghost,
                        class: "h-auto shrink-0 px-2 py-0.5 text-xs hover:bg-foreground/10 disabled:opacity-40",
                        disabled: state.commit_message.trim().is_empty() || !state.commit_pending.is_empty(),
                        onclick: move |_| {
                            let _ = send(&GitCommitSubmitRequest);
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
                    class: "h-auto shrink-0 gap-1 px-2 py-0.5 text-xs hover:bg-foreground/10",
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
    viewport: ReadSignal<Option<GitDiffViewport>>,
    display_rows: ReadSignal<Vec<GitDiffRow>>,
    loading: bool,
    visible: bool,
) -> Element {
    let viewport = viewport();
    let rows = viewport
        .as_ref()
        .map(|viewport| viewport.lines.as_slice())
        .unwrap_or_default();
    let error = viewport
        .as_ref()
        .map(|viewport| viewport.error.as_str())
        .unwrap_or_default();
    let maxno = rows
        .iter()
        .flat_map(|l| [l.old_no, l.new_no])
        .flatten()
        .max()
        .unwrap_or(0);
    let gw = maxno.max(1).to_string().len().max(3);
    let total_rows = u32::try_from(rows.len()).unwrap_or(u32::MAX);
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
            class: if visible { "min-h-0 flex-1 overflow-auto bg-background font-mono text-xs leading-6" } else { "hidden" },

            if loading {
                div { class: "flex flex-col gap-2 p-3",
                    for width in ["w-10/12", "w-full", "w-8/12", "w-11/12", "w-7/12"] {
                        Skeleton { class: "h-4 {width} bg-foreground/[0.045]" }
                    }
                }
            } else if !error.is_empty() {
                div { class: "p-3 font-sans text-xs text-ansi-1", "{error}" }
            } else if rows.is_empty() {
                div { class: "p-3 text-xs text-muted-foreground", {translate("git-no-changes")} }
            }

            for display_row in display_rows() {
                match display_row {
                    GitDiffRow::Line(index) => {
                        let index = index as usize;
                        let line = &rows[index];
                        rsx! {
                            div { key: "line-{index}-{line.kind:?}-{line.old_no:?}-{line.new_no:?}",
                                div { class: "group flex min-w-max whitespace-pre transition-colors {DiffPresentation::from(line.kind).row_class()}",
                                    span { class: "flex shrink-0 select-none bg-card/70",
                                        span {
                                            class: "flex shrink-0 items-center justify-end px-2 text-right tabular-nums text-muted-foreground/45 group-hover:text-muted-foreground/75",
                                            style: "width:calc(var(--cw, 1ch) * {gw} + 1rem);",
                                            "{opt_no(line.old_no)}"
                                        }
                                        span {
                                            class: "flex shrink-0 items-center justify-end px-2 text-right tabular-nums text-muted-foreground/55 group-hover:text-muted-foreground/85",
                                            style: "width:calc(var(--cw, 1ch) * {gw} + 1rem);",
                                            "{opt_no(line.new_no)}"
                                        }
                                        span {
                                            class: "flex w-6 shrink-0 items-center justify-center font-semibold {DiffPresentation::from(line.kind).text_class()}",
                                            "{DiffPresentation::from(line.kind).sign()}"
                                        }
                                    }
                                    span { class: "min-w-0 px-2 pr-8",
                                        for (j, styled) in line.spans.iter().enumerate() {
                                            span { key: "{j}", style: "{span_style(styled)}", "{styled.text}" }
                                        }
                                    }
                                }
                                if let Some(h) = ends[index] {
                                    {
                                        let accept_path_bytes = path_bytes.clone();
                                        let reject_path_bytes = path_bytes.clone();
                                        rsx! {
                                            div {
                                                class: "flex min-w-full items-center justify-end gap-1.5 bg-card/45 px-3 py-1.5 font-sans text-[11px] select-none",
                                                Button {
                                                    variant: ButtonVariant::Ghost,
                                                    class: "h-6 gap-1 rounded-md bg-ansi-2/15 px-2 text-[11px] text-ansi-2 hover:bg-ansi-2/25 hover:text-ansi-2",
                                                    onclick: move |_| {
                                                        let _ = send(&GitHunkRequest {
                                                            repo_root: repo_root(),
                                                            path: path(),
                                                            path_bytes: accept_path_bytes.clone(),
                                                            hunk: h,
                                                            accept: true,
                                                        });
                                                    },
                                                    LineIconView { icon: LineIcon::Plus, class: "h-3 w-3" }
                                                    {translate("git-stage")}
                                                }
                                                Button {
                                                    variant: ButtonVariant::Ghost,
                                                    class: "h-6 gap-1 rounded-md border border-foreground/[0.16] bg-transparent px-2 text-[11px] text-muted-foreground hover:border-ansi-1/35 hover:bg-transparent hover:text-ansi-1",
                                                    onclick: move |_| {
                                                        let _ = send(&GitHunkRequest {
                                                            repo_root: repo_root(),
                                                            path: path(),
                                                            path_bytes: reject_path_bytes.clone(),
                                                            hunk: h,
                                                            accept: false,
                                                        });
                                                    },
                                                    LineIconView { icon: LineIcon::RotateCcw, class: "h-3 w-3" }
                                                    {translate("git-revert")}
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    },
                    GitDiffRow::Gap { start, end, .. } => rsx! {
                        DiffGap { key: "gap-{start}-{end}", start, end, total: total_rows }
                    },
                }
            }
        }
    }
}

struct DiffGapRange {
    start: u32,
    end: u32,
    total: u32,
}

impl DiffGapRange {
    fn hidden(&self) -> u32 {
        self.end.saturating_sub(self.start)
    }

    fn chunk(&self) -> u32 {
        self.hidden().min(GitDiffRevealRequest::CHUNK)
    }

    fn previous(&self) -> Option<(u32, u32)> {
        (self.hidden() > self.chunk() && self.end < self.total)
            .then(|| (self.end - self.chunk(), self.end))
    }

    fn next(&self) -> Option<(u32, u32)> {
        (self.hidden() > self.chunk() && self.start > 0)
            .then(|| (self.start, self.start + self.chunk()))
    }
}

#[component]
fn DiffGap(start: u32, end: u32, total: u32) -> Element {
    let range = DiffGapRange { start, end, total };
    let hidden = range.hidden();
    let chunk = range.chunk();
    let previous = range.previous();
    let next = range.next();
    let show_all = translate_with(
        "git-show-unchanged-lines",
        &[("count", TranslationValue::Number(hidden as i64))],
    );
    rsx! {
        div { class: "flex min-w-full items-center border-y border-foreground/[0.055] bg-foreground/[0.018] px-3 py-1 font-sans",
            div { class: "h-px min-w-4 flex-1 bg-foreground/[0.06]" }
            div { class: "mx-2 flex shrink-0 items-center gap-1",
                if let Some((start, end)) = previous {
                    Button {
                        variant: ButtonVariant::Ghost,
                        class: "h-6 shrink-0 gap-1 rounded-md border border-foreground/[0.08] bg-background/70 px-2 text-[10px] text-muted-foreground hover:bg-foreground/[0.055] hover:text-foreground",
                        title: translate_with(
                            "git-show-previous-lines",
                            &[("count", TranslationValue::Number(chunk as i64))],
                        ),
                        onclick: move |_| {
                            let _ = send(&GitDiffRevealRequest { start, end });
                        },
                        LineIconView { icon: LineIcon::ArrowUp, class: "size-3" }
                        "{chunk}"
                    }
                }
                Button {
                    variant: ButtonVariant::Ghost,
                    class: "h-6 shrink-0 gap-1.5 rounded-md border border-foreground/[0.08] bg-background/70 px-3 text-[10px] text-muted-foreground hover:bg-foreground/[0.055] hover:text-foreground",
                    title: show_all.clone(),
                    onclick: move |_| {
                        let _ = send(&GitDiffRevealRequest { start, end });
                    },
                    span { "⋯" }
                    span { "{show_all}" }
                }
                if let Some((start, end)) = next {
                    Button {
                        variant: ButtonVariant::Ghost,
                        class: "h-6 shrink-0 gap-1 rounded-md border border-foreground/[0.08] bg-background/70 px-2 text-[10px] text-muted-foreground hover:bg-foreground/[0.055] hover:text-foreground",
                        title: translate_with(
                            "git-show-next-lines",
                            &[("count", TranslationValue::Number(chunk as i64))],
                        ),
                        onclick: move |_| {
                            let _ = send(&GitDiffRevealRequest { start, end });
                        },
                        "{chunk}"
                        LineIconView { icon: LineIcon::ArrowDown, class: "size-3" }
                    }
                }
            }
            div { class: "h-px min-w-4 flex-1 bg-foreground/[0.06]" }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn large_middle_gap_offers_both_chunk_directions() {
        let gap = DiffGapRange {
            start: 40,
            end: 140,
            total: 200,
        };

        assert_eq!(gap.previous(), Some((120, 140)));
        assert_eq!(gap.next(), Some((40, 60)));
    }

    #[test]
    fn edge_gap_only_expands_toward_the_file_edge() {
        let top = DiffGapRange {
            start: 0,
            end: 100,
            total: 200,
        };
        let bottom = DiffGapRange {
            start: 100,
            end: 200,
            total: 200,
        };

        assert_eq!(top.previous(), Some((80, 100)));
        assert_eq!(top.next(), None);
        assert_eq!(bottom.previous(), None);
        assert_eq!(bottom.next(), Some((100, 120)));
    }
}
