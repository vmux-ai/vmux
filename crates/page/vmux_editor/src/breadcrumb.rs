#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_core::event::*;
use vmux_ui::components::icon::Icon;
use vmux_ui::file_icon::TypeIcon;
use vmux_ui::hooks::{send, use_listener};
use vmux_ui::i18n::translate;

use crate::explorer::OutlineGlyph;

const PATH_CRUMBS_MAX: usize = 4;

#[component]
pub fn EditorBreadcrumbs(
    display_path: String,
    abs_path: String,
    leaf_is_dir: bool,
    outline: Vec<OutlineRow>,
    caret_line: u32,
) -> Element {
    let menus = CrumbMenus {
        open: use_signal(|| None::<CrumbMenu>),
        siblings: use_signal(Vec::<FileDirEntry>::new),
        pending: use_signal(|| false),
    };
    let _listing = use_listener::<FilePreviewEvent, _>(FILE_PREVIEW_EVENT, move |event| {
        menus.receive(event);
    });

    let trail = PathTrail::of(&display_path, &abs_path, leaf_is_dir);
    if trail.is_empty() {
        return rsx! {};
    }
    let symbols = SymbolTrail::at(&outline, caret_line);

    rsx! {
        div {
            class: "relative z-20 flex h-7 shrink-0 items-center overflow-hidden border-b border-foreground/[0.07] bg-background/40 px-4 font-sans text-ui text-muted-foreground",
            "aria-label": translate("editor-breadcrumb"),

            div { class: "flex min-w-0 shrink items-center overflow-hidden",
                if !trail.hidden.is_empty() {
                    {
                        let hidden = trail.hidden.clone();
                        rsx! {
                            button {
                                class: "shrink-0 rounded px-1 py-0.5 tracking-widest hover:bg-foreground/[0.08] hover:text-foreground",
                                title: translate("editor-breadcrumb-hidden"),
                                onclick: move |event: Event<MouseData>| {
                                    let at = event.client_coordinates();
                                    menus.show(CrumbMenu {
                                        x: at.x,
                                        y: at.y,
                                        kind: CrumbMenuKind::Paths(hidden.clone()),
                                    });
                                },
                                "\u{2026}"
                            }
                            CrumbChevron {}
                        }
                    }
                }
                for (index, crumb) in trail.shown.iter().enumerate() {
                    {
                        let last = index + 1 == trail.shown.len();
                        let dir = crumb.parent.clone();
                        let current = crumb.path.clone();
                        let label = crumb.label.clone();
                        rsx! {
                            span {
                                key: "{crumb.path}",
                                class: if last {
                                    "flex shrink-0 items-center"
                                } else {
                                    "flex min-w-0 shrink items-center"
                                },
                                button {
                                    class: "flex min-w-0 items-center gap-1 rounded px-1 py-0.5 hover:bg-foreground/[0.08] hover:text-foreground",
                                    onclick: move |event: Event<MouseData>| {
                                        let at = event.client_coordinates();
                                        menus.show(CrumbMenu {
                                            x: at.x,
                                            y: at.y,
                                            kind: CrumbMenuKind::Siblings {
                                                dir: dir.clone(),
                                                current: current.clone(),
                                            },
                                        });
                                    },
                                    if last {
                                        TypeIcon {
                                            path: crumb.path.clone(),
                                            is_dir: crumb.is_dir,
                                            class: "h-3.5 w-3.5 shrink-0 opacity-80",
                                        }
                                    }
                                    span {
                                        class: if last { "truncate text-foreground/90" } else { "truncate" },
                                        "{label}"
                                    }
                                }
                                if !last {
                                    CrumbChevron {}
                                }
                            }
                        }
                    }
                }
            }

            div { class: "flex min-w-0 shrink items-center overflow-hidden",
                for (index, crumb) in symbols.crumbs.iter().enumerate() {
                    {
                        let last = index + 1 == symbols.crumbs.len();
                        let rows = crumb.siblings.clone();
                        let line = crumb.row.line;
                        let name = crumb.row.name.clone();
                        let kind = crumb.row.kind;
                        rsx! {
                            span {
                                key: "{line}-{name}",
                                class: if last {
                                    "flex shrink-0 items-center"
                                } else {
                                    "flex min-w-0 shrink items-center"
                                },
                                CrumbChevron {}
                                button {
                                    class: "flex min-w-0 items-center rounded px-1 py-0.5 hover:bg-foreground/[0.08] hover:text-foreground",
                                    onclick: move |event: Event<MouseData>| {
                                        let at = event.client_coordinates();
                                        menus.show(CrumbMenu {
                                            x: at.x,
                                            y: at.y,
                                            kind: CrumbMenuKind::Symbols {
                                                rows: rows.clone(),
                                                current: line,
                                            },
                                        });
                                    },
                                    OutlineGlyph { kind }
                                    span { class: "truncate", "{name}" }
                                }
                            }
                        }
                    }
                }
            }

            if let Some(current) = menus.current() {
                div {
                    class: "fixed inset-0 z-[1]",
                    onclick: move |_| menus.hide(),
                    oncontextmenu: move |event: Event<MouseData>| {
                        event.prevent_default();
                        menus.hide();
                    },
                }
                CrumbMenuList {
                    menu: current,
                    entries: menus.entries(),
                    pending: menus.waiting(),
                    on_close: EventHandler::new(move |()| menus.hide()),
                }
            }
        }
    }
}

