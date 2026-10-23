#![allow(non_snake_case)]

use std::collections::HashSet;
use std::rc::Rc;

use dioxus::prelude::*;
use vmux_ui::components::button::{Button, ButtonVariant};
use vmux_ui::components::checkbox::{Checkbox, CheckboxTone};
use vmux_ui::components::error_state::ErrorState;
use vmux_ui::components::sheet::SheetHeaderChevron;
use vmux_ui::components::skeleton::Skeleton;
use vmux_ui::diff::DiffTone;
use vmux_ui::file_icon::TypeIcon;
use vmux_ui::hooks::send;
use vmux_ui::i18n::{TranslationValue, translate, translate_with};
use vmux_ui::icon::{LineIcon, LineIconView};
use vmux_ui::scroll::ScrollIntoView;

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

fn span_style(span: &StyledSpan) -> String {
    let [lr, lg, lb] = span.light_fg;
    let [dr, dg, db] = span.dark_fg;
    let mut s = format!("--syntax-light:rgb({lr},{lg},{lb});--syntax-dark:rgb({dr},{dg},{db});");
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

struct DiffPresentation(DiffKind);

impl From<DiffKind> for DiffPresentation {
    fn from(kind: DiffKind) -> Self {
        Self(kind)
    }
}

impl DiffPresentation {
    fn sign(&self) -> &'static str {
        match self.0 {
            DiffKind::Add => DiffTone::Added.sign(),
            DiffKind::Remove => DiffTone::Deleted.sign(),
            DiffKind::Staged => DiffTone::Staged.sign(),
            DiffKind::Context | DiffKind::Hunk => " ",
        }
    }

    fn row_class(&self) -> &'static str {
        match self.0 {
            DiffKind::Add => "bg-git-added hover:bg-git-added-hover",
            DiffKind::Remove => "bg-git-removed hover:bg-git-removed-hover",
            DiffKind::Staged => "bg-git-staged hover:bg-git-staged-hover",
            DiffKind::Hunk => "bg-git-hunk",
            DiffKind::Context => "hover:bg-foreground/[0.025]",
        }
    }

    fn text_class(&self) -> &'static str {
        match self.0 {
            DiffKind::Add => "text-git-added-foreground",
            DiffKind::Remove => "text-git-removed-foreground",
            DiffKind::Staged => "text-git-staged-foreground",
            DiffKind::Hunk => "text-git-hunk-foreground",
            DiffKind::Context => "text-muted-foreground",
        }
    }

    fn gutter_class(&self) -> &'static str {
        match self.0 {
            DiffKind::Add => "bg-git-added-gutter",
            DiffKind::Remove => "bg-git-removed-gutter",
            DiffKind::Staged => "bg-git-staged-gutter",
            DiffKind::Hunk => "bg-git-hunk-gutter",
            DiffKind::Context => "bg-foreground/[0.025]",
        }
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
                        send(&GitPushRequest { path: git_state().path });
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
    #[props(default)] identity: String,
    #[props(default)] reveal_path: Option<ReadSignal<String>>,
    viewport: ReadSignal<Option<GitDiffViewport>>,
    display_rows: ReadSignal<Vec<GitDiffRow>>,
    loading: bool,
    visible: bool,
) -> Element {
    let mut collapsed_files = use_signal(HashSet::<String>::new);
    let mut viewed_files = use_signal(HashSet::<String>::new);
    let mut scroll_container = use_signal(|| None::<Rc<MountedData>>);
    let mut render_window = use_signal(DiffRenderWindow::default);
    let mut revealed_file = use_signal(String::new);
    let layout = use_memo(move || {
        let viewport = viewport.read();
        let lines = viewport
            .as_ref()
            .map(|viewport| viewport.lines.as_slice())
            .unwrap_or_default();
        let rows = display_rows.read();
        DiffLayout::group(lines, &rows)
    });
    let gutter_width = use_memo(move || {
        let viewport = viewport.read();
        let rows = viewport
            .as_ref()
            .map(|viewport| viewport.lines.as_slice())
            .unwrap_or_default();
        rows.iter()
            .flat_map(|line| [line.old_no, line.new_no])
            .flatten()
            .max()
            .unwrap_or(0)
            .max(1)
            .to_string()
            .len()
            .max(3)
    });
    let hunk_ends = use_memo(move || {
        if path.read().is_empty() {
            return Vec::new();
        }
        let viewport = viewport.read();
        let rows = viewport
            .as_ref()
            .map(|viewport| viewport.lines.as_slice())
            .unwrap_or_default();
        rows.iter()
            .enumerate()
            .map(|(index, line)| match line.hunk {
                Some(hunk) if index + 1 == rows.len() || rows[index + 1].hunk != Some(hunk) => {
                    Some(hunk)
                }
                _ => None,
            })
            .collect::<Vec<_>>()
    });
    let reveal_identity = identity.clone();
    use_effect(move || {
        let Some(reveal_path) = reveal_path else {
            return;
        };
        let path = reveal_path();
        if path.is_empty() {
            return;
        }
        let key = format!("{reveal_identity}\0{path}");
        if *revealed_file.peek() == key {
            return;
        }
        let collapsed = collapsed_files.read();
        let Some(offset) = layout
            .read()
            .file_offset(&reveal_identity, &collapsed, &path)
        else {
            return;
        };
        revealed_file.set(key);
        let height = render_window.peek().height;
        render_window.set(DiffRenderWindow::at(offset, height));
        ScrollIntoView::element_to("git-diff-viewport", offset);
    });
    let viewport_state = viewport.read();
    let rows = viewport_state
        .as_ref()
        .map(|viewport| viewport.lines.as_slice())
        .unwrap_or_default();
    let error = viewport_state
        .as_ref()
        .map(|viewport| viewport.error.as_str())
        .unwrap_or_default();
    let has_more = viewport_state
        .as_ref()
        .is_some_and(|viewport| viewport.has_more);
    let gw = gutter_width();
    let total_rows = u32::try_from(rows.len()).unwrap_or(u32::MAX);
    let ends = hunk_ends.read();
    let collapsed_snapshot = collapsed_files();
    let viewed_snapshot = viewed_files();
    let virtualized = path.read().is_empty();
    let VisibleDiffLayout {
        top_spacer,
        bottom_spacer,
        sections,
    } = layout.read().visible(
        &identity,
        &collapsed_snapshot,
        *render_window.peek(),
        virtualized,
    );
    use_effect(move || {
        let viewport = viewport.read();
        let has_more = viewport.as_ref().is_some_and(|viewport| viewport.has_more);
        let row_count = viewport
            .as_ref()
            .map(|viewport| viewport.lines.len())
            .unwrap_or_default();
        if loading || !has_more || row_count == 0 {
            return;
        }
        spawn(async move {
            let Some(container) = scroll_container.peek().clone() else {
                return;
            };
            let Ok(size) = container.get_scroll_size().await else {
                return;
            };
            let Ok(rect) = container.get_client_rect().await else {
                return;
            };
            if size.height <= rect.size.height * 4.0 {
                send(&GitDiffNextRequest);
            }
        });
    });

    rsx! {
        div {
            key: "{identity}",
            id: "git-diff-viewport",
            onmounted: move |event: Event<MountedData>| {
                let element = event.data();
                scroll_container.set(Some(element.clone()));
                spawn(async move {
                    let Ok(rect) = element.get_client_rect().await else {
                        return;
                    };
                    render_window.set(DiffRenderWindow::at(0.0, rect.size.height));
                });
            },
            class: if visible { "min-h-0 flex-1 overflow-auto bg-transparent font-mono text-[11px] leading-5" } else { "hidden" },
            onscroll: move |event| {
                let next = DiffRenderWindow::from_scroll(
                    event.scroll_top(),
                    f64::from(event.client_height()),
                );
                if *render_window.peek() != next {
                    render_window.set(next);
                }
                let remaining = event.scroll_height() - event.scroll_top() as i32 - event.client_height();
                let preload_distance = event.client_height().saturating_mul(6).max(1600);
                if remaining <= preload_distance && has_more && !loading {
                    send(&GitDiffNextRequest);
                }
            },

            if loading && rows.is_empty() {
                div { class: "flex flex-col gap-2 p-3",
                    for width in ["w-10/12", "w-full", "w-8/12", "w-11/12", "w-7/12"] {
                        Skeleton { class: "h-4 {width} bg-foreground/[0.045]" }
                    }
                }
            } else if !error.is_empty() {
                ErrorState {
                    class: "min-h-64 font-sans".to_string(),
                    message: error.to_string(),
                    on_retry: move |_| {
                        send(&GitDiffRetryRequest);
                    },
                }
            } else if rows.is_empty() {
                div { class: "p-3 text-xs text-muted-foreground", {translate("git-no-changes")} }
            }

            if top_spacer > 0.0 {
                div { style: "height:{top_spacer}px;" }
            }
            for section in sections {
                {
                    let file_path = section.file_path.clone();
                    let collapse_key = file_path
                        .as_ref()
                        .map(|file_path| format!("{identity}\0{file_path}"));
                    let collapsed = collapse_key
                        .as_ref()
                        .is_some_and(|key| collapsed_snapshot.contains(key));
                    let viewed = collapse_key
                        .as_ref()
                        .is_some_and(|key| viewed_snapshot.contains(key));
                    let viewed_key = collapse_key.clone();
                    let section_class = if file_path.is_some() && section.index > 0 {
                        "mt-3 w-full min-w-0 overflow-clip rounded-lg bg-card"
                    } else if file_path.is_some() {
                        "w-full min-w-0 overflow-clip rounded-lg bg-card"
                    } else {
                        "contents"
                    };
                    rsx! {
                        section { key: "section-{section.index}-{file_path:?}", class: section_class,
                            if let Some(file_path) = file_path {
                                div {
                                    id: "git-diff-file-{file_path}",
                                    class: "sticky left-0 top-0 z-10 flex h-9 w-full min-w-0 scroll-mt-2 items-center gap-2 rounded-t-lg bg-card px-3 font-sans text-[11px] font-semibold text-foreground",
                                    TypeIcon { path: file_path.clone(), is_dir: false, class: "size-3.5 shrink-0" }
                                    span { class: "min-w-0 flex-1 truncate font-mono", "{file_path}" }
                                    label { class: "flex shrink-0 cursor-pointer items-center gap-1.5 font-sans font-normal text-muted-foreground",
                                        Checkbox {
                                            checked: viewed,
                                            tone: CheckboxTone::Neutral,
                                            aria_label: translate("git-viewed"),
                                            on_checked_change: move |checked| {
                                                let Some(key) = viewed_key.as_ref() else {
                                                    return;
                                                };
                                                let mut viewed = viewed_files.write();
                                                let mut collapsed = collapsed_files.write();
                                                if checked {
                                                    viewed.insert(key.clone());
                                                    collapsed.insert(key.clone());
                                                } else {
                                                    viewed.remove(key);
                                                    collapsed.remove(key);
                                                }
                                            },
                                        }
                                        {translate("git-viewed")}
                                    }
                                    SheetHeaderChevron {
                                        expanded: !collapsed,
                                        label: translate(if collapsed { "common-expand" } else { "common-collapse" }),
                                        onclick: move |_| {
                                            let Some(key) = collapse_key.as_ref() else {
                                                return;
                                            };
                                            let mut files = collapsed_files.write();
                                            if !files.remove(key) {
                                                files.insert(key.clone());
                                            }
                                        },
                                    }
                                }
                            }
                            if !collapsed {
                                if section.top_spacer > 0.0 {
                                    div { style: "height:{section.top_spacer}px;" }
                                }
                                for display_row in section.rows {
                                    match display_row {
                                        GitDiffRow::Line(index) => {
                                            let index = index as usize;
                                            let line = &rows[index];
                                            rsx! {
                                                div { key: "line-{index}-{line.kind:?}-{line.old_no:?}-{line.new_no:?}",
                                                    div { class: "group flex min-w-max whitespace-pre transition-colors {DiffPresentation::from(line.kind).row_class()}",
                                                        span { class: "flex shrink-0 select-none {DiffPresentation::from(line.kind).gutter_class()}",
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
                                                                span { key: "{j}", class: "syntax-token", style: "{span_style(styled)}", "{styled.text}" }
                                                            }
                                                        }
                                                    }
                                                    if let Some(h) = ends.get(index).copied().flatten() {
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
                                                                                send(&GitHunkRequest {
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
                                                                                send(&GitHunkRequest {
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
                                if section.bottom_spacer > 0.0 {
                                    div { style: "height:{section.bottom_spacer}px;" }
                                }
                            }
                        }
                    }
                }
            }
            if bottom_spacer > 0.0 {
                div { style: "height:{bottom_spacer}px;" }
            }
            if (loading || has_more) && !rows.is_empty() && error.is_empty() {
                div {
                    key: "diff-next-{rows.len()}",
                    class: "flex flex-col gap-2 p-3",
                    div { class: "h-8 rounded-md bg-card/70" }
                    for width in ["w-11/12", "w-9/12", "w-10/12", "w-7/12"] {
                        Skeleton { class: "h-4 {width} bg-foreground/[0.045]" }
                    }
                }
            }
        }
    }
}

const DIFF_LINE_HEIGHT: f64 = 20.0;
const DIFF_GAP_HEIGHT: f64 = 34.0;
const DIFF_FILE_HEADER_HEIGHT: f64 = 36.0;
const DIFF_SECTION_GAP: f64 = 12.0;
const DIFF_RENDER_OVERSCAN: f64 = 1200.0;
const DIFF_RENDER_STEP: f64 = 320.0;

#[derive(Clone, Copy, Default, PartialEq)]
struct DiffRenderWindow {
    top: f64,
    height: f64,
}

impl DiffRenderWindow {
    fn at(top: f64, height: f64) -> Self {
        Self {
            top: (top.max(0.0) / DIFF_RENDER_STEP).floor() * DIFF_RENDER_STEP,
            height: height.max(1600.0),
        }
    }

    fn from_scroll(top: f64, height: f64) -> Self {
        Self::at(top, height)
    }

    fn range(self) -> (f64, f64) {
        (
            (self.top - DIFF_RENDER_OVERSCAN).max(0.0),
            self.top + self.height + DIFF_RENDER_OVERSCAN,
        )
    }
}

#[derive(Clone, PartialEq)]
struct DiffLayout {
    sections: Vec<DiffSection>,
}

impl DiffLayout {
    fn group(lines: &[DiffLine], rows: &[GitDiffRow]) -> Self {
        let mut sections: Vec<DiffSection> = Vec::new();
        for row in rows {
            let file_path = match row {
                GitDiffRow::Line(index) => lines
                    .get(*index as usize)
                    .and_then(DiffLine::file_path)
                    .map(str::to_string),
                GitDiffRow::Gap { .. } => None,
            };
            if let Some(file_path) = file_path {
                sections.push(DiffSection::new(Some(file_path)));
                continue;
            }
            if sections.is_empty() {
                sections.push(DiffSection::new(None));
            }
            if let Some(section) = sections.last_mut() {
                section.push(*row);
            }
        }
        Self { sections }
    }

    fn file_offset(&self, identity: &str, collapsed: &HashSet<String>, path: &str) -> Option<f64> {
        let mut offset = 0.0;
        for (index, section) in self.sections.iter().enumerate() {
            if section.file_path.as_deref() == Some(path) {
                return Some(offset + section.margin(index));
            }
            offset += section.height(index, identity, collapsed);
        }
        None
    }

    fn visible(
        &self,
        identity: &str,
        collapsed: &HashSet<String>,
        window: DiffRenderWindow,
        virtualized: bool,
    ) -> VisibleDiffLayout {
        let (range_start, range_end) = if virtualized {
            window.range()
        } else {
            (0.0, f64::MAX)
        };
        let mut offset = 0.0;
        let mut first_visible = None;
        let mut last_visible = 0.0;
        let mut visible = Vec::new();
        let total_height = self.total_height(identity, collapsed);
        for (index, section) in self.sections.iter().enumerate() {
            let section_start = offset;
            let section_end = section_start + section.height(index, identity, collapsed);
            offset = section_end;
            if section_end < range_start {
                continue;
            }
            if section_start > range_end {
                break;
            }
            first_visible.get_or_insert(section_start);
            last_visible = section_end;
            visible.push(section.visible(
                index,
                identity,
                collapsed,
                section_start,
                range_start,
                range_end,
            ));
        }
        if visible.is_empty() {
            return VisibleDiffLayout {
                top_spacer: total_height,
                bottom_spacer: 0.0,
                sections: visible,
            };
        }
        let top_spacer = first_visible.unwrap_or_default();
        VisibleDiffLayout {
            top_spacer,
            bottom_spacer: (total_height - last_visible).max(0.0),
            sections: visible,
        }
    }

    fn total_height(&self, identity: &str, collapsed: &HashSet<String>) -> f64 {
        self.sections
            .iter()
            .enumerate()
            .map(|(index, section)| section.height(index, identity, collapsed))
            .sum()
    }
}

#[derive(Clone, PartialEq)]
struct DiffSection {
    file_path: Option<String>,
    rows: Vec<GitDiffRow>,
    row_offsets: Vec<f64>,
}

impl DiffSection {
    fn new(file_path: Option<String>) -> Self {
        Self {
            file_path,
            rows: Vec::new(),
            row_offsets: vec![0.0],
        }
    }

    fn push(&mut self, row: GitDiffRow) {
        let offset = self.row_offsets.last().copied().unwrap_or_default();
        self.rows.push(row);
        self.row_offsets.push(offset + DiffRowHeight::of(row));
    }

    fn margin(&self, index: usize) -> f64 {
        if self.file_path.is_some() && index > 0 {
            DIFF_SECTION_GAP
        } else {
            0.0
        }
    }

    fn collapsed(&self, identity: &str, collapsed: &HashSet<String>) -> bool {
        self.file_path
            .as_ref()
            .is_some_and(|path| collapsed.contains(&format!("{identity}\0{path}")))
    }

    fn height(&self, index: usize, identity: &str, collapsed: &HashSet<String>) -> f64 {
        let header = self
            .file_path
            .as_ref()
            .map(|_| DIFF_FILE_HEADER_HEIGHT)
            .unwrap_or_default();
        let rows = if self.collapsed(identity, collapsed) {
            0.0
        } else {
            self.row_offsets.last().copied().unwrap_or_default()
        };
        self.margin(index) + header + rows
    }

    fn visible(
        &self,
        index: usize,
        identity: &str,
        collapsed: &HashSet<String>,
        section_start: f64,
        range_start: f64,
        range_end: f64,
    ) -> VisibleDiffSection {
        let is_collapsed = self.collapsed(identity, collapsed);
        let content_start = section_start
            + self.margin(index)
            + self
                .file_path
                .as_ref()
                .map(|_| DIFF_FILE_HEADER_HEIGHT)
                .unwrap_or_default();
        let local_start = (range_start - content_start).max(0.0);
        let local_end = (range_end - content_start).max(0.0);
        let first = if is_collapsed {
            0
        } else {
            self.row_offsets[1..].partition_point(|end| *end < local_start)
        };
        let end = if is_collapsed {
            0
        } else {
            self.row_offsets[..self.rows.len()].partition_point(|start| *start <= local_end)
        };
        let total = self.row_offsets.last().copied().unwrap_or_default();
        let top_spacer = self.row_offsets.get(first).copied().unwrap_or_default();
        let bottom_spacer = total - self.row_offsets.get(end).copied().unwrap_or(total);
        VisibleDiffSection {
            index,
            file_path: self.file_path.clone(),
            rows: self.rows[first..end].to_vec(),
            top_spacer,
            bottom_spacer,
        }
    }
}

struct DiffRowHeight;

impl DiffRowHeight {
    fn of(row: GitDiffRow) -> f64 {
        match row {
            GitDiffRow::Line(_) => DIFF_LINE_HEIGHT,
            GitDiffRow::Gap { .. } => DIFF_GAP_HEIGHT,
        }
    }
}

struct VisibleDiffLayout {
    top_spacer: f64,
    bottom_spacer: f64,
    sections: Vec<VisibleDiffSection>,
}

struct VisibleDiffSection {
    index: usize,
    file_path: Option<String>,
    rows: Vec<GitDiffRow>,
    top_spacer: f64,
    bottom_spacer: f64,
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
                            send(&GitDiffRevealRequest { start, end });
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
                        send(&GitDiffRevealRequest { start, end });
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
                            send(&GitDiffRevealRequest { start, end });
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

    #[test]
    fn virtualized_diff_keeps_the_full_scroll_height() {
        let mut section = DiffSection::new(Some("src/lib.rs".to_string()));
        for index in 0..500 {
            section.push(GitDiffRow::Line(index));
        }
        let layout = DiffLayout {
            sections: vec![section],
        };
        let visible = layout.visible(
            "branch",
            &HashSet::new(),
            DiffRenderWindow {
                top: 5_000.0,
                height: 800.0,
            },
            true,
        );
        let section = &visible.sections[0];
        let rendered = section.rows.len() as f64 * DIFF_LINE_HEIGHT;

        assert!(section.rows.len() < 500);
        assert_eq!(
            visible.top_spacer
                + DIFF_FILE_HEADER_HEIGHT
                + section.top_spacer
                + rendered
                + section.bottom_spacer
                + visible.bottom_spacer,
            500.0 * DIFF_LINE_HEIGHT + DIFF_FILE_HEADER_HEIGHT,
        );
    }

    #[test]
    fn a_window_past_the_diff_does_not_duplicate_the_spacer() {
        let mut section = DiffSection::new(Some("src/lib.rs".to_string()));
        section.push(GitDiffRow::Line(0));
        let layout = DiffLayout {
            sections: vec![section],
        };
        let visible = layout.visible(
            "branch",
            &HashSet::new(),
            DiffRenderWindow {
                top: 10_000.0,
                height: 1.0,
            },
            true,
        );

        assert!(visible.sections.is_empty());
        assert_eq!(
            visible.top_spacer,
            DIFF_FILE_HEADER_HEIGHT + DIFF_LINE_HEIGHT
        );
        assert_eq!(visible.bottom_spacer, 0.0);
    }
}
