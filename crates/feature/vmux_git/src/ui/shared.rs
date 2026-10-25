#![allow(non_snake_case)]

use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::path::Path;
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
use vmux_ui::navigation::PageNavigation;
use vmux_ui::scroll::{ScrollIntoView, ScrollPreload};

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
        if !ScrollPreload::LIST.reached(&event) {
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
    #[props(default)] identity: String,
    #[props(default)] reveal_path: Option<ReadSignal<String>>,
    #[props(default)] onactivefile: Option<EventHandler<String>>,
    #[props(default)] onviewedchange: Option<EventHandler<usize>>,
    #[props(default)] expected_paths: Vec<String>,
    viewport: ReadSignal<Option<GitDiffViewport>>,
    display_rows: ReadSignal<Vec<GitDiffRow>>,
    loading: bool,
    visible: bool,
) -> Element {
    let mut collapsed_files = use_signal(HashSet::<String>::new);
    let mut viewed_files = use_signal(ViewedFiles::default);
    let mut scroll_container = use_signal(|| None::<Rc<MountedData>>);
    let mut render_window = use_signal(DiffRenderWindow::default);
    let mut revealed_file = use_signal(String::new);
    let mut active_file = use_signal(String::new);
    let expected_path_count = expected_paths.len();
    let layout_expected_paths = expected_paths.clone();
    let layout = use_memo(move || {
        let viewport = viewport.read();
        let lines = viewport
            .as_ref()
            .map(|viewport| viewport.lines.as_slice())
            .unwrap_or_default();
        let rows = display_rows.read();
        DiffLayout::group(lines, &rows, &layout_expected_paths)
    });
    let geometry_identity = identity.clone();
    let geometry = use_memo(move || {
        DiffGeometry::new(&layout.read(), &geometry_identity, &collapsed_files.read())
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
        let layout = layout.read();
        let geometry = geometry.read();
        let Some(offset) = geometry.file_offset(&layout, &path) else {
            return;
        };
        revealed_file.set(key);
        let height = render_window.peek().height;
        render_window.set(DiffRenderWindow::at(offset, height));
        ScrollIntoView::element_to("git-diff-viewport", offset);
    });
    let viewed_identity = identity.clone();
    use_effect(move || {
        let layout = layout.read();
        let mut viewed = viewed_files.write();
        let mut collapsed = collapsed_files.write();
        viewed.reconcile(&viewed_identity, &layout, &mut collapsed);
        if let Some(handler) = onviewedchange {
            handler.call(viewed.count(&viewed_identity, layout.file_paths()));
        }
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
        &geometry.read(),
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
                let _ = send(&GitDiffNextRequest);
            }
        });
    });
    let scroll_identity = identity.clone();

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
                if let Some(path) = geometry
                    .read()
                    .file_at(&layout.read(), event.scroll_top() + 1.0)
                    && *active_file.peek() != path
                {
                    active_file.set(path.clone());
                    revealed_file.set(format!("{scroll_identity}\0{path}"));
                    if let Some(handler) = onactivefile {
                        handler.call(path);
                    }
                }
                if ScrollPreload::DOCUMENT.reached(&event) && has_more && !loading {
                    let _ = send(&GitDiffNextRequest);
                }
            },

            if loading && rows.is_empty() && expected_path_count == 0 {
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
                        let _ = send(&GitDiffRetryRequest);
                    },
                }
            } else if rows.is_empty() && expected_path_count == 0 {
                div { class: "p-3 text-xs text-muted-foreground", {translate("git-no-changes")} }
            }

            if top_spacer > 0.0 {
                div { style: "height:{top_spacer}px;" }
            }
            for section in sections {
                {
                    let file_path = section.file_path.clone();
                    let placeholder = section.placeholder;
                    let collapse_key = file_path
                        .as_ref()
                        .map(|file_path| format!("{identity}\0{file_path}"));
                    let collapsed = collapse_key
                        .as_ref()
                        .is_some_and(|key| collapsed_snapshot.contains(key));
                    let viewed = collapse_key
                        .as_ref()
                        .is_some_and(|key| viewed_snapshot.contains(key, section.fingerprint));
                    let viewed_key = collapse_key.clone();
                    let viewed_fingerprint = section.fingerprint;
                    let viewed_count_identity = identity.clone();
                    let viewed_count_layout = layout;
                    let file_url = file_path
                        .as_ref()
                        .map(|file_path| {
                            vmux_path::FileUrl::from_path(
                                &Path::new(&repo_root()).join(file_path),
                                None,
                                None,
                                None,
                            )
                        })
                        .unwrap_or_default();
                    let section_class = if file_path.is_some() && section.index > 0 {
                        "mt-3 w-full min-w-0 overflow-clip rounded-lg bg-foreground/[0.035]"
                    } else if file_path.is_some() {
                        "w-full min-w-0 overflow-clip rounded-lg bg-foreground/[0.035]"
                    } else {
                        "contents"
                    };
                    rsx! {
                        section { key: "section-{section.index}-{file_path:?}", class: section_class,
                            if let Some(file_path) = file_path {
                                div {
                                    id: "git-diff-file-{file_path}",
                                    class: "sticky left-0 top-0 z-10 flex h-9 w-full min-w-0 scroll-mt-2 items-center gap-2 rounded-t-lg bg-inherit px-3 font-sans text-[11px] font-semibold text-foreground backdrop-blur-xl",
                                    TypeIcon { path: file_path.clone(), is_dir: false, class: "size-3.5 shrink-0" }
                                    button {
                                        r#type: "button",
                                        class: "min-w-0 flex-1 truncate text-left font-mono transition-colors hover:text-primary hover:underline",
                                        title: file_path.clone(),
                                        onclick: move |_| {
                                            PageNavigation::stack(file_url.clone());
                                        },
                                        "{file_path}"
                                    }
                                    if !placeholder {
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
                                                        viewed.insert(key.clone(), viewed_fingerprint);
                                                        collapsed.insert(key.clone());
                                                    } else {
                                                        viewed.remove(key);
                                                        collapsed.remove(key);
                                                    }
                                                    if let Some(handler) = onviewedchange {
                                                        handler.call(viewed.count(
                                                            &viewed_count_identity,
                                                            viewed_count_layout.read().file_paths(),
                                                        ));
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
                            }
                            if placeholder {
                                div { class: "flex h-36 flex-col gap-2 px-3 py-3",
                                    for width in ["w-11/12", "w-9/12", "w-10/12", "w-7/12"] {
                                        Skeleton { class: "h-4 {width} bg-foreground/[0.045]" }
                                    }
                                }
                            } else if !collapsed {
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
        }
    }
}

const DIFF_LINE_HEIGHT: f64 = 20.0;
const DIFF_GAP_HEIGHT: f64 = 34.0;
const DIFF_FILE_HEADER_HEIGHT: f64 = 36.0;
const DIFF_SECTION_GAP: f64 = 12.0;
const DIFF_PLACEHOLDER_HEIGHT: f64 = 180.0;
const DIFF_RENDER_OVERSCAN: f64 = 1600.0;
const DIFF_RENDER_STEP: f64 = 640.0;

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
    fn group(lines: &[DiffLine], rows: &[GitDiffRow], expected_paths: &[String]) -> Self {
        let fingerprints = DiffFingerprints::of(lines);
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
                let fingerprint = fingerprints.get(&file_path);
                sections.push(DiffSection::new(Some(file_path), fingerprint));
                continue;
            }
            if sections.is_empty() {
                sections.push(DiffSection::new(None, None));
            }
            if let Some(section) = sections.last_mut() {
                section.push(*row);
            }
        }
        if expected_paths.is_empty() {
            return Self { sections };
        }
        let mut loaded = HashMap::new();
        let mut loaded_order = Vec::new();
        let mut loose = Vec::new();
        for section in sections {
            match section.file_path.as_ref() {
                Some(path) => {
                    loaded_order.push(path.clone());
                    loaded.insert(path.clone(), section);
                }
                None => loose.push(section),
            }
        }
        let mut sections = Vec::with_capacity(expected_paths.len() + loose.len() + loaded.len());
        sections.extend(loose);
        for path in expected_paths {
            sections.push(
                loaded
                    .remove(path)
                    .unwrap_or_else(|| DiffSection::placeholder(path.clone())),
            );
        }
        for path in loaded_order {
            if let Some(section) = loaded.remove(&path) {
                sections.push(section);
            }
        }
        Self { sections }
    }

    fn file_paths(&self) -> impl Iterator<Item = &str> {
        self.sections
            .iter()
            .filter(|section| !section.placeholder)
            .filter_map(|section| section.file_path.as_deref())
    }

    fn visible(
        &self,
        identity: &str,
        collapsed: &HashSet<String>,
        geometry: &DiffGeometry,
        window: DiffRenderWindow,
        virtualized: bool,
    ) -> VisibleDiffLayout {
        let (range_start, range_end) = if virtualized {
            window.range()
        } else {
            (0.0, f64::MAX)
        };
        let mut first_visible = None;
        let mut last_visible = 0.0;
        let mut visible = Vec::new();
        let first = geometry
            .ends
            .partition_point(|section_end| *section_end < range_start);
        for (index, section) in self.sections.iter().enumerate().skip(first) {
            let section_start = geometry.starts[index];
            let section_end = geometry.ends[index];
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
                top_spacer: geometry.total_height,
                bottom_spacer: 0.0,
                sections: visible,
            };
        }
        let top_spacer = first_visible.unwrap_or_default();
        VisibleDiffLayout {
            top_spacer,
            bottom_spacer: (geometry.total_height - last_visible).max(0.0),
            sections: visible,
        }
    }
}

