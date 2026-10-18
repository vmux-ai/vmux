use crate::event::{ActiveSession, SideSheetPane, StackRevealTarget};
use dioxus::prelude::*;
use vmux_ecs::event::space::{
    SpaceAttachRequest, SpaceFormCloseRequest, SpaceFormInputRequest, SpaceFormOpenRequest,
    SpaceFormState, SpaceOpenPageRequest, SpaceRenameRequest, SpaceRow, SpacesListEvent,
};
use vmux_ui::components::context_menu::{ContextMenuContent, ContextMenuItem, ContextMenuTrigger};
use vmux_ui::components::dropdown::Dropdown;
use vmux_ui::components::icon::Icon;
use vmux_ui::components::sheet::{SheetHeaderAction, SheetHeaderChevron};
use vmux_ui::hooks::send;
use vmux_ui::i18n::{TranslationValue, translate, translate_with};
use vmux_ui::icon::{LineIcon, LineIconView};
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
fn SideSheetGrab(mut resizing: Signal<bool>) -> Element {
    let handle_class = if resizing() {
        "h-10 w-1 rounded-full bg-foreground/45 opacity-100"
    } else {
        "h-8 w-1 rounded-full bg-foreground/20 opacity-0 transition-[height,background-color,opacity] duration-150 group-hover:h-10 group-hover:bg-foreground/35 group-hover:opacity-100"
    };
    rsx! {
        div {
            class: "group absolute inset-y-0 z-20 flex w-4 cursor-col-resize items-center justify-center",
            style: "right:-12px;",
            onmousedown: move |event: Event<MouseData>| {
                event.prevent_default();
                resizing.set(true);
            },
            div { class: "{handle_class}" }
        }
    }
}

#[derive(Clone, Copy)]
struct StackReveal {
    settled: Signal<Option<StackRevealTarget>>,
}

impl StackReveal {
    fn forget(mut self) {
        if (self.settled)().is_some() {
            self.settled.set(None);
        }
    }