#[component]
fn CrumbChevron() -> Element {
    rsx! {
        Icon {
            class: "h-3 w-3 shrink-0 text-foreground/25",
            path { d: "m9 18 6-6-6-6" }
        }
    }
}

#[component]
fn CrumbMenuList(
    menu: CrumbMenu,
    entries: Vec<FileDirEntry>,
    pending: bool,
    on_close: EventHandler<()>,
) -> Element {
    let x = menu.x;
    let y = menu.y;
    rsx! {
        div {
            class: "fixed z-[2] max-h-[60dvh] min-w-[200px] max-w-[420px] origin-top-left overflow-y-auto rounded-lg bg-background p-1 text-xs text-foreground shadow-[0_12px_40px_rgba(0,0,0,0.28),inset_0_0_0_1px_var(--border)]",
            style: "left:clamp(8px, {x}px, 100dvw - 430px);top:clamp(8px, {y}px, 100dvh - 240px);",
            onclick: move |event: Event<MouseData>| event.stop_propagation(),
            match &menu.kind {
                CrumbMenuKind::Paths(crumbs) => rsx! {
                    for crumb in crumbs.iter() {
                        {
                            let target = crumb.path.clone();
                            rsx! {
                                button {
                                    key: "{crumb.path}",
                                    class: CRUMB_ITEM_CLASS,
                                    onclick: move |_| {
                                        let _ = send(&FileOpenEvent { path: target.clone() });
                                        on_close.call(());
                                    },
                                    TypeIcon {
                                        path: crumb.path.clone(),
                                        is_dir: crumb.is_dir,
                                        class: "h-4 w-4 shrink-0 opacity-80",
                                    }
                                    span { class: "truncate", "{crumb.label}" }
                                }
                            }
                        }
                    }
                },
                CrumbMenuKind::Siblings { current, .. } => rsx! {
                    if pending {
                        CrumbMenuNotice { label: translate("common-loading") }
                    } else if entries.is_empty() {
                        CrumbMenuNotice { label: translate("editor-breadcrumb-empty") }
                    }
                    for entry in entries.iter() {
                        {
                            let target = entry.path.clone();
                            let active = &entry.path == current;
                            rsx! {
                                button {
                                    key: "{entry.path}",
                                    class: if active { CRUMB_ITEM_ACTIVE_CLASS } else { CRUMB_ITEM_CLASS },
                                    onclick: move |_| {
                                        let _ = send(&FileOpenEvent { path: target.clone() });
                                        on_close.call(());
                                    },
                                    TypeIcon {
                                        path: entry.path.clone(),
                                        is_dir: entry.is_dir,
                                        class: "h-4 w-4 shrink-0 opacity-80",
                                    }
                                    span { class: "truncate", "{entry.name}" }
                                }
                            }
                        }
                    }
                },
                CrumbMenuKind::Symbols { rows, current } => rsx! {
                    for row in rows.iter() {
                        {
                            let line = row.line;
                            let active = row.line == *current;
                            rsx! {
                                button {
                                    key: "{row.line}-{row.name}",
                                    class: if active { CRUMB_ITEM_ACTIVE_CLASS } else { CRUMB_ITEM_CLASS },
                                    onclick: move |_| {
                                        let _ = send(&ExplorerGoto { path: String::new(), line });
                                        on_close.call(());
                                    },
                                    OutlineGlyph { kind: row.kind }
                                    span { class: "truncate", "{row.name}" }
                                }
                            }
                        }
                    }
                },
            }
        }
    }
}

