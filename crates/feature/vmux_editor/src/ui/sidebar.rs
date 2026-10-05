use dioxus::prelude::*;
use vmux_ecs::event::{ExplorerPanelSetVisible, ExplorerPanelViewportWidth, ExplorerPanelWidth};
use vmux_setting::{EXPLORER_MAX_WIDTH, EXPLORER_MIN_WIDTH};
use vmux_ui::hooks::send;
use vmux_ui::i18n::translate;

use super::explorer::{ExplorerPanel, SidebarView};
use super::key::FileKeys;
use super::{EditorFocus, Mode};

#[derive(Clone, Copy, PartialEq)]
pub(crate) struct ExplorerPane {
    visible: Memo<bool>,
    width: Memo<u32>,
    drag_width: Signal<Option<u32>>,
    resizing: Signal<bool>,
}

impl ExplorerPane {
    pub(super) fn new(visible: Memo<bool>, width: Memo<u32>) -> Self {
        Self {
            visible,
            width,
            drag_width: use_signal(|| None),
            resizing: use_signal(|| false),
        }
    }

    fn rendered_width(self) -> u32 {
        (self.drag_width)().unwrap_or_else(|| (self.width)())
    }

    pub(crate) fn toggle(self, mode: Memo<Mode>) {
        let next = !(self.visible)();
        let _ = send(&ExplorerPanelSetVisible { visible: next });
        if next {
            return;
        }
        match mode() {
            Mode::Text => EditorFocus::file(),
            Mode::Dir | Mode::Media(_) => EditorFocus::container(),
        }
    }

    pub(super) fn resize_to(mut self, x: f64) {
        if !(self.resizing)() {
            return;
        }
        let width = (x as i32).max(0) as u32;
        self.drag_width
            .set(Some(width.clamp(EXPLORER_MIN_WIDTH, EXPLORER_MAX_WIDTH)));
    }

    fn start_resize(mut self) {
        self.resizing.set(true);
    }

    pub(super) fn finish_resize(mut self) {
        if !(self.resizing)() {
            return;
        }
        self.resizing.set(false);
        let px = self.rendered_width();
        self.drag_width.set(None);
        let _ = send(&ExplorerPanelWidth { px });
    }
}

#[component]
pub(super) fn PaneWidth() -> Element {
    rsx! {
        div {
            class: "pointer-events-none absolute inset-x-0 top-0 h-0",
            onresize: move |event: Event<ResizeData>| {
                let Ok(size) = event.get_border_box_size() else {
                    return;
                };
                let _ = send(&ExplorerPanelViewportWidth {
                    px: size.width.max(0.0) as u32,
                });
            },
        }
    }
}

#[component]
pub(super) fn ExplorerSidebar(
    pane: ExplorerPane,
    caret_line: u32,
    view: ReadSignal<SidebarView>,
) -> Element {
    let keys = use_context::<FileKeys>();
    let open = (pane.visible)();
    let panel_width = pane.rendered_width();
    let wrapper_style = if open {
        format!("width:{panel_width}px;min-width:{EXPLORER_MIN_WIDTH}px;")
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
            div { class: "{panel_class}", style: "{panel_style}", ExplorerPanel { visible: pane.visible, caret_line, view } }
        }
        div {
            class: if open {
                "relative z-[2] h-full w-1 shrink-0 cursor-col-resize bg-foreground/[0.06] opacity-100 transition-opacity duration-150 hover:bg-primary/40"
            } else {
                "pointer-events-none h-full w-0 shrink-0 opacity-0"
            },
            onmousedown: move |e: Event<MouseData>| {
                e.prevent_default();
                pane.start_resize();
            },
        }
    }
}

#[component]
pub(super) fn ExplorerToggleButton(pane: ExplorerPane, mode: Memo<Mode>) -> Element {
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
