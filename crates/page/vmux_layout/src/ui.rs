#![allow(non_snake_case)]

mod bookmark;
mod header;
mod side_sheet;
mod stack;
mod state;
mod tab_drag;
mod update;
mod window_drag;

use self::header::HeaderView;
use self::side_sheet::{ActiveStack, OverlayReadiness, SideSheetGrab, SideSheetView, StackReveal};
use self::state::LayoutPageState;
use self::update::UpdateNoticeFooter;
use self::window_drag::WindowDragRegion;
use crate::event::{PaneTreeEvent, ReloadEvent};
use crate::extension::ExtensionPopupModal;
use dioxus::prelude::*;
use vmux_api::bookmark::BookmarkMenuActionEvent;
use vmux_command::panel::CommandBarPanel;

use vmux_ui::hooks::{send, use_listener, use_theme};

#[component]
pub fn Page() -> Element {
    use_theme();
    let layout_ui = LayoutPageState::use_state();
    let mut bookmark_menu_action = use_signal(BookmarkMenuActionEvent::default);
    let bookmark_menu_state = bookmark_menu_action;
    let _bookmark_menu_listener = use_listener::<BookmarkMenuActionEvent, _>(move |event| {
        bookmark_menu_action.set(event);
    });
    use_context_provider(|| bookmark_menu_state);

    let mut reload_key = use_signal(|| 0u32);
    let _reload_listener = use_listener::<ReloadEvent, _>(move |_| {
        reload_key.set(reload_key() + 1);
    });

    let ui = layout_ui.value();
    let layout_ready = ui.layout.is_some();
    let stacks_ready = ui.stacks.is_some();
    let tabs_ready = ui.tabs.is_some();
    let pane_tree_ready = ui.pane_tree.is_some();
    let spaces_ready = ui.spaces.is_some();
    let state = ui.layout.unwrap_or_default();
    let stacks = ui.stacks.unwrap_or_default();
    let tabs = ui.tabs.unwrap_or_default();
    let bookmarks = ui.bookmarks;
    let PaneTreeEvent { panes } = ui.pane_tree.unwrap_or_default();
    let active_space = ui
        .spaces
        .unwrap_or_default()
        .spaces
        .into_iter()
        .find(|space| space.is_active);
    let projects = ui.projects;
    let team = ui.team;
    let remote = ui.remote;
    let extensions = ui.extensions;
    let extension_popup = ui.extension_popup;
    let extension_popup_size = ui.extension_popup_size;
    let update_phase = ui.update;
    let ui_error = layout_ui.error();
    let overlay_ready = OverlayReadiness::is_ready(
        &state,
        OverlayReadiness::listener(layout_ready, &ui_error),
        OverlayReadiness::listener(stacks_ready, &ui_error),
        OverlayReadiness::listener(tabs_ready, &ui_error),
        OverlayReadiness::listener(pane_tree_ready, &ui_error),
        OverlayReadiness::listener(spaces_ready, &ui_error),
    );
    let reveal = StackReveal::side_sheet(use_signal(|| None::<(u64, u64)>));
    use_effect(move || {
        let state = layout_ui.value();
        if !state.layout.unwrap_or_default().side_sheet_open {
            reveal.forget();
            return;
        }
        let PaneTreeEvent { panes } = state.pane_tree.unwrap_or_default();
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
    let header_vars = format!(
        "--vmux-header-top:{}px;--vmux-header-left:{}px;--vmux-header-right:{}px;--vmux-header-height:{}px;--vmux-tab-row-pad-left:{}px;",
        state.header_top(),
        state.header_left(),
        state.header_right(),
        state.header_height,
        state.tab_row_pad_left(),
    );

    rsx! {
        div { class: "fixed inset-0 pointer-events-none text-foreground",
            if overlay_ready && state.side_sheet_open {
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
                        SideSheetView {
                            panes,
                            active_space,
                            bookmarks: bookmarks.clone(),
                            projects: projects.projects.clone(),
                            boundary: projects.boundary,
                            team: team.members.clone(),
                            pane_tree_error: ui_error.clone(),
                        }
                        if let Some(phase) = update_phase.clone() {
                            UpdateNoticeFooter { phase }
                        }
                    }
                }
            }
            if overlay_ready && state.header_visible() {
                div {
                    class: "pointer-events-auto fixed top-[var(--vmux-header-top)] left-[var(--vmux-header-left)] right-[var(--vmux-header-right)] h-[var(--vmux-header-height)]",
                    style: "{header_vars}",
                    HeaderView {
                        stacks_state: stacks,
                        tabs_state: tabs,
                        bookmarks,
                        team: team.members,
                        extensions: extensions.extensions,
                        remote,
                        reload_key: reload_key(),
                        stacks_error: ui_error.clone(),
                        tabs_error: ui_error.clone(),
                    }
                }
            }
            CommandBarPanel {}
            if !extension_popup.id.is_empty() {
                ExtensionPopupModal {
                    popup: extension_popup,
                    preferred_size: extension_popup_size,
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
}
