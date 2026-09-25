use crate::event::{PaneNode, PaneTreeState};
use dioxus::prelude::*;
use vmux_ui::components::context_menu::{ContextMenuContent, ContextMenuItem, ContextMenuTrigger};
use vmux_ui::components::icon::Icon;
use vmux_ui::components::inline_edit::EditableText;
use vmux_ui::components::tree_row::{SIDEBAR_CARD_CHEVRON_CLOSED, SIDEBAR_CARD_CHEVRON_OPEN};
use vmux_ui::hooks::send;
use vmux_ui::i18n::{TranslationValue, translate, translate_with};
use vmux_ui::scroll::ScrollIntoView;

use super::active_session::ActiveSessionPanel;
use super::bookmark::{
    BookmarkContext, BookmarkDragState, BookmarkInput, BookmarksSection, LayoutContextMenu,
};
use super::stack::{NewStackRow, SideSheetStackRow};
use super::state::LayoutUi;
use super::update::UpdateNoticeFooter;
use super::window_drag::WindowDragRegion;

#[component]
pub(super) fn SideSheetGrab(mut resizing: Signal<bool>) -> Element {
    let handle_class = if resizing() {
        "relative flex h-10 w-2 items-center justify-center rounded-full bg-primary/20 shadow-sm ring-1 ring-primary/50"
    } else {
        "relative flex h-8 w-1.5 items-center justify-center rounded-full bg-background/80 opacity-0 shadow-sm ring-1 ring-foreground/15 transition-all duration-150 group-hover:h-10 group-hover:w-2 group-hover:bg-primary/15 group-hover:opacity-100 group-hover:ring-primary/45"
    };
    let line_class = if resizing() {
        "absolute inset-y-0 left-1/2 w-px -translate-x-1/2 bg-primary/45"
    } else {
        "absolute inset-y-0 left-1/2 w-px -translate-x-1/2 bg-primary/45 opacity-0 transition-opacity duration-150 group-hover:opacity-100"
    };
    rsx! {
        div {
            class: "group absolute inset-y-0 z-20 flex w-6 cursor-col-resize items-center justify-center",
            style: "right:-10px;",
            onmousedown: move |event: Event<MouseData>| {
                event.prevent_default();
                resizing.set(true);
            },
            div { class: "{line_class}" }
            div { class: "{handle_class}",
                div { class: "h-5 w-px rounded-full bg-foreground/40 transition-colors duration-150 group-hover:bg-primary/90" }
            }
        }
    }
}

pub(super) struct ActiveStack;

impl ActiveStack {
    pub(super) fn find(panes: &[PaneNode]) -> Option<(u64, u64)> {
        let mut fallback = None;
        for pane in panes {
            for stack in &pane.stacks {
                if !stack.is_active {
                    continue;
                }
                if pane.is_active {
                    return Some((pane.id, stack.id));
                }
                if fallback.is_none() {
                    fallback = Some((pane.id, stack.id));
                }
            }
        }
        fallback
    }
}

#[derive(Clone, Copy)]
pub(super) struct StackReveal {
    settled: Signal<Option<(u64, u64)>>,
    prefix: &'static str,
}

impl StackReveal {
    pub(super) fn side_sheet(settled: Signal<Option<(u64, u64)>>) -> Self {
        Self {
            settled,
            prefix: "sidesheet-stack",
        }
    }

    pub(super) fn forget(mut self) {
        if (self.settled)().is_some() {
            self.settled.set(None);
        }
    }

    pub(super) fn follow(mut self, target: (u64, u64)) {
        let Some(settled) = (self.settled)() else {
            self.settled.set(Some(target));
            return;
        };
        if settled == target {
            return;
        }
        let (pane_id, stack_id) = target;
        if ScrollIntoView::nearest(&format!("{}-{pane_id}-{stack_id}", self.prefix)) {
            self.settled.set(Some(target));
        }
    }
}

