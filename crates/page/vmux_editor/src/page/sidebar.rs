use dioxus::prelude::*;
use vmux_core::event::{
    ExplorerPanelEvent, ExplorerPanelSetVisible, ExplorerPanelWidth, ExplorerRevealCurrent,
};
use vmux_ui::hooks::send;
use vmux_ui::i18n::translate;
use vmux_ui::platform::{now_millis, random_index, sleep_ms};

use super::{Mode, focus_container, focus_file_input};
use crate::explorer::{ExplorerPanel, SidebarView};
use crate::page_key::FileKeys;

const EXPLORER_SQUEEZE_TOLERANCE_PX: u32 = 160;
const EDITOR_MIN_WIDTH_PX: u32 = 320;
const EXPLORER_MIN_WIDTH_PX: u32 = 160;
const EXPLORER_MAX_WIDTH_PX: u32 = 600;
const NOTE_MAX_CONTENT_WIDTH_PX: u32 = 768;

#[derive(Clone, Copy)]
struct ExplorerRoom {
    page_width: u32,
    explorer_width: u32,
    open: bool,
}

impl ExplorerRoom {
    fn fits(self) -> bool {
        if self.page_width == 0 {
            return false;
        }
        let mut needed = NOTE_MAX_CONTENT_WIDTH_PX.saturating_add(self.explorer_width);
        if self.open {
            needed = needed.saturating_sub(EXPLORER_SQUEEZE_TOLERANCE_PX);
        }
        self.page_width >= needed
    }