#[derive(Clone, PartialEq)]
struct DiffGeometry {
    starts: Vec<f64>,
    ends: Vec<f64>,
    total_height: f64,
}

impl DiffGeometry {
    fn new(layout: &DiffLayout, identity: &str, collapsed: &HashSet<String>) -> Self {
        let mut starts = Vec::with_capacity(layout.sections.len());
        let mut ends = Vec::with_capacity(layout.sections.len());
        let mut offset = 0.0;
        for (index, section) in layout.sections.iter().enumerate() {
            starts.push(offset);
            offset += section.height(index, identity, collapsed);
            ends.push(offset);
        }
        Self {
            starts,
            ends,
            total_height: offset,
        }
    }

    fn file_offset(&self, layout: &DiffLayout, path: &str) -> Option<f64> {
        layout
            .sections
            .iter()
            .position(|section| section.file_path.as_deref() == Some(path))
            .map(|index| self.starts[index] + layout.sections[index].margin(index))
    }

    fn file_at(&self, layout: &DiffLayout, offset: f64) -> Option<String> {
        let index = self.ends.partition_point(|end| *end <= offset);
        let index = index.min(layout.sections.len().saturating_sub(1));
        layout.sections[..=index]
            .iter()
            .rev()
            .find_map(|section| section.file_path.clone())
    }
}