#[component]
pub(super) fn SideSheetView() -> Element {
    let layout = LayoutUi::current();
    let ui = layout.value();
    let state = ui.layout.unwrap_or_default();
    if !ui.overlay_ready(&layout.error()) || !state.side_sheet_open {
        return rsx! {};
    }

    rsx! { SideSheetContent {} }
}

#[component]
fn SideSheetContent() -> Element {
    let layout = LayoutUi::current();
    let ui = layout.value();
    let state = ui.layout.unwrap_or_default();
    let PaneTreeState { panes } = ui.pane_tree.unwrap_or_default();
    let active_space = ui
        .spaces
        .unwrap_or_default()
        .spaces
        .into_iter()
        .find(|space| space.is_active);
    let bookmarks = ui.bookmarks;
    let active_session = ui.active_session;
    let pane_tree_error = layout.error();
    let update_phase = ui.update;
    let reveal = StackReveal::side_sheet(use_signal(|| None::<(u64, u64)>));
    use_effect(move || {
        let ui = layout.value();
        if !ui.layout.unwrap_or_default().side_sheet_open {
            reveal.forget();
            return;
        }
        let PaneTreeState { panes } = ui.pane_tree.unwrap_or_default();
        let Some(target) = ActiveStack::find(&panes) else {
            return;
        };
        reveal.follow(target);
    });
    let host_sheet_width = state.side_sheet_width;
    let sheet_left = state.window_pad_left;
    let mut sheet_width = use_signal(|| host_sheet_width);
    let mut sheet_resizing = use_signal(|| false);
    use_effect(use_reactive!(|host_sheet_width| {
        if !*sheet_resizing.peek() {
            sheet_width.set(host_sheet_width);
        }
    }));
    let side_sheet_vars = format!(
        "--vmux-side-sheet-width:{}px;--vmux-side-sheet-left:{}px;--vmux-side-sheet-top:{}px;--vmux-side-sheet-bottom:{}px;--vmux-side-sheet-pad-top:{}px;",
        sheet_width(),
        state.window_pad_left,
        state.window_pad_top,
        state.window_pad_bottom,
        crate::event::url_bar_top(),
    );
    let active_pane = panes
        .iter()
        .find(|pane| pane.is_active)
        .or_else(|| panes.first())
        .cloned();
    let active_page = active_session.as_ref().map(|session| session.page.clone());
    let folders = bookmarks.folders.clone();
    let initial_folders = folders.clone();
    let mut folder_context = use_signal(|| initial_folders);
    let drag_state = use_signal(|| None::<BookmarkDragState>);
    use_context_provider(|| folder_context);
    use_context_provider(|| drag_state);
    use_effect(move || folder_context.set(folders.clone()));
    use_drop(move || {
        BookmarkContext::set_active(false);
    });
    rsx! {
        aside {
            id: "vmux-side-sheet",
            class: "pointer-events-auto fixed left-[var(--vmux-side-sheet-left)] top-[var(--vmux-side-sheet-top)] bottom-[var(--vmux-side-sheet-bottom)] min-h-0 overflow-visible w-[var(--vmux-side-sheet-width)] pt-[var(--vmux-side-sheet-pad-top)]",
            style: "{side_sheet_vars}",
            WindowDragRegion {
                id: "side-sheet-titlebar",
                revision: sheet_width().to_string(),
                class: "pointer-events-none absolute top-0 h-7",
                style: "left:80px;right:4px;",
            }
            SideSheetGrab { resizing: sheet_resizing }
            div { class: "flex h-full min-h-0 flex-col",
                div {
                    class: "flex min-h-0 flex-1 flex-col overflow-x-hidden overflow-y-auto px-2 pb-3 pt-2 text-foreground [scrollbar-gutter:stable]",
                    ..BookmarkDragState::listeners(drag_state),
                    if let Some(space) = active_space {
                        div { class: "glass mb-2 flex shrink-0 flex-col overflow-hidden rounded-lg",
                            SideSheetSpaceRow { key: "{space.id}", space: space.clone() }
                            if let Some(session) = active_session {
                                ActiveSessionPanel { session }
                            }
                        }
                    }
                    if let Some(pane) = active_pane {
                        BookmarksSection {
                            bookmarks: bookmarks.clone(),
                            active_page,
                            pane_id: pane.id,
                            expanded: pane.bookmarks_expanded,
                        }
                    }
                    if let Some(err) = pane_tree_error {
                        div { class: "flex shrink-0 items-center px-2 py-1",
                            span { class: "text-ui text-destructive", "{err}" }
                        }
                    } else if panes.is_empty() {
                        div { class: "flex shrink-0 items-center px-2 py-1",
                            span { class: "text-ui text-muted-foreground", {translate("layout-no-stacks")} }
                        }
                    } else {
                        for (i, pane) in panes.iter().enumerate() {
                            PaneSection { key: "{pane.id}", pane: pane.clone(), index: i }
                        }
                    }
                }
                if let Some(phase) = update_phase {
                    UpdateNoticeFooter { phase }
                }
            }
        }
        if sheet_resizing() {
            div {
                class: "pointer-events-auto fixed inset-0 z-[900] cursor-col-resize",
                onmousemove: move |event: Event<MouseData>| {
                    let x = event.client_coordinates().x as f32 - sheet_left;
                    let width = crate::event::SideSheetResizeEvent::live(x).clamped();
                    sheet_width.set(width);
                    let _ = send(&crate::event::SideSheetResizeEvent::live(width));
                },
                onmouseup: move |_| {
                    sheet_resizing.set(false);
                    let _ = send(&crate::event::SideSheetResizeEvent::settled(sheet_width()));
                },
            }
        }
    }
}