    fn leaves_editor_usable(self) -> bool {
        if self.page_width == 0 {
            return false;
        }
        self.page_width >= self.explorer_width.saturating_add(EDITOR_MIN_WIDTH_PX)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct ExplorerReflowKey {
    page_width: u32,
    preferred_visible: bool,
}

#[derive(Clone, Copy)]
struct ExplorerRequest {
    client_id: u64,
    request_id: u64,
}

impl ExplorerRequest {
    fn accepts(self, client_id: u64, request_id: u64) -> bool {
        client_id != self.client_id || request_id >= self.request_id
    }
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) struct ExplorerPane {
    visible: Signal<bool>,
    preferred_visible: Signal<bool>,
    width: Signal<u32>,
    page_width: Signal<u32>,
    client_id: Signal<u64>,
    request_id: Signal<u64>,
    reflowed_at: Signal<Option<ExplorerReflowKey>>,
    user_chose: Signal<bool>,
    resizing: Signal<bool>,
}

impl ExplorerPane {
    pub(super) fn new(page_width: Signal<u32>) -> Self {
        Self {
            visible: use_signal(|| false),
            preferred_visible: use_signal(|| false),
            width: use_signal(|| 240),
            page_width,
            client_id: use_signal(|| ((now_millis() as u64) << 12) ^ random_index(4096) as u64),
            request_id: use_signal(|| 0),
            reflowed_at: use_signal(|| None),
            user_chose: use_signal(|| false),
            resizing: use_signal(|| false),
        }
    }

    fn room(self) -> ExplorerRoom {
        ExplorerRoom {
            page_width: (self.page_width)(),
            explorer_width: (self.width)(),
            open: (self.visible)(),
        }
    }

    fn has_room(self) -> bool {
        self.room().fits()
    }

    fn opens_unasked(self) -> bool {
        let room = self.room();
        room.leaves_editor_usable() && room.fits()
    }

    fn request(self) -> ExplorerRequest {
        ExplorerRequest {
            client_id: (self.client_id)(),
            request_id: (self.request_id)(),
        }
    }

    fn reflow_key(self) -> ExplorerReflowKey {
        ExplorerReflowKey {
            page_width: (self.page_width)(),
            preferred_visible: (self.preferred_visible)(),
        }
    }

    pub(super) fn apply_panel(mut self, event: ExplorerPanelEvent) {
        if self.request().accepts(event.client_id, event.request_id) {
            self.preferred_visible.set(event.visible);
        }
        if (self.width)() != event.width {
            self.width.set(event.width);
        }
        self.sync();
    }

    pub(super) fn sync(mut self) {
        let key = self.reflow_key();
        if key.page_width == 0 || (self.reflowed_at)() == Some(key) {
            return;
        }
        self.reflowed_at.set(Some(key));
        let next = key.preferred_visible && ((self.user_chose)() || self.opens_unasked());
        if (self.visible)() != next {
            self.visible.set(next);
        }
    }

    fn set_visible(mut self, next: bool, mode: Signal<Mode>) {
        let request_id = (self.request_id)().wrapping_add(1);
        self.request_id.set(request_id);
        self.preferred_visible.set(next);
        self.visible.set(next);
        self.reflowed_at.set(Some(self.reflow_key()));
        let _ = send(&ExplorerPanelSetVisible {
            visible: next,
            client_id: (self.client_id)(),
            request_id,
        });
        if next {
            return;
        }
        match mode() {
            Mode::Text => focus_file_input(),
            Mode::Dir | Mode::Media(_) => focus_container(),
        }
    }

    pub(crate) fn toggle(mut self, mode: Signal<Mode>) {
        self.user_chose.set(true);
        self.set_visible(!(self.visible)(), mode);
    }

    pub(crate) fn show(mut self, mode: Signal<Mode>) {
        if (self.visible)() {
            return;
        }
        self.user_chose.set(true);
        self.set_visible(true, mode);
    }

    pub(crate) fn reveal_current(mut self, mode: Signal<Mode>) {
        if (self.visible)() {
            let _ = send(&ExplorerRevealCurrent);
            return;
        }
        self.user_chose.set(true);
        self.set_visible(true, mode);
    }

    pub(super) fn show_if_room(self, mode: Signal<Mode>) {
        spawn(async move {
            sleep_ms(0).await;
            if (self.visible)() || !self.has_room() {
                return;
            }
            self.set_visible(true, mode);
        });
    }

    pub(super) fn resize_to(mut self, x: f64) {
        if !(self.resizing)() {
            return;
        }
        let width = (x as i32).max(0) as u32;
        self.width
            .set(width.clamp(EXPLORER_MIN_WIDTH_PX, EXPLORER_MAX_WIDTH_PX));
    }

    fn start_resize(mut self) {
        self.resizing.set(true);
    }

    pub(super) fn finish_resize(mut self) {
        if !(self.resizing)() {
            return;
        }
        self.resizing.set(false);
        let _ = send(&ExplorerPanelWidth { px: (self.width)() });
    }
}

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
    pane: ExplorerPane,
    caret_line: u32,
    view: Signal<SidebarView>,
) -> Element {
    let keys = use_context::<FileKeys>();
    let open = (pane.visible)();
    let panel_width = (pane.width)();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_width_that_will_not_open_the_explorer_does_not_close_it_either() {
        let explorer_width = 240;
        let short = NOTE_MAX_CONTENT_WIDTH_PX + explorer_width - 1;

        assert!(
            ExplorerRoom {
                page_width: short,
                explorer_width,
                open: true,
            }
            .fits()
        );
        assert!(
            !ExplorerRoom {
                page_width: short,
                explorer_width,
                open: false,
            }
            .fits()
        );
    }

    #[test]
    fn the_editor_floor_never_preempts_the_squeeze_tolerance() {
        let explorer_width = 240;
        let auto_closes_below =
            NOTE_MAX_CONTENT_WIDTH_PX + explorer_width - EXPLORER_SQUEEZE_TOLERANCE_PX;
        let floor = explorer_width + EDITOR_MIN_WIDTH_PX;

        assert!(floor < auto_closes_below);
        assert!(
            ExplorerRoom {
                page_width: floor,
                explorer_width,
                open: true,
            }
            .leaves_editor_usable()
        );
        assert!(
            !ExplorerRoom {
                page_width: floor - 1,
                explorer_width,
                open: true,
            }
            .leaves_editor_usable()
        );
    }

    #[test]
    fn the_row_settles_even_when_what_it_measures_follows_the_panel_it_renders() {
        let explorer_width = 240;
        let grip = 4;

        for pane_width in [480u32, 502, 560, 746, 1496] {
            let mut open = true;
            let mut seen = Vec::new();

            for _ in 0..6 {
                let panel = explorer_width;
                let measured = pane_width.saturating_sub(panel + grip);
                let room = ExplorerRoom {
                    page_width: measured,
                    explorer_width,
                    open,
                };
                open = room.fits() && room.leaves_editor_usable();
                seen.push((measured, open));
            }

            let settled = seen[seen.len() - 1];
            assert!(
                seen[2..].iter().all(|step| *step == settled),
                "a pane of {pane_width} never settled: {seen:?}"
            );
        }
    }

    #[test]
    fn an_explorer_still_opens_at_the_width_it_always_did() {
        let explorer_width = 240;
        let snug = NOTE_MAX_CONTENT_WIDTH_PX + explorer_width;

        assert!(
            ExplorerRoom {
                page_width: snug,
                explorer_width,
                open: false,
            }
            .fits()
        );
        assert!(
            !ExplorerRoom {
                page_width: snug - 1,
                explorer_width,
                open: false,
            }
            .fits()
        );
    }

    #[test]
    fn rapid_toggle_ignores_stale_echoes() {
        let request = ExplorerRequest {
            client_id: 7,
            request_id: 3,
        };

        assert!(!request.accepts(7, 1));
        assert!(!request.accepts(7, 2));
        assert!(request.accepts(7, 3));
        assert!(request.accepts(9, 1));
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
