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
    let duration = vmux_ui::motion::Motion::LAYOUT_MS;
    let wrapper_width = if open { panel_width } else { 0 };
    let wrapper_style =
        format!("--vmux-sidebar-motion-ms:{duration}ms;width:{wrapper_width}px;min-width:0px;");
    let panel_style = format!("width:{panel_width}px;");
    let wrapper_class = if (pane.resizing)() {
        "relative z-[2] h-full shrink-0 [contain:layout_style] transition-none"
    } else {
        "relative z-[2] h-full shrink-0 [contain:layout_style] transition-[width] duration-[var(--vmux-sidebar-motion-ms)] ease-out motion-reduce:transition-none"
    };
    let panel_class = if open {
        "absolute inset-y-0 left-0 h-full translate-x-0 opacity-100 transition-[translate,opacity] duration-[var(--vmux-sidebar-motion-ms)] ease-out motion-reduce:transition-none"
    } else {
        "pointer-events-none absolute inset-y-0 left-0 h-full -translate-x-3 opacity-0 transition-[translate,opacity] duration-[var(--vmux-sidebar-motion-ms)] ease-out motion-reduce:transition-none"
    };
    rsx! {
        div {
            class: wrapper_class,
            style: "{wrapper_style}",
            onkeydown: move |event| {
                keys.offer(&event);
            },
            div { class: "absolute inset-0 overflow-hidden",
                div { class: "{panel_class}", style: "{panel_style}", ExplorerPanel { visible: pane.visible, caret_line, view } }
            }
            if open {
                div {
                    class: "group absolute inset-y-0 -right-1 z-[3] w-2 cursor-col-resize",
                    onmousedown: move |e: Event<MouseData>| {
                        e.prevent_default();
                        pane.start_resize();
                    },
                    span { class: "pointer-events-none absolute left-1/2 top-1/2 h-9 w-1 -translate-x-1/2 -translate-y-1/2 rounded-full bg-foreground/20 opacity-0 transition-opacity group-hover:opacity-100" }
                }
            }
        }
    }
}

#[component]
pub(super) fn ExplorerToggleButton(pane: ExplorerPane, mode: Memo<Mode>) -> Element {
    let open = (pane.visible)();
    rsx! {
        button {
            class: "flex size-6 shrink-0 cursor-default items-center justify-center rounded-md text-foreground/60 transition-[color,background-color,transform] duration-100 hover:bg-foreground/[0.045] hover:text-foreground active:scale-95 motion-reduce:transition-none",
            title: translate("editor-toggle-explorer"),
            aria_label: translate("editor-toggle-explorer"),
            aria_expanded: open,
            onclick: move |_| {
                pane.toggle(mode)
            },
            svg {
                class: "h-4 w-4",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "1.5",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                rect { x: "3", y: "3", width: "18", height: "18", rx: "2" }
                rect { x: "4", y: "4", width: "4", height: "16", fill: "currentColor", stroke: "none",
                    class: if open {
                        "origin-[4px_12px] scale-x-100 opacity-[0.12] transition-[transform,opacity] duration-100 motion-reduce:transition-none"
                    } else {
                        "origin-[4px_12px] scale-x-0 opacity-0 transition-[transform,opacity] duration-100 motion-reduce:transition-none"
                    },
                }
                line { x1: "9", y1: "3", x2: "9", y2: "21" }
            }
        }
    }
}