#[component]
fn CrumbMenuNotice(label: String) -> Element {
    rsx! {
        div { class: "px-3 py-2 text-muted-foreground", "{label}" }
    }
}

const CRUMB_ITEM_CLASS: &str = "flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left transition-colors hover:bg-foreground/[0.08]";
const CRUMB_ITEM_ACTIVE_CLASS: &str = "flex w-full items-center gap-2 rounded-md bg-cyan-400/12 px-2 py-1.5 text-left transition-colors hover:bg-foreground/[0.08]";

#[derive(Clone, Copy)]
struct CrumbMenus {
    open: Signal<Option<CrumbMenu>>,
    siblings: Signal<Vec<FileDirEntry>>,
    pending: Signal<bool>,
}

impl CrumbMenus {
    fn show(mut self, next: CrumbMenu) {
        if let CrumbMenuKind::Siblings { dir, .. } = &next.kind {
            self.siblings.set(Vec::new());
            self.pending.set(true);
            let _ = send(&FilePreviewRequest {
                path: dir.clone(),
                thumb: false,
            });
        } else {
            self.pending.set(false);
        }
        self.open.set(Some(next));
    }

    fn hide(mut self) {
        self.open.set(None);
    }

    fn current(self) -> Option<CrumbMenu> {
        (self.open)()
    }

    fn entries(self) -> Vec<FileDirEntry> {
        (self.siblings)()
    }

    fn waiting(self) -> bool {
        (self.pending)()
    }

    fn receive(mut self, event: FilePreviewEvent) {
        if event.thumb {
            return;
        }
        let Some(current) = self.open.peek().clone() else {
            return;
        };
        let CrumbMenuKind::Siblings { dir, .. } = current.kind else {
            return;
        };
        if dir != event.path {
            return;
        }
        let PreviewKind::Dir(entries) = event.kind else {
            return;
        };
        self.siblings.set(entries);
        self.pending.set(false);
    }
}

#[derive(Clone, PartialEq)]
struct CrumbMenu {
    x: f64,
    y: f64,
    kind: CrumbMenuKind,
}

#[derive(Clone, PartialEq)]
enum CrumbMenuKind {
    Paths(Vec<PathCrumb>),
    Siblings { dir: String, current: String },
    Symbols { rows: Vec<OutlineRow>, current: u32 },
}

#[derive(Clone, PartialEq)]
struct PathCrumb {
    label: String,
    path: String,
    parent: String,
    is_dir: bool,
}

#[derive(Clone, PartialEq, Default)]
struct PathTrail {
    hidden: Vec<PathCrumb>,
    shown: Vec<PathCrumb>,
}

impl PathTrail {
    fn of(display: &str, abs: &str, leaf_is_dir: bool) -> Self {
        let mut parts = Vec::new();
        for part in abs.split('/') {
            if !part.is_empty() {
                parts.push(part);
            }
        }
        if parts.is_empty() {
            return Self::default();
        }
        let relative = display.strip_prefix("~/").unwrap_or(display);
        let mut wanted = 0usize;
        for part in relative.split('/') {
            if !part.is_empty() {
                wanted += 1;
            }
        }
        let wanted = wanted.clamp(1, parts.len());
        let start = parts.len() - wanted;
        let mut prefix = String::new();
        for part in parts.iter().take(start) {
            prefix.push('/');
            prefix.push_str(part);
        }
        let mut crumbs = Vec::with_capacity(wanted);
        for (offset, part) in parts[start..].iter().enumerate() {
            let parent = match prefix.is_empty() {
                true => "/".to_string(),
                false => prefix.clone(),
            };
            prefix.push('/');
            prefix.push_str(part);
            crumbs.push(PathCrumb {
                label: (*part).to_string(),
                path: prefix.clone(),
                parent,
                is_dir: leaf_is_dir || offset + 1 < wanted,
            });
        }
        let split = crumbs.len().saturating_sub(PATH_CRUMBS_MAX);
        let hidden = crumbs.drain(..split).collect();
        Self {
            hidden,
            shown: crumbs,
        }
    }

