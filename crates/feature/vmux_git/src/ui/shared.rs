#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
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
use vmux_ui::scroll::{ScrollIntoView, ScrollPreload, Target};

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

fn hunk_end(lines: &[DiffLine], index: usize) -> Option<u32> {
    let hunk = lines.get(index)?.hunk?;
    (lines.get(index + 1).and_then(|line| line.hunk) != Some(hunk)).then_some(hunk)
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
    #[props(default)] reveal_path: Option<ReadSignal<Target>>,
    #[props(default)] onactivefile: Option<EventHandler<String>>,
    #[props(default)] onviewedchange: Option<EventHandler<usize>>,
    #[props(default)] expected_paths: Vec<String>,
    viewport: ReadSignal<Option<GitDiffViewport>>,
    display_rows: ReadSignal<Vec<GitDiffRow>>,
    rows_from: ReadSignal<Option<u32>>,
    loading: bool,
    visible: bool,
) -> Element {
    let mut collapsed_files = use_signal(HashSet::<String>::new);
    let mut viewed_files = use_signal(ViewedFiles::default);
    let mut scroll_container = use_signal(|| None::<Rc<MountedData>>);
    let mut render_window = use_signal(DiffRenderWindow::default);
    let mut revealed_file = use_signal(String::new);
    let mut active_file = use_signal(String::new);
    let next_request = use_hook(|| Rc::new(DiffNextRequest::default()));
    let derived_cache = use_hook(|| Rc::new(RefCell::new(DiffDerivedCache::default())));
    let layout_cache = use_hook(|| Rc::new(RefCell::new(DiffLayoutCache::default())));
    let geometry_cache = use_hook(|| Rc::new(RefCell::new(DiffGeometryCache::default())));
    let scroll_position = use_hook(|| Rc::new(Cell::new(0.0)));
    let geometry_window = use_hook(|| Rc::new(Cell::new(None::<DiffRenderWindow>)));
    let pending_anchor = use_hook(|| Rc::new(RefCell::new(None::<ScrollAnchor>)));
    let expected_path_count = expected_paths.len();
    let layout_expected_paths = expected_paths.clone();
    let derived_rows_from = rows_from;
    let derived_state_cache = Rc::clone(&derived_cache);
    let derived = use_memo(move || {
        let viewport = viewport.read();
        let mut cache = derived_state_cache.borrow_mut();
        cache.update(viewport.as_ref(), *derived_rows_from.read())
    });
    let layout_derived_cache = Rc::clone(&derived_cache);
    let layout_state_cache = Rc::clone(&layout_cache);
    let layout_identity = identity.clone();
    let layout_geometry_cache = Rc::clone(&geometry_cache);
    let layout_scroll_position = Rc::clone(&scroll_position);
    let layout_pending_anchor = Rc::clone(&pending_anchor);
    let layout_revision = use_memo(use_reactive!(|(layout_expected_paths, layout_identity)| {
        derived.read();
        let viewport = viewport.read();
        let rows = display_rows.read();
        let derived = layout_derived_cache.borrow();
        let anchor = {
            let layout = layout_state_cache.borrow();
            let geometry = layout_geometry_cache.borrow();
            if layout.has_viewport
                && viewport
                    .as_ref()
                    .is_some_and(|viewport| viewport.generation == layout.generation)
                && geometry.identity == *layout_identity
            {
                ScrollAnchor::capture(
                    &layout.layout,
                    &geometry.geometry,
                    layout_scroll_position.get(),
                )
            } else {
                None
            }
        };
        let mut layout = layout_state_cache.borrow_mut();
        let previous = layout.version;
        let revision = layout.update(
            viewport.as_ref(),
            &rows,
            &layout_expected_paths,
            &derived.fingerprints,
            *rows_from.read(),
            !path.read().is_empty(),
        );
        if revision.version != previous {
            *layout_pending_anchor.borrow_mut() = anchor;
        }
        revision
    }));
    let geometry_identity = identity.clone();
    let geometry_layout_cache = Rc::clone(&layout_cache);
    let geometry_state_cache = Rc::clone(&geometry_cache);
    let geometry_scroll_position = Rc::clone(&scroll_position);
    let geometry_pending_anchor = Rc::clone(&pending_anchor);
    let geometry_render_window = Rc::clone(&geometry_window);
    let geometry_revision = use_memo(use_reactive!(|(geometry_identity,)| {
        layout_revision.read();
        let layout = geometry_layout_cache.borrow();
        let mut geometry = geometry_state_cache.borrow_mut();
        let same_document = geometry.identity == *geometry_identity;
        let revision = geometry.update(&layout, &geometry_identity, &collapsed_files.read());
        if !same_document {
            geometry_render_window.set(None);
        }
        if let Some(anchor) = geometry_pending_anchor.borrow_mut().take()
            && same_document
            && let Some(top) = anchor.restore(&layout.layout, &geometry.geometry)
        {
            geometry_scroll_position.set(top);
            geometry_render_window
                .set(Some(DiffRenderWindow::at(top, render_window.peek().height)));
        }
        revision
    }));
    let reveal_identity = identity.clone();
    let reveal_layout_cache = Rc::clone(&layout_cache);
    let reveal_geometry_cache = Rc::clone(&geometry_cache);
    use_effect(use_reactive!(|(reveal_identity,)| {
        layout_revision.read();
        geometry_revision.read();
        let Some(reveal_path) = reveal_path else {
            return;
        };
        let request = reveal_path();
        let path = request.target;
        if path.is_empty() {
            return;
        }
        let key = format!("{reveal_identity}\0{}\0{path}", request.revision);
        if *revealed_file.peek() == key {
            return;
        }
        let layout = reveal_layout_cache.borrow();
        let geometry = reveal_geometry_cache.borrow();
        let Some(offset) = geometry.geometry.file_offset(&layout.layout, &path) else {
            return;
        };
        revealed_file.set(key);
        if request.behavior == ScrollBehavior::Smooth {
            ScrollIntoView::element_to_smooth("git-diff-viewport", offset);
        } else {
            let height = render_window.peek().height;
            render_window.set(DiffRenderWindow::at(offset, height));
            ScrollIntoView::element_to("git-diff-viewport", offset);
        }
    }));
    let viewed_identity = identity.clone();
    let viewed_layout_cache = Rc::clone(&layout_cache);
    use_effect(use_reactive!(|(viewed_identity,)| {
        layout_revision.read();
        let layout = viewed_layout_cache.borrow();
        let mut viewed = viewed_files.write();
        let mut collapsed = collapsed_files.write();
        viewed.reconcile(&viewed_identity, &layout.layout, &mut collapsed);
        if let Some(handler) = onviewedchange {
            handler.call(viewed.count(&viewed_identity, layout.layout.file_paths()));
        }
    }));
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
    let (generation, revision) = viewport_state
        .as_ref()
        .map(|viewport| (viewport.generation, viewport.revision))
        .unwrap_or_default();
    let next_request_key = viewport_state.as_ref().map(DiffNextRequestKey::from);
    let derived_snapshot = derived.read();
    let gw = derived_snapshot.gutter_width;
    let total_rows = u32::try_from(rows.len()).unwrap_or(u32::MAX);
    let collapsed_snapshot = collapsed_files();
    let viewed_snapshot = viewed_files();
    layout_revision.read();
    geometry_revision.read();
    let visible_layout = {
        let layout = layout_cache.borrow();
        let geometry = geometry_cache.borrow();
        layout.layout.visible(
            &identity,
            &collapsed_snapshot,
            &geometry.geometry,
            geometry_window.get().unwrap_or(*render_window.read()),
            true,
        )
    };
    let VisibleDiffLayout {
        top_spacer,
        bottom_spacer,
        sections,
    } = visible_layout;
    let initial_next_request = Rc::clone(&next_request);
    let initial_layout_cache = Rc::clone(&layout_cache);
    let initial_geometry_cache = Rc::clone(&geometry_cache);
    use_effect(move || {
        layout_revision.read();
        geometry_revision.read();
        let viewport = viewport.read();
        let has_more = viewport.as_ref().is_some_and(|viewport| viewport.has_more);
        let next_request_key = viewport.as_ref().map(DiffNextRequestKey::from);
        let row_count = viewport
            .as_ref()
            .map(|viewport| viewport.lines.len())
            .unwrap_or_default();
        if loading || !has_more || row_count == 0 {
            return;
        }
        let next_request = Rc::clone(&initial_next_request);
        let placeholder_within = {
            let layout = initial_layout_cache.borrow();
            let geometry = initial_geometry_cache.borrow();
            layout
                .layout
                .placeholder_within(&geometry.geometry, *render_window.peek())
        };
        if placeholder_within {
            next_request.send_key(next_request_key);
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
                next_request.send_key(next_request_key);
            }
        });
    });
    let scroll_next_request = Rc::clone(&next_request);
    let scroll_layout_cache = Rc::clone(&layout_cache);
    let scroll_geometry_cache = Rc::clone(&geometry_cache);
    let mounted_scroll_position = Rc::clone(&scroll_position);
    let mounted_geometry_window = Rc::clone(&geometry_window);
    let event_scroll_position = Rc::clone(&scroll_position);
    let event_geometry_window = Rc::clone(&geometry_window);

    let initial_loading = loading && viewport.read().is_none() && expected_path_count == 0;
    let (top_spacer, bottom_spacer, sections) = if initial_loading {
        (0.0, 0.0, Vec::new())
    } else {
        (top_spacer, bottom_spacer, sections)
    };
    rsx! {
        div {
            key: "{identity}",
            id: "git-diff-viewport",
            "data-vmux-preserve-scroll": "",
            onmounted: move |event: Event<MountedData>| {
                mounted_scroll_position.set(0.0);
                mounted_geometry_window.set(None);
                let element = event.data();
                scroll_container.set(Some(element.clone()));
                let position = Rc::clone(&mounted_scroll_position);
                spawn(async move {
                    let Ok(rect) = element.get_client_rect().await else {
                        return;
                    };
                    let top = element.get_scroll_offset().await.map(|offset| offset.y).unwrap_or_default();
                    position.set(top);
                    render_window.set(DiffRenderWindow::at(top, rect.size.height));
                });
            },
            class: if visible { "min-h-0 flex-1 overflow-auto bg-transparent font-mono text-[11px] leading-5" } else { "hidden" },
            style: "overflow-anchor:none;scrollbar-gutter:stable;",
            onscroll: move |event| {
                event_scroll_position.set(event.scroll_top());
                let had_geometry_window = event_geometry_window.replace(None).is_some();
                let viewport_window = DiffRenderWindow::from_scroll(
                    event.scroll_top(),
                    f64::from(event.client_height()),
                );
                let current_window = *render_window.peek();
                if had_geometry_window || !current_window.covers(
                    event.scroll_top(),
                    f64::from(event.client_height()),
                ) && current_window != viewport_window
                {
                    render_window.set(viewport_window);
                }
                let layout = scroll_layout_cache.borrow();
                let geometry = scroll_geometry_cache.borrow();
                let placeholder_nearby = layout
                    .layout
                    .placeholder_within(&geometry.geometry, viewport_window);
                if let Some(path) = geometry
                    .geometry
                    .file_at(&layout.layout, event.scroll_top() + 1.0)
                    && *active_file.peek() != path
                {
                    active_file.set(path.clone());
                    if let Some(handler) = onactivefile {
                        handler.call(path);
                    }
                }
                if (placeholder_nearby || ScrollPreload::DOCUMENT.reached(&event))
                    && has_more
                    && !loading
                {
                    scroll_next_request.send_key(next_request_key);
                }
            },

            if initial_loading {
                DiffLoadingSurface { height: render_window.read().height.max(360.0) }
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
                DiffVirtualSpacer { height: top_spacer }
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
                    let viewed_count_layout = Rc::clone(&layout_cache);
                    let viewed_reveal_path = file_path.clone().unwrap_or_default();
                    let collapse_reveal_path = viewed_reveal_path.clone();
                    let collapse_identity = identity.clone();
                    let collapse_layout = Rc::clone(&layout_cache);
                    let height_element = format!("git-diff-section-{}", crate::GitUrl::file_hash(viewed_reveal_path.as_bytes()));
                    let viewed_height_element = height_element.clone();
                    let collapse_height_element = height_element.clone();
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
                    let scroll_anchor_key = file_path.clone().unwrap_or_default();
                    let scroll_anchor_id = if file_path.is_some() { height_element } else { format!("git-diff-scroll-anchor-{}", section.index) };
                    rsx! {
                        section {
                            key: "section-{section.index}-{file_path:?}",
                            id: scroll_anchor_id,
                            class: section_class,
                            aria_busy: placeholder,
                            style: "overflow-anchor:none;",
                            "data-vmux-scroll-anchor": scroll_anchor_key,
                            if let Some(file_path) = file_path {
                                div {
                                    id: crate::GitUrl::file_hash(file_path.as_bytes()),
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
                                                    vmux_ui::animation::Height::transition(&viewed_height_element);
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
                                                            viewed_count_layout.borrow().layout.file_paths(),
                                                        ));
                                                    }
                                                    viewed_count_layout.borrow().layout.reveal(
                                                        &viewed_count_identity,
                                                        &collapsed,
                                                        &viewed_reveal_path,
                                                        render_window,
                                                    );
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
                                                vmux_ui::animation::Height::transition(&collapse_height_element);
                                                let mut files = collapsed_files.write();
                                                if !files.remove(key) {
                                                    files.insert(key.clone());
                                                }
                                                collapse_layout.borrow().layout.reveal(
                                                    &collapse_identity,
                                                    &files,
                                                    &collapse_reveal_path,
                                                    render_window,
                                                );
                                            },
                                        }
                                    }
                                }
                            }
                            div { "data-vmux-height-body": "",
                            if placeholder {
                                div { class: "flex h-36 flex-col px-3 py-3",
                                    for width in ["w-2/5", "w-3/5", "w-1/3", "w-1/2"] {
                                        div { class: "flex h-5 shrink-0 items-center",
                                            Skeleton { class: "h-1.5 {width} bg-foreground/[0.03]" }
                                        }
                                    }
                                }
                            } else if !collapsed {
                                if section.top_spacer > 0.0 {
                                    DiffVirtualRows { height: section.top_spacer }
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
                                                    if let Some(h) = hunk_end(rows, index).filter(|_| !path().is_empty()) {
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
                                            DiffGap {
                                                key: "gap-{start}-{end}",
                                                generation,
                                                revision,
                                                start,
                                                end,
                                                total: total_rows,
                                            }
                                        },
                                    }
                                }
                                if section.bottom_spacer > 0.0 {
                                    DiffVirtualRows { height: section.bottom_spacer }
                                }
                            }
                            }
                        }
                    }
                }
            }
            if bottom_spacer > 0.0 {
                DiffVirtualSpacer { height: bottom_spacer }
            }
        }
    }
}