    fn follow(mut self, target: StackRevealTarget) {
        if (self.settled)().is_some_and(|settled| settled.revision >= target.revision) {
            return;
        }
        if ScrollIntoView::nearest(&format!(
            "sidesheet-stack-{}-{}",
            target.pane_id, target.stack_id
        )) {
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
    let side_sheet = ui.side_sheet.unwrap_or_default();
    let active_space = side_sheet.active_space;
    let spaces = ui.spaces;
    let panes = side_sheet.panes;
    let bookmarks = ui.bookmarks;
    let bookmark_edit = ui.bookmark_edit;
    let space_form = ui.space_form;
    let active_session = ui.active_session;
    let side_sheet_error = layout.error();
    let update_phase = ui.update;
    let reveal = StackReveal {
        settled: use_signal(|| None::<StackRevealTarget>),
    };
    use_effect(move || {
        let ui = layout.value();
        if !ui.layout.unwrap_or_default().side_sheet_open {
            reveal.forget();
            return;
        }
        let Some(target) = ui.side_sheet.unwrap_or_default().reveal else {
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
        crate::event::SPACES_ROW_HEIGHT_PX,
    );
    let active_pane = side_sheet.active_pane;
    let drag_state = use_signal(|| None::<BookmarkDragState>);
    use_context_provider(|| drag_state);
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
                        SideSheetSpaceCard {
                            key: "{space.id}",
                            space,
                            spaces,
                            form: space_form.clone(),
                            active_session,
                        }
                    }
                    if let Some(pane) = active_pane {
                        BookmarksSection {
                            bookmarks: bookmarks.clone(),
                            edit: bookmark_edit.clone(),
                            pane_id: pane.id,
                            expanded: pane.bookmarks_expanded,
                        }
                    }
                    if let Some(err) = side_sheet_error {
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
fn SideSheetSpaceCard(
    space: SpaceRow,
    spaces: SpacesListEvent,
    form: SpaceFormState,
    active_session: Option<ActiveSession>,
) -> Element {
    let mut expanded = use_signal(|| true);
    rsx! {
        div { class: "mb-2 flex shrink-0 flex-col rounded-lg bg-foreground/[0.035] p-1",
            SideSheetSpaceRow {
                space,
                spaces,
                form,
                expanded: expanded(),
                on_toggle: move |_| expanded.set(!expanded()),
            }
            div { class: if expanded() {
                    "grid grid-rows-[1fr] opacity-100 transition-[grid-template-rows,opacity] duration-200 ease-out"
                } else {
                    "grid grid-rows-[0fr] opacity-0 transition-[grid-template-rows,opacity] duration-200 ease-out"
                },
                div { class: "min-h-0 overflow-hidden",
                    if let Some(session) = active_session {
                        ActiveSessionPanel { session }
                    }
                }
            }
        }
    }
}

#[component]
fn SideSheetSpaceRow(
    space: SpaceRow,
    spaces: SpacesListEvent,
    form: SpaceFormState,
    expanded: bool,
    on_toggle: EventHandler<()>,
) -> Element {
    let mut open = use_signal(|| false);
    let editing = form.open && form.space_id.as_deref() == Some(space.id.as_str());
    let menu_value = use_signal(|| space.id.clone());
    let menu_id = space.id.clone();
    let menu_name = space.name.clone();

    rsx! {
        Dropdown {
            open: open(),
            on_dismiss: move |_| open.set(false),
            LayoutContextMenu {
                ContextMenuTrigger { attributes: vec![],
                    div {
                        class: "group relative flex w-full cursor-pointer items-center rounded-md px-2 py-1.5 text-foreground hover:bg-foreground/5",
                        button {
                            r#type: "button",
                            class: if editing {
                                "pointer-events-none absolute inset-0 z-0 rounded outline-none"
                            } else {
                                "absolute inset-0 z-0 rounded outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-primary/60"
                            },
                            title: space.display_name(),
                            aria_label: space.display_name(),
                            disabled: editing,
                            onclick: move |_| on_toggle.call(()),
                        }
                        div { class: "pointer-events-none relative z-10 flex min-w-0 flex-1 items-center gap-2",
                            LineIconView {
                                icon: LineIcon::Planet,
                                class: "h-4 w-4 shrink-0".to_string(),
                            }
                            if editing {
                                SpaceRenameInput { form: form.clone() }
                            } else {
                                span { class: "min-w-0 flex-1 truncate px-1 py-0.5 text-left text-ui font-medium text-foreground",
                                    {space.display_name()}
                                }
                            }
                            div { class: "pointer-events-auto",
                                SheetHeaderAction {
                                    label: translate("spaces-switch"),
                                    active: open(),
                                    onclick: move |event: MouseEvent| {
                                        event.stop_propagation();
                                        open.set(!open());
                                    },
                                    LineIconView {
                                        icon: LineIcon::ChevronsUpDown,
                                        class: "size-3.5".to_string(),
                                    }
                                }
                            }
                            div { class: "pointer-events-auto",
                                SheetHeaderChevron {
                                    expanded,
                                    label: if expanded {
                                        translate("common-collapse")
                                    } else {
                                        translate("common-expand")
                                    },
                                    onclick: move |event: MouseEvent| {
                                        event.stop_propagation();
                                        on_toggle.call(());
                                    },
                                }
                            }
                        }
                    }
                }
                ContextMenuContent { attributes: vec![],
                    ContextMenuItem {
                        index: 0usize,
                        value: Into::<ReadSignal<String>>::into(menu_value),
                        on_select: move |_: String| {
                            let _ = send(&SpaceFormOpenRequest {
                                space_id: Some(menu_id.clone()),
                                draft: menu_name.clone(),
                            });
                        },
                        attributes: vec![],
                        {translate("common-rename")}
                    }
                }
            }
            if open() {
                div { class: "absolute left-1 right-1 top-11 z-40 rounded-md bg-background p-1 shadow-xl ring-1 ring-inset ring-foreground/[0.12]",
                    for choice in spaces.spaces {
                        SpaceChoice {
                            key: "{choice.id}",
                            space: choice,
                            onselect: move |_| open.set(false),
                        }
                    }
                    div { class: "my-1 h-px bg-foreground/[0.08]" }
                    button {
                        r#type: "button",
                        class: "flex h-8 w-full items-center gap-2 rounded-md px-2 text-left text-[11px] text-muted-foreground hover:bg-foreground/[0.05] hover:text-foreground",
                        onclick: move |_| {
                            open.set(false);
                            let _ = send(&SpaceOpenPageRequest);
                        },
                        Icon { class: "size-3.5 shrink-0",
                            path { d: "M12 5v14" }
                            path { d: "M5 12h14" }
                        }
                        span { {translate("common-add")} }
                    }
                }
            }
        }
    }
}

#[component]
fn SpaceChoice(space: SpaceRow, onselect: EventHandler<()>) -> Element {
    let space_id = space.id.clone();
    rsx! {
        button {
            r#type: "button",
            class: if space.is_active {
                "flex h-8 w-full items-center gap-2 rounded-md bg-foreground/[0.07] px-2 text-left text-[11px] text-foreground"
            } else {
                "flex h-8 w-full items-center gap-2 rounded-md px-2 text-left text-[11px] text-muted-foreground hover:bg-foreground/[0.05] hover:text-foreground"
            },
            onclick: move |_| {
                onselect.call(());
                let _ = send(&SpaceAttachRequest { space_id: space_id.clone() });
            },
            LineIconView {
                icon: LineIcon::Planet,
                class: "size-3.5 shrink-0".to_string(),
            }
            span { class: "min-w-0 flex-1 truncate", {space.display_name()} }
            if space.is_active {
                Icon { class: "size-3.5 shrink-0",
                    path { d: "m5 12 4 4L19 6" }
                }
            }
        }
    }
}

#[component]
fn SpaceRenameInput(form: SpaceFormState) -> Element {
    let mut finished = use_signal(|| false);
    let submission = SpaceRenameSubmission(form.clone());
    let enter = submission.clone();
    let blur = submission.clone();
    rsx! {
        input {
            r#type: "text",
            class: "pointer-events-auto min-w-0 flex-1 rounded bg-background/70 px-1 py-0.5 text-ui font-medium text-foreground outline-none ring-1 ring-inset ring-primary/40",
            value: form.draft,
            autofocus: true,
            autocomplete: "off",
            onfocus: move |_| BookmarkInput::set_active(true),
            onclick: move |event| event.stop_propagation(),
            oninput: move |event: FormEvent| {
                let _ = send(&SpaceFormInputRequest { draft: event.value() });
            },
            onkeydown: move |event: KeyboardEvent| match event.key() {
                Key::Enter if !finished() => {
                    event.prevent_default();
                    finished.set(true);
                    BookmarkInput::set_active(false);
                    enter.submit();
                }
                Key::Escape if !finished() => {
                    event.prevent_default();
                    finished.set(true);
                    BookmarkInput::set_active(false);
                    let _ = send(&SpaceFormCloseRequest);
                }
                _ => {}
            },
            onblur: move |_| {
                BookmarkInput::set_active(false);
                if !finished() {
                    finished.set(true);
                    blur.submit();
                }
            },
        }
    }
}

#[derive(Clone)]
struct SpaceRenameSubmission(SpaceFormState);

impl SpaceRenameSubmission {
    fn submit(&self) {
        let name = self.0.draft.trim().to_string();
        if let Some(space_id) = self.0.space_id.as_ref()
            && !name.is_empty()
        {
            let _ = send(&SpaceRenameRequest {
                space_id: space_id.clone(),
                name,
            });
        }
        let _ = send(&SpaceFormCloseRequest);
    }
}

#[component]
fn PaneSection(pane: SideSheetPane, index: usize) -> Element {
    let label = translate_with(
        "layout-stack-number",
        &[("number", TranslationValue::Number((index + 1) as i64))],
    );
    let pane_id = pane.id;
    let any_loading = pane.any_loading;
    let expanded = !pane.collapsed;
    let fold_title = if expanded {
        translate("layout-fold-stack")
    } else {
        translate("layout-unfold-stack")
    };
    let collapsed_stack = pane.collapsed_stack.clone();
    rsx! {
        div { class: if pane.is_active && any_loading {
                "group mb-2 flex shrink-0 flex-col rounded-lg bg-foreground/[0.035] p-1 pane-loading-ring"
            } else {
                "group mb-2 flex shrink-0 flex-col rounded-lg bg-foreground/[0.035] p-1"
            },
            div {
                class: "flex h-9 items-center rounded-md transition-colors hover:bg-glass-hover",
                div { class: "flex min-w-0 flex-1 items-center gap-2 px-2",
                    div { class: if pane.is_active {
                            "grid h-7 w-7 shrink-0 place-items-center text-primary"
                        } else {
                            "grid h-7 w-7 shrink-0 place-items-center text-muted-foreground"
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
                div { class: "mr-2",
                    SheetHeaderChevron {
                        expanded,
                        label: fold_title,
                        subtle: true,
                        onclick: move |_| {
                            let _ = send(&crate::event::SideSheetSectionRequest::new(
                                pane_id,
                                "pane",
                                !expanded,
                            ));
                        },
                    }
                }
            }
            div { class: "pt-0.5",
                div { class: "flex flex-col gap-1",
                    if !expanded && let Some(stack) = collapsed_stack {
                        SideSheetStackRow { stack, pane_id }
                    }
                    div { class: if expanded {
                            "grid grid-rows-[1fr] opacity-100 transition-[grid-template-rows,opacity] duration-200 ease-out"
                        } else {
                            "grid grid-rows-[0fr] opacity-0 transition-[grid-template-rows,opacity] duration-200 ease-out"
                        },
                        div { class: "min-h-0 overflow-hidden",
                            div { class: "flex flex-col gap-1",
                                for stack in pane.stacks.iter() {
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