#[derive(Clone, PartialEq)]
struct DiffSection {
    file_path: Option<String>,
    fingerprint: Option<DiffFingerprint>,
    placeholder: bool,
    rows: Vec<GitDiffRow>,
    row_offsets: Vec<f64>,
}

impl DiffSection {
    fn new(file_path: Option<String>, fingerprint: Option<DiffFingerprint>) -> Self {
        Self {
            file_path,
            fingerprint,
            placeholder: false,
            rows: Vec::new(),
            row_offsets: vec![0.0],
        }
    }

    fn placeholder(file_path: String) -> Self {
        Self {
            file_path: Some(file_path),
            fingerprint: None,
            placeholder: true,
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
        if self.placeholder {
            return self.margin(index) + DIFF_PLACEHOLDER_HEIGHT;
        }
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
            fingerprint: self.fingerprint,
            placeholder: self.placeholder,
            rows: self.rows[first..end].to_vec(),
            top_spacer,
            bottom_spacer,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DiffFingerprint(u64);

impl DiffFingerprint {
    fn write(line: &DiffLine, hasher: &mut DefaultHasher) {
        let kind = match line.kind {
            DiffKind::Context => 0_u8,
            DiffKind::Add => 1,
            DiffKind::Remove => 2,
            DiffKind::Hunk => 3,
            DiffKind::Staged => 4,
        };
        kind.hash(hasher);
        line.old_no.hash(hasher);
        line.new_no.hash(hasher);
        line.hunk.hash(hasher);
        for span in &line.spans {
            span.text.hash(hasher);
        }
    }
}

#[derive(Default)]
struct DiffFingerprints(HashMap<String, DiffFingerprint>);

impl DiffFingerprints {
    fn of(lines: &[DiffLine]) -> Self {
        let mut fingerprints = HashMap::new();
        let mut current: Option<(String, DefaultHasher)> = None;
        for line in lines {
            if let Some(path) = line.file_path() {
                if let Some((path, hasher)) = current.take() {
                    fingerprints.insert(path, DiffFingerprint(hasher.finish()));
                }
                current = Some((path.to_string(), DefaultHasher::new()));
            }
            if let Some((_, hasher)) = current.as_mut() {
                DiffFingerprint::write(line, hasher);
            }
        }
        if let Some((path, hasher)) = current {
            fingerprints.insert(path, DiffFingerprint(hasher.finish()));
        }
        Self(fingerprints)
    }

    fn get(&self, path: &str) -> Option<DiffFingerprint> {
        self.0.get(path).copied()
    }
}

#[derive(Clone, Default)]
struct ViewedFiles(HashMap<String, DiffFingerprint>);

impl ViewedFiles {
    fn count<'a>(&self, identity: &str, paths: impl Iterator<Item = &'a str>) -> usize {
        paths
            .filter(|path| {
                let key = format!("{identity}\0{path}");
                self.0.contains_key(&key)
            })
            .count()
    }

    fn contains(&self, key: &str, fingerprint: Option<DiffFingerprint>) -> bool {
        fingerprint.is_some_and(|fingerprint| self.0.get(key) == Some(&fingerprint))
    }

    fn insert(&mut self, key: String, fingerprint: Option<DiffFingerprint>) {
        if let Some(fingerprint) = fingerprint {
            self.0.insert(key, fingerprint);
        }
    }

    fn remove(&mut self, key: &str) {
        self.0.remove(key);
    }

    fn reconcile(&mut self, identity: &str, layout: &DiffLayout, collapsed: &mut HashSet<String>) {
        for section in &layout.sections {
            let Some(path) = section.file_path.as_ref() else {
                continue;
            };
            let Some(fingerprint) = section.fingerprint else {
                continue;
            };
            let key = format!("{identity}\0{path}");
            let changed = self
                .0
                .get(&key)
                .is_some_and(|viewed| *viewed != fingerprint);
            if changed {
                self.0.remove(&key);
                collapsed.remove(&key);
            }
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
    fingerprint: Option<DiffFingerprint>,
    placeholder: bool,
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

    #[test]
    fn virtualized_diff_keeps_the_full_scroll_height() {
        let mut section = DiffSection::new(Some("src/lib.rs".to_string()), None);
        for index in 0..500 {
            section.push(GitDiffRow::Line(index));
        }
        let layout = DiffLayout {
            sections: vec![section],
        };
        let geometry = DiffGeometry::new(&layout, "branch", &HashSet::new());
        let visible = layout.visible(
            "branch",
            &HashSet::new(),
            &geometry,
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
    fn scroll_offset_resolves_the_active_file() {
        let mut first = DiffSection::new(Some("src/first.rs".to_string()), None);
        first.push(GitDiffRow::Line(0));
        let mut second = DiffSection::new(Some("src/second.rs".to_string()), None);
        second.push(GitDiffRow::Line(1));
        let layout = DiffLayout {
            sections: vec![first, second],
        };
        let geometry = DiffGeometry::new(&layout, "branch", &HashSet::new());

        assert_eq!(
            geometry.file_at(&layout, 0.0).as_deref(),
            Some("src/first.rs")
        );
        assert_eq!(
            geometry
                .file_at(
                    &layout,
                    DIFF_FILE_HEADER_HEIGHT + DIFF_LINE_HEIGHT + DIFF_SECTION_GAP,
                )
                .as_deref(),
            Some("src/second.rs")
        );
    }

    #[test]
    fn changed_diff_invalidates_viewed_and_expands_the_file() {
        let initial = DiffFixture::new("before");
        let unchanged = DiffFixture::new("before");
        let changed = DiffFixture::new("after");
        let key = "branch\0src/lib.rs".to_string();
        let mut viewed = ViewedFiles::default();
        let mut collapsed = HashSet::from([key.clone()]);
        viewed.insert(key.clone(), initial.fingerprint());

        viewed.reconcile("branch", &unchanged.layout, &mut collapsed);
        assert!(viewed.contains(&key, unchanged.fingerprint()));
        assert!(collapsed.contains(&key));

        viewed.reconcile("branch", &changed.layout, &mut collapsed);
        assert!(!viewed.contains(&key, changed.fingerprint()));
        assert!(!collapsed.contains(&key));
    }

    #[test]
    fn a_window_past_the_diff_does_not_duplicate_the_spacer() {
        let mut section = DiffSection::new(Some("src/lib.rs".to_string()), None);
        section.push(GitDiffRow::Line(0));
        let layout = DiffLayout {
            sections: vec![section],
        };
        let geometry = DiffGeometry::new(&layout, "branch", &HashSet::new());
        let visible = layout.visible(
            "branch",
            &HashSet::new(),
            &geometry,
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

    struct DiffFixture {
        layout: DiffLayout,
    }

    impl DiffFixture {
        fn new(text: &str) -> Self {
            let lines = vec![
                DiffLine {
                    kind: DiffKind::Hunk,
                    old_no: None,
                    new_no: None,
                    hunk: None,
                    spans: vec![StyledSpan {
                        text: "src/lib.rs".to_string(),
                        light_fg: [0; 3],
                        dark_fg: [0; 3],
                        bold: false,
                        italic: false,
                    }],
                },
                DiffLine {
                    kind: DiffKind::Add,
                    old_no: None,
                    new_no: Some(1),
                    hunk: None,
                    spans: vec![StyledSpan {
                        text: text.to_string(),
                        light_fg: [0; 3],
                        dark_fg: [0; 3],
                        bold: false,
                        italic: false,
                    }],
                },
            ];
            Self {
                layout: DiffLayout::group(&lines, &[GitDiffRow::Line(0), GitDiffRow::Line(1)], &[]),
            }
        }

        fn fingerprint(&self) -> Option<DiffFingerprint> {
            self.layout.sections[0].fingerprint
        }
    }
}
