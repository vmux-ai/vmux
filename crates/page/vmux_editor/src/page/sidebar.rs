use dioxus::prelude::*;
use vmux_ui::i18n::translate;

use super::{EXPLORER_MIN_WIDTH_PX, ExplorerPane, Mode};
use crate::explorer::{ExplorerPanel, SidebarView};
use crate::page_key::FileKeys;

#[component]
pub(super) fn PaneWidth(mut width: Signal<u32>) -> Element {
    rsx! {
        div {
            class: "pointer-events-none absolute inset-x-0 top-0 h-0",
            onresize: move |event: Event<ResizeData>| {
                let Ok(size) = event.get_border_box_size() else {
                    return;
                };
                width.set(size.width.max(0.0) as u32);
            },
        }
    }
}

#[component]
pub(super) fn ExplorerSidebar(
    visible: Signal<bool>,
    width: Signal<u32>,
    mut resizing: Signal<bool>,
    caret_line: u32,
    view: Signal<SidebarView>,
) -> Element {
    let keys = use_context::<FileKeys>();
    let open = visible();
    let panel_width = width();
    let wrapper_style = if open {
        format!("width:{panel_width}px;min-width:{EXPLORER_MIN_WIDTH_PX}px;")
    } else {
        "width:0px;min-width:0px;".to_string()
    };
    let panel_style = if open {
        "width:100%;".to_string()
    } else {
        format!("width:{panel_width}px;")
    };
    let panel_class = if open {
        "absolute inset-y-0 left-0 h-full translate-x-0 opacity-100 transition-[translate,opacity] duration-200 ease-out will-change-[translate]"
    } else {
        "pointer-events-none absolute inset-y-0 left-0 h-full -translate-x-full opacity-0 transition-[translate,opacity] duration-200 ease-out will-change-[translate]"
    };
    rsx! {
        div {
            class: "relative z-[2] h-full shrink [contain:layout_style]",
            style: "{wrapper_style}",
            onkeydown: move |event| {
                keys.offer(&event);
            },
            div { class: "{panel_class}", style: "{panel_style}", ExplorerPanel { visible, caret_line, view } }
        }
        div {
            class: if open {
                "relative z-[2] h-full w-1 shrink-0 cursor-col-resize bg-foreground/[0.06] opacity-100 transition-opacity duration-150 hover:bg-primary/40"
            } else {
                "pointer-events-none h-full w-0 shrink-0 opacity-0"
            },
            onmousedown: move |e: Event<MouseData>| {
                e.prevent_default();
                resizing.set(true);
            },
        }
    }
}

#[component]
pub(super) fn ExplorerToggleButton(pane: ExplorerPane, mode: Signal<Mode>) -> Element {
    rsx! {
        button {
            class: "shrink-0 cursor-default rounded p-0.5 text-foreground/60 hover:bg-foreground/[0.08] hover:text-foreground",
            title: translate("editor-toggle-explorer"),
            onclick: move |_| {
                pane.toggle(mode)
            },
            svg {
                class: "h-4 w-4",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                rect { x: "3", y: "3", width: "18", height: "18", rx: "2" }
                line { x1: "9", y1: "3", x2: "9", y2: "21" }
            }
        }
    }
}