#[component]
fn DiffVirtualSpacer(height: f64) -> Element {
    let preview_height = height.min(820.0);
    rsx! {
        div { class: "relative w-full min-w-0", style: "height:{height}px;overflow-anchor:none;",
            div {
                class: "pointer-events-none sticky top-0 flex shrink-0 flex-col gap-3 overflow-hidden py-3",
                style: "height:{preview_height}px;",
                for index in 0..5 {
                    div { key: "virtual-diff-{index}", class: "h-36 shrink-0 overflow-hidden rounded-lg bg-foreground/[0.028]",
                        div { class: "flex h-9 items-center gap-2 bg-foreground/[0.025] px-3",
                            div { class: "size-3.5 rounded-sm bg-foreground/[0.07]" }
                            div { class: "h-2.5 w-5/12 rounded bg-foreground/[0.07]" }
                        }
                        div { class: "flex flex-col px-3 py-3",
                            for width in ["w-2/5", "w-3/5", "w-1/3", "w-1/2"] {
                                div { class: "flex h-5 shrink-0 items-center",
                                    div { class: "h-1.5 {width} rounded bg-foreground/[0.03]" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn DiffLoadingSurface(height: f64) -> Element {
    let rows = ((height - DIFF_FILE_HEADER_HEIGHT).max(DIFF_LINE_HEIGHT) / DIFF_LINE_HEIGHT).ceil()
        as usize;
    let widths = ["w-2/5", "w-3/5", "w-1/3", "w-1/2"];
    rsx! {
        div {
            class: "min-h-full overflow-hidden",
            style: "height:{height}px;",
            "data-vmux-loading-surface": "",
            div { class: "flex h-9 items-center gap-2 bg-foreground/[0.025] px-3",
                Skeleton { class: "size-3.5 shrink-0 rounded-sm bg-foreground/[0.07]" }
                Skeleton { class: "h-2.5 w-32 bg-foreground/[0.07]" }
            }
            for index in 0..rows.min(14) {
                div { key: "initial-diff-{index}", class: "flex h-5 items-center gap-3 px-3",
                    if index % 4 != 3 {
                        div { class: "flex w-16 shrink-0 gap-2",
                            Skeleton { class: "h-1.5 w-5 bg-foreground/[0.025]" }
                            Skeleton { class: "h-1.5 w-7 bg-foreground/[0.02]" }
                        }
                        Skeleton { class: "h-1.5 {widths[index % widths.len()]} bg-foreground/[0.03]" }
                    }
                }
            }
        }
    }
}

#[component]
fn DiffVirtualRows(height: f64) -> Element {
    let preview_height = height.min(820.0);
    let rows = ((preview_height / DIFF_LINE_HEIGHT).ceil() as usize).clamp(1, 14);
    let widths = ["w-2/5", "w-3/5", "w-1/3", "w-1/2"];
    rsx! {
        div {
            class: "pointer-events-none relative w-full bg-foreground/[0.012]",
            style: "height:{height}px;overflow-anchor:none;",
            div {
                class: "sticky top-9 flex flex-col",
                style: "height:{preview_height}px;",
                for index in 0..rows {
                    div { key: "virtual-row-{index}", class: "flex h-5 shrink-0 items-center gap-3 px-3",
                        if index % 4 != 3 {
                            div { class: "flex w-16 shrink-0 gap-2",
                                div { class: "h-1.5 w-5 rounded-md bg-foreground/[0.025]" }
                                div { class: "h-1.5 w-7 rounded-md bg-foreground/[0.02]" }
                            }
                            div { class: "h-1.5 {widths[index % widths.len()]} rounded-md bg-foreground/[0.03]" }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct DiffNextRequestKey {
    generation: u64,
    revision: u64,
}

impl From<&GitDiffViewport> for DiffNextRequestKey {
    fn from(viewport: &GitDiffViewport) -> Self {
        Self {
            generation: viewport.generation,
            revision: viewport.revision,
        }
    }
}

#[derive(Default)]
struct DiffNextRequest(Cell<Option<DiffNextRequestKey>>);

impl DiffNextRequest {
    fn send_key(&self, key: Option<DiffNextRequestKey>) {
        let Some(key) = key else {
            return;
        };
        if !self.claim(key) {
            return;
        }
        if send(&GitDiffNextRequest {
            generation: key.generation,
            revision: key.revision,
        })
        .is_err()
        {
            self.0.set(None);
        }
    }

    fn claim(&self, key: DiffNextRequestKey) -> bool {
        if self.0.get() == Some(key) {
            return false;
        }
        self.0.set(Some(key));
        true
    }
}

const DIFF_LINE_HEIGHT: f64 = 20.0;
const DIFF_GAP_HEIGHT: f64 = 34.0;
const DIFF_FILE_HEADER_HEIGHT: f64 = 36.0;
const DIFF_SECTION_GAP: f64 = 12.0;
const DIFF_PLACEHOLDER_HEIGHT: f64 = 180.0;
const DIFF_RENDER_OVERSCAN: f64 = 4800.0;
const DIFF_RENDER_GUARD: f64 = 1600.0;
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
            height: height.max(3200.0),
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

    fn covers(self, top: f64, height: f64) -> bool {
        let (start, end) = self.range();
        let guarded_start = if start == 0.0 {
            0.0
        } else {
            start + DIFF_RENDER_GUARD
        };
        let guarded_end = (end - DIFF_RENDER_GUARD).max(guarded_start);
        top >= guarded_start && top + height <= guarded_end
    }

    fn clamped(self, total_height: f64) -> Self {
        Self {
            top: self.top.min((total_height - self.height).max(0.0)),
            height: self.height,
        }
    }
}

#[derive(Clone, Default, PartialEq)]
struct DiffLayout {
    sections: Vec<DiffSection>,
}

impl DiffLayout {
    fn reveal(
        &self,
        identity: &str,
        collapsed: &HashSet<String>,
        path: &str,
        mut window: Signal<DiffRenderWindow>,
    ) {
        let geometry = DiffGeometry::new(self, identity, collapsed);
        let Some(top) = geometry.file_offset(self, path) else {
            return;
        };
        let height = window.peek().height;
        let next = DiffRenderWindow::at(top, height);
        if *window.peek() != next {
            window.set(next);
        }
    }

    fn group(
        lines: &[DiffLine],
        rows: &[GitDiffRow],
        expected_paths: &[String],
        fingerprints: &DiffFingerprints,
        hunk_controls: bool,
    ) -> Self {
        let sections = Self::group_rows(lines, rows, fingerprints, hunk_controls);
        Self::ordered(sections, expected_paths)
    }

    fn group_rows(
        lines: &[DiffLine],
        rows: &[GitDiffRow],
        fingerprints: &DiffFingerprints,
        hunk_controls: bool,
    ) -> Vec<DiffSection> {
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
                let mut section = DiffSection::new(Some(file_path), fingerprint);
                section.source_start = row.start();
                section.source_end = row.end();
                sections.push(section);
                continue;
            }
            if sections.is_empty() {
                let mut section = DiffSection::new(None, None);
                section.source_start = row.start();
                sections.push(section);
            }
            if let Some(section) = sections.last_mut() {
                let hunk_control = match row {
                    GitDiffRow::Line(index) => {
                        hunk_controls && hunk_end(lines, *index as usize).is_some()
                    }
                    GitDiffRow::Gap { .. } => false,
                };
                section.push(*row, hunk_control);
            }
        }
        sections
    }

    fn ordered(sections: Vec<DiffSection>, expected_paths: &[String]) -> Self {
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

    fn rebuild_from(
        &mut self,
        lines: &[DiffLine],
        rows: &[GitDiffRow],
        expected_paths: &[String],
        fingerprints: &DiffFingerprints,
        rows_from: u32,
        hunk_controls: bool,
    ) -> usize {
        let mut affected = self
            .sections
            .iter()
            .enumerate()
            .filter(|(_, section)| !section.placeholder && section.source_end > rows_from)
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        if affected.is_empty()
            && let Some((index, _)) = self
                .sections
                .iter()
                .enumerate()
                .filter(|(_, section)| !section.placeholder && section.source_start <= rows_from)
                .max_by_key(|(_, section)| section.source_start)
        {
            affected.push(index);
        }
        let rebuild_source = affected
            .iter()
            .filter_map(|index| self.sections.get(*index))
            .map(|section| section.source_start)
            .min()
            .unwrap_or(rows_from);
        let expected = expected_paths.iter().cloned().collect::<HashSet<_>>();
        let mut changed_from = self.sections.len();
        affected.sort_unstable();
        for index in affected.into_iter().rev() {
            changed_from = changed_from.min(index);
            let path = self.sections[index].file_path.clone();
            if path.as_ref().is_some_and(|path| expected.contains(path)) {
                self.sections[index] = DiffSection::placeholder(path.unwrap());
            } else {
                self.sections.remove(index);
            }
        }
        let first = rows.partition_point(|row| row.end() <= rebuild_source);
        let tail = Self::group_rows(lines, &rows[first..], fingerprints, hunk_controls);
        for section in tail {
            if let Some(path) = section.file_path.as_ref() {
                let index = self
                    .sections
                    .iter()
                    .position(|current| current.file_path.as_ref() == Some(path));
                match index {
                    Some(index) => {
                        changed_from = changed_from.min(index);
                        self.sections[index] = section;
                    }
                    None => {
                        changed_from = changed_from.min(self.sections.len());
                        self.sections.push(section);
                    }
                }
            } else if let Some(index) = self
                .sections
                .iter()
                .position(|current| current.file_path.is_none())
            {
                changed_from = changed_from.min(index);
                self.sections[index] = section;
            } else {
                changed_from = 0;
                self.sections.insert(0, section);
            }
        }
        changed_from
    }

    fn file_paths(&self) -> impl Iterator<Item = &str> {
        self.sections
            .iter()
            .filter(|section| !section.placeholder)
            .filter_map(|section| section.file_path.as_deref())
    }

    fn placeholder_within(&self, geometry: &DiffGeometry, window: DiffRenderWindow) -> bool {
        let (range_start, range_end) = window.range();
        let first = geometry
            .ends
            .partition_point(|section_end| *section_end < range_start);
        for (index, section) in self.sections.iter().enumerate().skip(first) {
            if geometry.starts[index] > range_end {
                return false;
            }
            if section.placeholder {
                return true;
            }
        }
        false
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
            window.clamped(geometry.total_height).range()
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

#[derive(Clone, Default, PartialEq)]
struct DiffGeometry {
    starts: Vec<f64>,
    ends: Vec<f64>,
    total_height: f64,
}

impl DiffGeometry {
    fn new(layout: &DiffLayout, identity: &str, collapsed: &HashSet<String>) -> Self {
        let mut geometry = Self::default();
        geometry.rebuild_from(layout, identity, collapsed, 0);
        geometry
    }

    fn rebuild_from(
        &mut self,
        layout: &DiffLayout,
        identity: &str,
        collapsed: &HashSet<String>,
        from: usize,
    ) {
        let from = from.min(layout.sections.len());
        self.starts.truncate(from);
        self.ends.truncate(from);
        let mut offset = self.ends.last().copied().unwrap_or_default();
        self.starts
            .reserve(layout.sections.len().saturating_sub(from));
        self.ends
            .reserve(layout.sections.len().saturating_sub(from));
        for (index, section) in layout.sections.iter().enumerate().skip(from) {
            self.starts.push(offset);
            offset += section.height(index, identity, collapsed);
            self.ends.push(offset);
        }
        self.total_height = offset;
    }

    fn file_offset(&self, layout: &DiffLayout, path: &str) -> Option<f64> {
        layout
            .sections
            .iter()
            .position(|section| section.file_path.as_deref() == Some(path))
            .map(|index| self.starts[index] + layout.sections[index].margin(index))
    }

    fn file_at(&self, layout: &DiffLayout, offset: f64) -> Option<String> {
        if layout.sections.is_empty() {
            return None;
        }
        let index = self.ends.partition_point(|end| *end <= offset);
        let index = index.min(layout.sections.len().saturating_sub(1));
        layout.sections[..=index]
            .iter()
            .rev()
            .find_map(|section| section.file_path.clone())
    }
}

struct ScrollAnchor {
    path: String,
    offset: f64,
}

impl ScrollAnchor {
    fn capture(layout: &DiffLayout, geometry: &DiffGeometry, top: f64) -> Option<Self> {
        let path = geometry.file_at(layout, top)?;
        let offset = top - geometry.file_offset(layout, &path)?;
        Some(Self { path, offset })
    }

    fn restore(&self, layout: &DiffLayout, geometry: &DiffGeometry) -> Option<f64> {
        Some((geometry.file_offset(layout, &self.path)? + self.offset).max(0.0))
    }
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct DiffLayoutRevision {
    version: u64,
    changed_from: usize,
}

#[derive(Default)]
struct DiffLayoutCache {
    has_viewport: bool,
    generation: u64,
    revision: u64,
    first_line: u32,
    expected_paths: Vec<String>,
    hunk_controls: bool,
    layout: DiffLayout,
    version: u64,
    changed_from: usize,
}

impl DiffLayoutCache {
    fn update(
        &mut self,
        viewport: Option<&GitDiffViewport>,
        rows: &[GitDiffRow],
        expected_paths: &[String],
        fingerprints: &DiffFingerprints,
        rows_from: Option<u32>,
        hunk_controls: bool,
    ) -> DiffLayoutRevision {
        let Some(viewport) = viewport else {
            if !self.has_viewport
                && self.expected_paths == expected_paths
                && self.hunk_controls == hunk_controls
            {
                return self.revision();
            }
            let pending = DiffLayout::group(&[], &[], expected_paths, fingerprints, hunk_controls);
            if self.layout.sections != pending.sections {
                self.layout = pending;
                self.changed_from = 0;
                self.version = self.version.wrapping_add(1).max(1);
            }
            self.hunk_controls = hunk_controls;
            self.generation = 0;
            self.has_viewport = false;
            self.revision = 0;
            self.first_line = 0;
            self.expected_paths.clear();
            self.expected_paths.extend_from_slice(expected_paths);
            return self.revision();
        };
        if self.has_viewport
            && self.generation == viewport.generation
            && self.revision == viewport.revision
            && self.first_line == viewport.first_line
            && self.expected_paths == expected_paths
            && self.hunk_controls == hunk_controls
            && rows_from.is_some()
        {
            return self.revision();
        }
        let append = self.has_viewport
            && self.generation == viewport.generation
            && self.first_line == viewport.first_line
            && viewport.revision > self.revision
            && self.expected_paths == expected_paths
            && self.hunk_controls == hunk_controls
            && rows_from.is_some();
        self.changed_from = if append {
            self.layout.rebuild_from(
                &viewport.lines,
                rows,
                expected_paths,
                fingerprints,
                rows_from.unwrap(),
                hunk_controls,
            )
        } else {
            self.layout = DiffLayout::group(
                &viewport.lines,
                rows,
                expected_paths,
                fingerprints,
                hunk_controls,
            );
            0
        };
        self.hunk_controls = hunk_controls;
        self.generation = viewport.generation;
        self.has_viewport = true;
        self.revision = viewport.revision;
        self.first_line = viewport.first_line;
        self.expected_paths.clear();
        self.expected_paths.extend_from_slice(expected_paths);
        self.version = self.version.wrapping_add(1).max(1);
        self.revision()
    }

    fn revision(&self) -> DiffLayoutRevision {
        DiffLayoutRevision {
            version: self.version,
            changed_from: self.changed_from,
        }
    }
}

#[derive(Default)]
struct DiffGeometryCache {
    layout_version: u64,
    identity: String,
    collapsed: HashSet<String>,
    geometry: DiffGeometry,
    version: u64,
}

impl DiffGeometryCache {
    fn update(
        &mut self,
        layout: &DiffLayoutCache,
        identity: &str,
        collapsed: &HashSet<String>,
    ) -> u64 {
        if self.layout_version == layout.version
            && self.identity == identity
            && self.collapsed == *collapsed
        {
            return self.version;
        }
        let rebuild_all = self.identity != identity || self.collapsed != *collapsed;
        let from = if rebuild_all { 0 } else { layout.changed_from };
        self.geometry
            .rebuild_from(&layout.layout, identity, collapsed, from);
        self.layout_version = layout.version;
        self.identity.clear();
        self.identity.push_str(identity);
        self.collapsed.clone_from(collapsed);
        self.version = self.version.wrapping_add(1).max(1);
        self.version
    }
}

#[derive(Clone, PartialEq)]
struct DiffSection {
    file_path: Option<String>,
    fingerprint: Option<DiffFingerprint>,
    placeholder: bool,
    rows: Vec<GitDiffRow>,
    row_offsets: Vec<f64>,
    source_start: u32,
    source_end: u32,
}

impl DiffSection {
    fn new(file_path: Option<String>, fingerprint: Option<DiffFingerprint>) -> Self {
        Self {
            file_path,
            fingerprint,
            placeholder: false,
            rows: Vec::new(),
            row_offsets: vec![0.0],
            source_start: 0,
            source_end: 0,
        }
    }

    fn placeholder(file_path: String) -> Self {
        Self {
            file_path: Some(file_path),
            fingerprint: None,
            placeholder: true,
            rows: Vec::new(),
            row_offsets: vec![0.0],
            source_start: 0,
            source_end: 0,
        }
    }

    fn push(&mut self, row: GitDiffRow, hunk_control: bool) {
        let offset = self.row_offsets.last().copied().unwrap_or_default();
        self.rows.push(row);
        self.row_offsets
            .push(offset + DiffRowHeight::of(row, hunk_control));
        self.source_end = self.source_end.max(row.end());
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
        let collapsed = self.collapsed(identity, collapsed);
        let content_start = section_start
            + self.margin(index)
            + self
                .file_path
                .as_ref()
                .map(|_| DIFF_FILE_HEADER_HEIGHT)
                .unwrap_or_default();
        let local_start = (range_start - content_start).max(0.0);
        let local_end = (range_end - content_start).max(0.0);
        let first = if collapsed {
            0
        } else {
            self.row_offsets[1..].partition_point(|end| *end < local_start)
        };
        let end = if collapsed {
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

#[derive(Clone, Default, PartialEq)]
struct DiffFingerprints(HashMap<String, DiffFingerprint>);

impl DiffFingerprints {
    #[cfg(test)]
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

#[derive(Clone, Default, PartialEq)]
struct DiffDerived {
    gutter_width: usize,
}

#[derive(Default)]
struct DiffDerivedCache {
    generation: u64,
    revision: u64,
    first_line: u32,
    processed: usize,
    fingerprints: DiffFingerprints,
    current_fingerprint: Option<(String, DefaultHasher)>,
    max_line: u32,
}

impl DiffDerivedCache {
    fn update(
        &mut self,
        viewport: Option<&GitDiffViewport>,
        rows_from: Option<u32>,
    ) -> DiffDerived {
        let Some(viewport) = viewport else {
            *self = Self::default();
            return DiffDerived { gutter_width: 3 };
        };
        let lines = &viewport.lines;
        let replace = self.generation != viewport.generation
            || self.first_line != viewport.first_line
            || self.processed > lines.len()
            || (self.revision != viewport.revision && rows_from.is_none());
        if replace {
            *self = Self {
                generation: viewport.generation,
                revision: viewport.revision,
                first_line: viewport.first_line,
                ..Default::default()
            };
        }
        if self.processed < lines.len() {
            for line in &lines[self.processed..] {
                if let Some(path) = line.file_path() {
                    self.finish_fingerprint();
                    self.current_fingerprint = Some((path.to_string(), DefaultHasher::new()));
                }
                if let Some((_, hasher)) = self.current_fingerprint.as_mut() {
                    DiffFingerprint::write(line, hasher);
                }
                self.max_line = self
                    .max_line
                    .max(line.old_no.unwrap_or_default())
                    .max(line.new_no.unwrap_or_default());
            }
            self.processed = lines.len();
        }
        self.revision = viewport.revision;
        if let Some((path, hasher)) = self.current_fingerprint.as_ref() {
            self.fingerprints
                .0
                .insert(path.clone(), DiffFingerprint(hasher.clone().finish()));
        }
        DiffDerived {
            gutter_width: self.max_line.max(1).to_string().len().max(3),
        }
    }

    fn finish_fingerprint(&mut self) {
        if let Some((path, hasher)) = self.current_fingerprint.take() {
            self.fingerprints
                .0
                .insert(path, DiffFingerprint(hasher.finish()));
        }
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
    fn of(row: GitDiffRow, hunk_control: bool) -> f64 {
        match row {
            GitDiffRow::Line(_) => DIFF_LINE_HEIGHT + if hunk_control { 36.0 } else { 0.0 },
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
fn DiffGap(generation: u64, revision: u64, start: u32, end: u32, total: u32) -> Element {
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
                            let _ = send(&GitDiffRevealRequest {
                                generation,
                                revision,
                                start,
                                end,
                            });
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
                        let _ = send(&GitDiffRevealRequest {
                            generation,
                            revision,
                            start,
                            end,
                        });
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
                            let _ = send(&GitDiffRevealRequest {
                                generation,
                                revision,
                                start,
                                end,
                            });
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
    use dioxus::core::{AttributeValue, Mutation, Mutations, VirtualDom};
    use dioxus::html::{EventData, SerializedMouseData, SerializedScrollData};
    use vmux_ui::hooks::EventListenerError;
    use vmux_ui::transport::{BytesListener, HostScope, PageHost};

    #[derive(Default)]
    struct ScrollRequests(
        RefCell<Vec<(String, f64)>>,
        RefCell<Vec<(String, f64)>>,
        RefCell<Vec<String>>,
    );

    #[derive(Clone, Default)]
    struct RevealProbe(Rc<RefCell<Option<Signal<Target>>>>);

    impl PageHost for ScrollRequests {
        fn send(&self, _: &str, _: &[u8]) -> Result<(), EventListenerError> {
            Ok(())
        }

        fn listen(&self, _: &str, _: BytesListener) -> Result<(), EventListenerError> {
            Ok(())
        }

        fn scroll_element_to(&self, element: &str, top: f64) {
            self.0.borrow_mut().push((element.to_string(), top));
        }

        fn scroll_element_to_smooth(&self, element: &str, top: f64) {
            self.1.borrow_mut().push((element.to_string(), top));
        }

        fn animate_height(&self, element: &str, _: u32) {
            self.2.borrow_mut().push(element.to_string());
        }
    }

    #[test]
    fn editable_hunk_controls_contribute_height_but_read_only_diffs_do_not() {
        let mut lines = Vec::new();
        for (hunk, number) in [(1, 1), (1, 2), (2, 10), (2, 11)] {
            lines.push(DiffLine {
                kind: DiffKind::Add,
                old_no: None,
                new_no: Some(number),
                hunk: Some(hunk),
                spans: Vec::new(),
            });
        }
        let rows = (0..4).map(GitDiffRow::Line).collect::<Vec<_>>();
        let fingerprints = DiffFingerprints::default();
        let read_only = DiffLayout::group(&lines, &rows, &[], &fingerprints, false);
        let editable = DiffLayout::group(&lines, &rows, &[], &fingerprints, true);
        let read_geometry = DiffGeometry::new(&read_only, "read", &HashSet::new());
        let edit_geometry = DiffGeometry::new(&editable, "edit", &HashSet::new());
        assert_eq!(
            edit_geometry.total_height - read_geometry.total_height,
            72.0
        );
        assert_eq!(
            editable.sections[0].row_offsets.as_slice(),
            [0.0, 20.0, 76.0, 96.0, 152.0]
        );
    }

    struct DiffScrollFixture;

    impl DiffScrollFixture {
        fn folding_viewport() -> Option<GitDiffViewport> {
            let mut viewport = Self::viewport().unwrap();
            for (index, path) in [(0, "first.rs"), (1_001, "second.rs")] {
                viewport.lines.insert(
                    index,
                    DiffLine {
                        kind: DiffKind::Hunk,
                        old_no: None,
                        new_no: None,
                        hunk: None,
                        spans: vec![StyledSpan {
                            text: path.to_string(),
                            light_fg: [0; 3],
                            dark_fg: [0; 3],
                            bold: false,
                            italic: false,
                        }],
                    },
                );
            }
            viewport.total_lines = 2_002;
            Some(viewport)
        }

        fn folding_rows() -> Vec<GitDiffRow> {
            let mut rows = Vec::new();
            for number in 0..2_002 {
                rows.push(GitDiffRow::Line(number));
            }
            rows
        }
        fn viewport() -> Option<GitDiffViewport> {
            let mut lines = Vec::new();
            for number in 0..2_000 {
                lines.push(DiffLine {
                    kind: DiffKind::Add,
                    old_no: None,
                    new_no: Some(number + 1),
                    hunk: None,
                    spans: vec![StyledSpan {
                        text: format!("source-row-{number}"),
                        light_fg: [0; 3],
                        dark_fg: [0; 3],
                        bold: false,
                        italic: false,
                    }],
                });
            }
            Some(GitDiffViewport {
                generation: 1,
                revision: 1,
                first_line: 0,
                total_lines: 2_000,
                has_more: false,
                lines,
                markers: Vec::new(),
                error: String::new(),
            })
        }

        fn rows() -> Vec<GitDiffRow> {
            let mut rows = Vec::new();
            for number in 0..2_000 {
                rows.push(GitDiffRow::Line(number));
            }
            rows
        }

        fn contains_text(mutations: &Mutations, text: &str) -> bool {
            for mutation in &mutations.edits {
                if let Mutation::CreateTextNode { value, .. } = mutation
                    && value == text
                {
                    return true;
                }
            }
            false
        }
    }

    #[component]
    fn ScrollingDiff() -> Element {
        let repo_root = use_signal(String::new);
        let path = use_signal(String::new);
        let viewport = use_signal(DiffScrollFixture::viewport);
        let display_rows = use_signal(DiffScrollFixture::rows);
        let rows_from = use_signal(|| Some(0));
        rsx! {
            DiffView {
                repo_root,
                path,
                viewport,
                display_rows,
                rows_from,
                loading: false,
                visible: true,
            }
        }
    }

    #[component]
    fn InitialDiffFiles(probe: Rc<RefCell<Option<Signal<Option<GitDiffViewport>>>>>) -> Element {
        let repo_root = use_signal(String::new);
        let path = use_signal(String::new);
        let viewport = use_signal(|| None::<GitDiffViewport>);
        *probe.borrow_mut() = Some(viewport);
        let display_rows = use_memo(move || {
            let viewport = viewport.read();
            let mut rows = Vec::new();
            if let Some(viewport) = viewport.as_ref() {
                for index in 0..viewport.lines.len() {
                    rows.push(GitDiffRow::Line(index as u32));
                }
            }
            rows
        });
        let rows_from = use_signal(|| None);
        let loading = viewport.read().is_none();
        rsx! { DiffView {
            repo_root, path, viewport, display_rows, rows_from, loading,
            expected_paths: vec!["first.rs".to_string(), "second.rs".to_string()],
            visible: true,
        } }
    }

    #[test]
    fn known_files_render_pending_bodies_before_the_first_diff_viewport() {
        let requests = Rc::new(ScrollRequests::default());
        let _host = HostScope::enter(requests);
        let probe = Rc::new(RefCell::new(None));
        let mut dom = VirtualDom::new_with_props(
            InitialDiffFiles,
            InitialDiffFilesProps {
                probe: Rc::clone(&probe),
            },
        );
        let initial = dom.rebuild_to_vec();
        for path in ["first.rs", "second.rs"] {
            assert!(DiffScrollFixture::contains_text(&initial, path));
        }
        let pending = initial
            .edits
            .iter()
            .filter(|edit| {
                matches!(
                    edit,
                    Mutation::SetAttribute {
                        name: "aria-busy",
                        value: AttributeValue::Bool(true),
                        ..
                    }
                )
            })
            .count();
        assert_eq!(pending, 2);
        dom.render_immediate_to_vec();
        let mut viewport = probe.borrow().unwrap();
        dom.runtime().in_scope(ScopeId::ROOT, || {
            viewport.set(Some(GitDiffViewport {
                generation: 1,
                revision: 1,
                first_line: 0,
                total_lines: 2,
                has_more: true,
                lines: vec![
                    DiffLine {
                        kind: DiffKind::Hunk,
                        old_no: None,
                        new_no: None,
                        hunk: None,
                        spans: vec![StyledSpan {
                            text: "first.rs".to_string(),
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
                        hunk: Some(1),
                        spans: vec![StyledSpan {
                            text: "first file content".to_string(),
                            light_fg: [0; 3],
                            dark_fg: [0; 3],
                            bold: false,
                            italic: false,
                        }],
                    },
                ],
                markers: Vec::new(),
                error: String::new(),
            }));
        });
        let loaded = dom.render_immediate_to_vec();
        assert!(DiffScrollFixture::contains_text(
            &loaded,
            "first file content"
        ));
        assert!(loaded.edits.iter().any(|edit| matches!(
            edit,
            Mutation::SetAttribute {
                name: "aria-busy",
                value: AttributeValue::Bool(false),
                ..
            }
        )));
    }

    #[component]
    fn FoldingDiff() -> Element {
        let repo_root = use_signal(String::new);
        let path = use_signal(String::new);
        let viewport = use_signal(DiffScrollFixture::folding_viewport);
        let display_rows = use_signal(DiffScrollFixture::folding_rows);
        let rows_from = use_signal(|| Some(0));
        let reveal_path = use_signal(Target::default);
        if let Some(probe) = try_consume_context::<RevealProbe>() {
            *probe.0.borrow_mut() = Some(reveal_path);
        }
        rsx! { DiffView {
            repo_root, path, viewport, display_rows, rows_from,
            reveal_path: Some(reveal_path.into()),
            identity: "review",
            loading: false,
            visible: true,
        } }
    }

    #[test]
    fn file_selection_scrolls_smoothly_and_can_select_the_same_file_again() {
        dioxus::html::set_event_converter(Box::new(dioxus::html::SerializedHtmlEventConverter));
        let requests = Rc::new(ScrollRequests::default());
        let _host = HostScope::enter(requests.clone());
        let probe = RevealProbe::default();
        let mut dom = VirtualDom::new(FoldingDiff);
        dom.provide_root_context(probe.clone());
        let initial = dom.rebuild_to_vec();
        let scroll_node = initial
            .edits
            .iter()
            .find_map(|edit| match edit {
                Mutation::NewEventListener { name, id } if name == "scroll" => Some(*id),
                _ => None,
            })
            .unwrap();
        dom.render_immediate_to_vec();
        let mut reveal = probe.0.borrow().unwrap();
        dom.runtime().in_scope(ScopeId::ROOT, || {
            reveal
                .write()
                .request("second.rs".to_string(), ScrollBehavior::Smooth)
        });
        let selected = dom.render_immediate_to_vec();
        assert!(requests.0.borrow().is_empty());
        assert_eq!(
            *requests.1.borrow(),
            vec![("git-diff-viewport".to_string(), 20_048.0)]
        );
        assert!(!DiffScrollFixture::contains_text(
            &selected,
            "source-row-1000"
        ));

        let data = EventData::Scroll(SerializedScrollData {
            scroll_top: 20_048.0,
            scroll_left: 0.0,
            scroll_width: 1_000,
            scroll_height: 40_084,
            client_width: 1_000,
            client_height: 800,
        });
        dom.runtime()
            .handle_event("scroll", Event::new(data.into_any(), false), scroll_node);
        dom.render_immediate_to_vec();
        dom.mark_all_dirty();
        dom.render_immediate_to_vec();
        assert_eq!(requests.1.borrow().len(), 1);
        dom.runtime().in_scope(ScopeId::ROOT, || {
            reveal
                .write()
                .request("second.rs".to_string(), ScrollBehavior::Smooth)
        });
        dom.render_immediate_to_vec();
        assert_eq!(requests.1.borrow().len(), 2);
    }

    #[test]
    fn folding_and_marking_viewed_retain_the_header_without_an_unconditional_scroll() {
        dioxus::html::set_event_converter(Box::new(dioxus::html::SerializedHtmlEventConverter));
        for label in ["git-viewed", "common-collapse"] {
            let requests = Rc::new(ScrollRequests::default());
            let _host = HostScope::enter(requests.clone());
            let mut dom = VirtualDom::new(FoldingDiff);
            let initial = dom.rebuild_to_vec();
            let scroll_node = initial
                .edits
                .iter()
                .find_map(|edit| match edit {
                    Mutation::NewEventListener { name, id } if name == "scroll" => Some(*id),
                    _ => None,
                })
                .unwrap();
            dom.render_immediate_to_vec();
            let data = EventData::Scroll(SerializedScrollData {
                scroll_top: 30_000.0,
                scroll_left: 0.0,
                scroll_width: 1_000,
                scroll_height: 40_084,
                client_width: 1_000,
                client_height: 800,
            });
            dom.runtime()
                .handle_event("scroll", Event::new(data.into_any(), false), scroll_node);
            let scrolled = dom.render_immediate_to_vec();
            let label = translate(label);
            let button = scrolled
                .edits
                .iter()
                .find_map(|edit| match edit {
                    Mutation::SetAttribute {
                        name: "aria-label",
                        value: AttributeValue::Text(value),
                        id,
                        ..
                    } if value == &label => Some(*id),
                    _ => None,
                })
                .expect("the visible file header must expose its control");
            requests.0.borrow_mut().clear();
            let data = EventData::Mouse(SerializedMouseData::default());
            dom.runtime()
                .handle_event("click", Event::new(data.into_any(), true), button);
            dom.render_immediate_to_vec();

            assert!(requests.0.borrow().is_empty());
            assert_eq!(
                *requests.2.borrow(),
                vec![format!(
                    "git-diff-section-{}",
                    crate::GitUrl::file_hash(b"second.rs")
                )]
            );
        }
    }

    #[test]
    fn scrolling_loaded_diff_renders_rows_without_another_host_update() {
        dioxus::html::set_event_converter(Box::new(dioxus::html::SerializedHtmlEventConverter));
        let mut dom = VirtualDom::new(ScrollingDiff);
        let initial = dom.rebuild_to_vec();
        let scroll_node = initial
            .edits
            .iter()
            .find_map(|mutation| match mutation {
                Mutation::NewEventListener { name, id } if name == "scroll" => Some(*id),
                _ => None,
            })
            .expect("the diff viewport must handle scroll events");
        dom.render_immediate_to_vec();
        assert!(!DiffScrollFixture::contains_text(
            &initial,
            "source-row-1000"
        ));

        let data = EventData::Scroll(SerializedScrollData {
            scroll_top: 20_000.0,
            scroll_left: 0.0,
            scroll_width: 1_000,
            scroll_height: 40_000,
            client_width: 1_000,
            client_height: 800,
        });
        dom.runtime()
            .handle_event("scroll", Event::new(data.into_any(), false), scroll_node);
        let scrolled = dom.render_immediate_to_vec();

        assert!(DiffScrollFixture::contains_text(
            &scrolled,
            "source-row-1000"
        ));
    }

    #[test]
    fn growing_an_earlier_file_keeps_the_current_file_in_the_rendered_window() {
        dioxus::html::set_event_converter(Box::new(dioxus::html::SerializedHtmlEventConverter));
        let requests = Rc::new(ScrollRequests::default());
        let _host = HostScope::enter(requests);
        let probe = Rc::new(RefCell::new(None));
        let mut dom = VirtualDom::new_with_props(
            InitialDiffFiles,
            InitialDiffFilesProps {
                probe: Rc::clone(&probe),
            },
        );
        let initial = dom.rebuild_to_vec();
        let scroll_node = initial
            .edits
            .iter()
            .find_map(|edit| match edit {
                Mutation::NewEventListener { name, id } if name == "scroll" => Some(*id),
                _ => None,
            })
            .unwrap();
        let mut viewport = probe.borrow().unwrap();
        dom.runtime().in_scope(ScopeId::ROOT, || {
            viewport.set(DiffScrollFixture::folding_viewport())
        });
        dom.render_immediate_to_vec();
        let data = EventData::Scroll(SerializedScrollData {
            scroll_top: 20_500.0,
            scroll_left: 0.0,
            scroll_width: 1_000,
            scroll_height: 40_084,
            client_width: 1_000,
            client_height: 800,
        });
        dom.runtime()
            .handle_event("scroll", Event::new(data.into_any(), false), scroll_node);
        dom.render_immediate_to_vec();
        assert!(dioxus_ssr::render(&dom).contains("source-row-1000"));

        let mut grown = DiffScrollFixture::folding_viewport().unwrap();
        let extra = grown.lines[1..501].to_vec();
        grown.lines.splice(1..1, extra);
        grown.revision = 2;
        grown.total_lines += 500;
        dom.runtime()
            .in_scope(ScopeId::ROOT, || viewport.set(Some(grown)));
        dom.render_immediate_to_vec();

        assert!(dioxus_ssr::render(&dom).contains("source-row-1000"));
    }

    #[test]
    fn next_diff_request_is_claimed_once_per_loaded_window() {
        let request = DiffNextRequest::default();
        let first = DiffNextRequestKey {
            generation: 4,
            revision: 1,
        };
        let appended = DiffNextRequestKey {
            generation: 4,
            revision: 2,
        };

        assert!(request.claim(first));
        assert!(!request.claim(first));
        assert!(request.claim(appended));
    }

    #[test]
    fn diff_prefetches_when_a_placeholder_approaches_the_render_window() {
        let mut loaded = DiffSection::new(Some("src/loaded.rs".to_string()), None);
        for index in 0..300 {
            loaded.push(GitDiffRow::Line(index), false);
        }
        let layout = DiffLayout {
            sections: vec![
                loaded,
                DiffSection::placeholder("src/pending.rs".to_string()),
            ],
        };
        let geometry = DiffGeometry::new(&layout, "branch", &HashSet::new());

        assert!(!layout.placeholder_within(
            &geometry,
            DiffRenderWindow {
                top: 0.0,
                height: 800.0,
            },
        ));
        assert!(layout.placeholder_within(
            &geometry,
            DiffRenderWindow {
                top: 4_000.0,
                height: 800.0,
            },
        ));
    }

    #[test]
    fn appended_diff_rebuilds_only_the_changed_layout_tail() {
        let file = |path: &str| DiffLine {
            kind: DiffKind::Hunk,
            old_no: None,
            new_no: None,
            hunk: None,
            spans: vec![StyledSpan {
                text: path.to_string(),
                light_fg: [0; 3],
                dark_fg: [0; 3],
                bold: false,
                italic: false,
            }],
        };
        let content = |number| DiffLine {
            kind: DiffKind::Add,
            old_no: None,
            new_no: Some(number),
            hunk: Some(number),
            spans: Vec::new(),
        };
        let expected = vec![
            "first.rs".to_string(),
            "second.rs".to_string(),
            "third.rs".to_string(),
        ];
        let mut viewport = GitDiffViewport {
            generation: 7,
            revision: 1,
            first_line: 0,
            total_lines: 6,
            has_more: true,
            lines: vec![file("first.rs"), content(1), file("second.rs"), content(2)],
            markers: Vec::new(),
            error: String::new(),
        };
        let mut derived = DiffDerivedCache::default();
        derived.update(Some(&viewport), None);
        let mut layout = DiffLayoutCache::default();
        layout.update(
            Some(&viewport),
            &[
                GitDiffRow::Line(0),
                GitDiffRow::Line(1),
                GitDiffRow::Line(2),
                GitDiffRow::Line(3),
            ],
            &expected,
            &derived.fingerprints,
            None,
            false,
        );
        let first = layout.layout.sections[0].clone();

        viewport.revision = 2;
        viewport.has_more = false;
        viewport.lines.extend([file("third.rs"), content(3)]);
        derived.update(Some(&viewport), Some(3));
        let revision = layout.update(
            Some(&viewport),
            &[
                GitDiffRow::Line(0),
                GitDiffRow::Line(1),
                GitDiffRow::Line(2),
                GitDiffRow::Line(3),
                GitDiffRow::Line(4),
                GitDiffRow::Line(5),
            ],
            &expected,
            &derived.fingerprints,
            Some(3),
            false,
        );

        assert_eq!(revision.changed_from, 1);
        assert!(layout.layout.sections[0] == first);
        assert!(
            layout
                .layout
                .sections
                .iter()
                .all(|section| !section.placeholder)
        );
    }

    #[test]
    fn full_row_projection_rebuilds_at_the_same_viewport_revision() {
        let lines = vec![
            DiffLine {
                kind: DiffKind::Hunk,
                old_no: None,
                new_no: None,
                hunk: None,
                spans: vec![StyledSpan {
                    text: "file.rs".to_string(),
                    light_fg: [0; 3],
                    dark_fg: [0; 3],
                    bold: false,
                    italic: false,
                }],
            },
            DiffLine {
                kind: DiffKind::Context,
                old_no: Some(1),
                new_no: Some(1),
                hunk: Some(1),
                spans: Vec::new(),
            },
            DiffLine {
                kind: DiffKind::Context,
                old_no: Some(2),
                new_no: Some(2),
                hunk: Some(1),
                spans: Vec::new(),
            },
            DiffLine {
                kind: DiffKind::Context,
                old_no: Some(3),
                new_no: Some(3),
                hunk: Some(1),
                spans: Vec::new(),
            },
        ];
        let viewport = GitDiffViewport {
            generation: 1,
            revision: 1,
            first_line: 0,
            total_lines: 4,
            has_more: false,
            lines: lines.clone(),
            markers: Vec::new(),
            error: String::new(),
        };
        let fingerprints = DiffFingerprints::of(&lines);
        let mut layout = DiffLayoutCache::default();
        layout.update(
            Some(&viewport),
            &[
                GitDiffRow::Line(0),
                GitDiffRow::Gap {
                    start: 1,
                    end: 4,
                    reveal_start: 1,
                    reveal_end: 4,
                },
            ],
            &[],
            &fingerprints,
            None,
            false,
        );
        assert_eq!(layout.layout.sections[0].rows.len(), 1);

        layout.update(
            Some(&viewport),
            &[
                GitDiffRow::Line(0),
                GitDiffRow::Line(1),
                GitDiffRow::Line(2),
                GitDiffRow::Line(3),
            ],
            &[],
            &fingerprints,
            None,
            false,
        );

        assert_eq!(layout.layout.sections[0].rows.len(), 3);
    }

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
            section.push(GitDiffRow::Line(index), false);
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
        first.push(GitDiffRow::Line(0), false);
        let mut second = DiffSection::new(Some("src/second.rs".to_string()), None);
        second.push(GitDiffRow::Line(1), false);
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
    fn scroll_offset_without_diff_sections_has_no_active_file() {
        let layout = DiffLayout {
            sections: Vec::new(),
        };
        let geometry = DiffGeometry::new(&layout, "branch", &HashSet::new());

        assert_eq!(geometry.file_at(&layout, 0.0), None);
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
    fn a_window_past_the_diff_clamps_to_the_last_section() {
        let mut section = DiffSection::new(Some("src/lib.rs".to_string()), None);
        section.push(GitDiffRow::Line(0), false);
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

        assert_eq!(visible.sections.len(), 1);
        assert_eq!(visible.top_spacer, 0.0);
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
            let fingerprints = DiffFingerprints::of(&lines);
            Self {
                layout: DiffLayout::group(
                    &lines,
                    &[GitDiffRow::Line(0), GitDiffRow::Line(1)],
                    &[],
                    &fingerprints,
                    false,
                ),
            }
        }

        fn fingerprint(&self) -> Option<DiffFingerprint> {
            self.layout.sections[0].fingerprint
        }
    }
}