#[component]
fn SideSheetSpaceRow(space: vmux_core::event::space::SpaceRow) -> Element {
    let editing = use_signal(|| false);
    let draft = use_signal(|| space.name.clone());
    let menu_value = use_signal(|| space.id.clone());
    let rename_id = space.id.clone();

    rsx! {
        LayoutContextMenu {
            ContextMenuTrigger { attributes: vec![],
                div {
                    class: "group relative flex w-full cursor-pointer items-center px-2 py-1.5 text-foreground hover:bg-foreground/5",
                    button {
                        r#type: "button",
                        class: if editing() {
                            "pointer-events-none absolute inset-0 z-0 rounded outline-none"
                        } else {
                            "absolute inset-0 z-0 rounded outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-primary/60"
                        },
                        title: space.name.clone(),
                        aria_label: space.name.clone(),
                        disabled: editing(),
                        onclick: move |_| {
                            let _ = send(&vmux_core::event::space::SpaceOpenPageRequest);
                        },
                    },
                    div { class: "pointer-events-none relative z-10 flex min-w-0 flex-1 items-center gap-2",
                        Icon { class: "h-4 w-4 shrink-0",
                            path { d: "M3 3h7v7H3z" }
                            path { d: "M14 3h7v7h-7z" }
                            path { d: "M3 14h7v7H3z" }
                            path { d: "M14 14h7v7h-7z" }
                        }
                        EditableText {
                            value: space.name.clone(),
                            editing,
                            draft,
                            display_class: "pointer-events-auto min-w-0 flex-1 cursor-text truncate rounded px-1 py-0.5 text-left text-ui font-medium text-foreground hover:bg-foreground/[0.06]".to_string(),
                            input_class: "pointer-events-auto min-w-0 flex-1 rounded bg-background/70 px-1 py-0.5 text-ui font-medium text-foreground outline-none ring-1 ring-inset ring-primary/40".to_string(),
                            title: translate("common-rename"),
                            on_active_change: BookmarkInput::set_active,
                            on_commit: move |name| {
                                let _ = send(&vmux_core::event::space::SpaceRenameRequest {
                                    space_id: rename_id.clone(),
                                    name,
                                });
                            },
                        }
                    }
                }
            }
            ContextMenuContent { attributes: vec![],
                ContextMenuItem {
                    index: 0usize,
                    value: Into::<ReadSignal<String>>::into(menu_value),
                    on_select: move |_: String| {
                        BookmarkInput::begin_rename(editing, draft, space.name.clone())
                    },
                    attributes: vec![],
                    {translate("common-rename")}
                }
            }
        }
    }
}