    fn is_empty(&self) -> bool {
        self.shown.is_empty()
    }
}

#[derive(Clone, PartialEq)]
struct SymbolCrumb {
    row: OutlineRow,
    siblings: Vec<OutlineRow>,
}

#[derive(Clone, PartialEq, Default)]
struct SymbolTrail {
    crumbs: Vec<SymbolCrumb>,
}

impl SymbolTrail {
    fn at(rows: &[OutlineRow], line: u32) -> Self {
        let mut chain: Vec<usize> = Vec::new();
        for (index, row) in rows.iter().enumerate() {
            if !row.contains(line) {
                continue;
            }
            while let Some(&open) = chain.last() {
                if rows[open].depth < row.depth {
                    break;
                }
                chain.pop();
            }
            chain.push(index);
        }
        if chain.is_empty() {
            return Self::default();
        }
        let mut crumbs = Vec::with_capacity(chain.len());
        for (level, index) in chain.iter().enumerate() {
            let parent = match level {
                0 => None,
                _ => Some(chain[level - 1]),
            };
            crumbs.push(SymbolCrumb {
                row: rows[*index].clone(),
                siblings: Self::siblings(rows, parent, rows[*index].depth),
            });
        }
        Self { crumbs }
    }

    fn siblings(rows: &[OutlineRow], parent: Option<usize>, depth: u16) -> Vec<OutlineRow> {
        let (start, bound) = match parent {
            Some(index) => (index + 1, Some(rows[index].depth)),
            None => (0, None),
        };
        let mut out = Vec::new();
        for row in &rows[start..] {
            if let Some(bound) = bound
                && row.depth <= bound
            {
                break;
            }
            if row.depth == depth {
                out.push(row.clone());
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    impl PathTrail {
        fn labels(&self) -> Vec<&str> {
            let mut out = Vec::new();
            for crumb in self.shown.iter() {
                out.push(crumb.label.as_str());
            }
            out
        }
    }

    impl SymbolTrail {
        fn names(&self) -> Vec<&str> {
            let mut out = Vec::new();
            for crumb in self.crumbs.iter() {
                out.push(crumb.row.name.as_str());
            }
            out
        }
    }

    fn row(name: &str, line: u32, depth: u16) -> OutlineRow {
        span(name, line, OutlineRow::OPEN_END, depth)
    }

    fn span(name: &str, line: u32, end_line: u32, depth: u16) -> OutlineRow {
        OutlineRow {
            name: name.to_string(),
            kind: 12,
            line,
            end_line,
            depth,
        }
    }

    #[test]
    fn trail_shows_the_project_relative_tail_with_absolute_targets() {
        let trail = PathTrail::of("crates/page/x.rs", "/home/me/proj/crates/page/x.rs", false);
        assert_eq!(trail.labels(), vec!["crates", "page", "x.rs"]);
        assert!(trail.hidden.is_empty());
        assert_eq!(trail.shown[0].path, "/home/me/proj/crates");
        assert_eq!(trail.shown[0].parent, "/home/me/proj");
        assert_eq!(trail.shown[2].path, "/home/me/proj/crates/page/x.rs");
        assert_eq!(trail.shown[2].parent, "/home/me/proj/crates/page");
        assert!(trail.shown[1].is_dir);
        assert!(!trail.shown[2].is_dir);
    }

    #[test]
    fn trail_drops_the_home_marker_and_keeps_real_directory_names() {
        let trail = PathTrail::of("~/notes/todo.md", "/home/me/notes/todo.md", false);
        assert_eq!(trail.labels(), vec!["notes", "todo.md"]);
        assert_eq!(trail.shown[0].path, "/home/me/notes");
    }

    #[test]
    fn trail_collapses_leading_segments_past_the_cap() {
        let trail = PathTrail::of("a/b/c/d/e/f.rs", "/root/a/b/c/d/e/f.rs", false);
        assert_eq!(trail.labels(), vec!["c", "d", "e", "f.rs"]);
        let mut hidden = Vec::new();
        for crumb in trail.hidden.iter() {
            hidden.push(crumb.label.as_str());
        }
        assert_eq!(hidden, vec!["a", "b"]);
    }

    #[test]
    fn trail_marks_every_segment_of_a_directory_target_as_a_directory() {
        let trail = PathTrail::of("src/page", "/proj/src/page", true);
        assert!(trail.shown.iter().all(|crumb| crumb.is_dir));
    }

    #[test]
    fn trail_is_empty_without_an_absolute_path() {
        assert!(PathTrail::of("", "", false).is_empty());
    }

    #[test]
    fn symbols_are_the_chain_enclosing_the_caret() {
        let rows = [
            row("Outer", 0, 0),
            row("inner", 4, 1),
            row("deep", 6, 2),
            row("Other", 20, 0),
        ];
        assert_eq!(
            SymbolTrail::at(&rows, 7).names(),
            vec!["Outer", "inner", "deep"]
        );
        assert_eq!(SymbolTrail::at(&rows, 5).names(), vec!["Outer", "inner"]);
        assert_eq!(SymbolTrail::at(&rows, 25).names(), vec!["Other"]);
        assert!(SymbolTrail::at(&rows, 0).names() == vec!["Outer"]);
    }

    #[test]
    fn symbols_are_empty_above_the_first_symbol_and_without_an_outline() {
        let rows = [row("Outer", 3, 0)];
        assert!(SymbolTrail::at(&rows, 2).crumbs.is_empty());
        assert!(SymbolTrail::at(&[], 9).crumbs.is_empty());
    }

    #[test]
    fn symbol_siblings_stay_inside_the_enclosing_symbol() {
        let rows = [
            row("A", 0, 0),
            row("a1", 1, 1),
            row("a2", 2, 1),
            row("B", 10, 0),
            row("b1", 11, 1),
        ];
        let trail = SymbolTrail::at(&rows, 2);
        assert_eq!(trail.names(), vec!["A", "a2"]);
        let mut top = Vec::new();
        for row in trail.crumbs[0].siblings.iter() {
            top.push(row.name.as_str());
        }
        assert_eq!(top, vec!["A", "B"]);
        let mut nested = Vec::new();
        for row in trail.crumbs[1].siblings.iter() {
            nested.push(row.name.as_str());
        }
        assert_eq!(nested, vec!["a1", "a2"]);
    }

    #[test]
    fn symbol_chain_survives_a_skipped_depth_level() {
        let rows = [row("Title", 0, 0), row("Step", 3, 2)];
        assert_eq!(SymbolTrail::at(&rows, 4).names(), vec!["Title", "Step"]);
    }

    #[test]
    fn symbols_end_with_the_body_instead_of_running_on_to_the_next_one() {
        let rows = [
            span("first", 0, 8, 0),
            span("nested", 2, 6, 1),
            span("second", 20, 30, 0),
        ];
        assert_eq!(SymbolTrail::at(&rows, 4).names(), vec!["first", "nested"]);
        assert_eq!(SymbolTrail::at(&rows, 7).names(), vec!["first"]);
        assert!(SymbolTrail::at(&rows, 12).crumbs.is_empty());
        assert_eq!(SymbolTrail::at(&rows, 25).names(), vec!["second"]);
    }

    #[test]
    fn a_symbol_with_no_extent_holds_the_caret_until_the_next_one_starts() {
        let rows = [
            row("open", 0, 0),
            span("bounded", 4, 6, 1),
            row("later", 40, 0),
        ];
        assert_eq!(SymbolTrail::at(&rows, 900).names(), vec!["later"]);
        assert_eq!(SymbolTrail::at(&rows, 20).names(), vec!["open"]);
        assert_eq!(SymbolTrail::at(&rows, 5).names(), vec!["open", "bounded"]);
    }
}