#[component]
fn PaneSection(pane: PaneNode, index: usize) -> Element {
    let label = translate_with(
        "layout-stack-number",
        &[("number", TranslationValue::Number((index + 1) as i64))],
    );
    let pane_id = pane.id;
    let any_loading = pane.stacks.iter().any(|s| s.is_loading);
    let expanded = !pane.collapsed;
    let fold_title = if expanded {
        translate("layout-fold-stack")
    } else {
        translate("layout-unfold-stack")
    };
    let visible_stacks = pane
        .stacks
        .iter()
        .filter(|stack| !(stack.url.is_empty() && stack.title == "New Stack"))
        .cloned()
        .collect::<Vec<_>>();
    let active_stack = visible_stacks
        .iter()
        .find(|stack| stack.is_active)
        .or_else(|| visible_stacks.first())
        .cloned();
    rsx! {
        div { class: if pane.is_active && any_loading {
                "glass group mb-2 flex shrink-0 flex-col overflow-hidden rounded-lg pane-loading-ring"
            } else {
                "glass group mb-2 flex shrink-0 flex-col overflow-hidden rounded-lg"
            },
            div {
                class: "flex items-center transition-colors hover:bg-glass-hover",
                div { class: "flex min-w-0 flex-1 items-center gap-2 px-2.5 py-2",
                    div { class: if pane.is_active {
                            "grid h-7 w-7 shrink-0 place-items-center rounded-lg bg-primary/10 text-primary ring-1 ring-inset ring-primary/20"
                        } else {
                            "grid h-7 w-7 shrink-0 place-items-center rounded-lg bg-foreground/[0.07] text-foreground ring-1 ring-inset ring-foreground/10"
                        },
                        Icon { class: "h-3.5 w-3.5",
                            path { d: "M4 6h16M4 12h16M4 18h16" }
                        }
                    }
                    span {
                        class: if pane.is_active {
                            "min-w-0 flex-1 text-ui font-semibold text-foreground"
                        } else {
                            "min-w-0 flex-1 text-ui font-medium text-muted-foreground"
                        },
                        "{label}"
                    }
                }
                button {
                    r#type: "button",
                    aria_label: "{fold_title}",
                    title: "{fold_title}",
                    class: if expanded {
                        "mr-2 flex h-6 w-6 shrink-0 cursor-pointer items-center justify-center rounded-sm text-muted-foreground opacity-0 transition-opacity group-hover:opacity-100 focus-visible:opacity-100 hover:bg-foreground/10 hover:text-foreground"
                    } else {
                        "mr-2 flex h-6 w-6 shrink-0 cursor-pointer items-center justify-center rounded-sm bg-foreground/10 text-foreground"
                    },
                    onclick: move |_| {
                        let _ = send(&crate::event::SideSheetSectionRequest::new(
                            pane_id,
                            "pane",
                            !expanded,
                        ));
                    },
                    Icon {
                        class: if expanded { SIDEBAR_CARD_CHEVRON_OPEN } else { SIDEBAR_CARD_CHEVRON_CLOSED },
                        path { d: "m9 18 6-6-6-6" }
                    }
                }
            }
            div { class: "border-t border-foreground/10 p-1.5",
                div { class: "flex flex-col gap-1",
                    if !expanded && let Some(stack) = active_stack {
                        SideSheetStackRow { stack, pane_id }
                    }
                    div { class: if expanded {
                            "grid grid-rows-[1fr] opacity-100 transition-[grid-template-rows,opacity] duration-200 ease-out"
                        } else {
                            "grid grid-rows-[0fr] opacity-0 transition-[grid-template-rows,opacity] duration-200 ease-out"
                        },
                        div { class: "min-h-0 overflow-hidden",
                            div { class: "flex flex-col gap-1",
                                for stack in visible_stacks.iter() {
                                    SideSheetStackRow { stack: stack.clone(), pane_id }
                                }
                                NewStackRow { pane_id }
                            }
                        }
                    }
                }
            }
        }
    }
}
