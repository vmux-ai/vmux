//! The command bar as a floating panel inside another page.
//!
//! The same palette as `vmux://command-bar/`, rendered in the host page's own DOM rather than in a
//! webview of its own. It lived in `vmux_layout::page` for as long as that was the only page
//! hosting it, which made the window shell the implementer of a surface it merely makes room for.

use crate::event::{
    CommandBarOpenEvent, CommandBarPanelActiveEvent, CommandBarPanelCloseEvent,
    LAYOUT_COMMAND_BAR_CLOSE_EVENT, LAYOUT_COMMAND_BAR_OPEN_EVENT, PanelPlacement,
    clamp_panel_placement,
};
use crate::page::{CommandPalette, PaletteVariant};
use dioxus::prelude::*;
use vmux_ui::hooks::{send, use_listener};
use wasm_bindgen::JsCast;

/// Tell the host the panel holds a focused DOM field, so the layout shell takes
/// `CefKeyboardTarget`.
///
/// A missed clear strands the keyboard on the layout shell and no pane can ever reclaim it, so
/// every route in and out goes through `set_open` and this rides along with it.
///
/// It used to hang off `use_drop`, which worked only while the host page mounted the panel
/// conditionally. The component stays mounted now and renders nothing when closed, so unmount no
/// longer coincides with closing; `use_drop` remains only to catch the page going away.
fn set_command_bar_panel_active(active: bool) {
    let _ = send(&CommandBarPanelActiveEvent { active });
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PanelDragMode {
    Move,
    Resize,
}

#[derive(Clone, Copy)]
struct PanelDrag {
    mode: PanelDragMode,
    pointer_x: f64,
    pointer_y: f64,
    start: PanelPlacement,
}

fn apply_panel_drag(drag: PanelDrag, pointer_x: f64, pointer_y: f64) -> PanelPlacement {
    let dx = pointer_x - drag.pointer_x;
    let dy = pointer_y - drag.pointer_y;
    match drag.mode {
        PanelDragMode::Move => PanelPlacement {
            left: drag.start.left + dx,
            top: drag.start.top + dy,
            ..drag.start
        },
        PanelDragMode::Resize => PanelPlacement {
            width: drag.start.width + dx,
            height: drag.start.height + dy,
            ..drag.start
        },
    }
}

fn panel_pointer(event: &Event<PointerData>) -> Option<web_sys::PointerEvent> {
    event.data().downcast::<web_sys::PointerEvent>().cloned()
}

fn panel_card_rect(pointer: &web_sys::PointerEvent) -> Option<PanelPlacement> {
    let target = pointer.target()?.dyn_into::<web_sys::Element>().ok()?;
    let card = target.closest("[data-command-bar-card]").ok().flatten()?;
    let rect = card.get_bounding_client_rect();
    Some(PanelPlacement {
        left: rect.left(),
        top: rect.top(),
        width: rect.width(),
        height: rect.height(),
    })
}

/// The viewport, or `None` when the browser will not report it.
///
/// Never substitute a sentinel: `clamp_panel_placement` would then bound the panel against it and
/// happily let a drag carry the bar off screen, which is the one thing the clamp exists to stop.
fn panel_viewport() -> Option<(f64, f64)> {
    let window = web_sys::window()?;
    let width = window.inner_width().ok()?.as_f64()?;
    let height = window.inner_height().ok()?.as_f64()?;
    (width > 0.0 && height > 0.0).then_some((width, height))
}

fn set_panel_pointer_capture(pointer: &web_sys::PointerEvent, capture: bool) {
    let Some(element) = pointer
        .current_target()
        .and_then(|target| target.dyn_into::<web_sys::Element>().ok())
    else {
        return;
    };
    if capture {
        let _ = element.set_pointer_capture(pointer.pointer_id());
    } else {
        let _ = element.release_pointer_capture(pointer.pointer_id());
    }
}

fn advance_panel_drag(
    drag: Signal<Option<PanelDrag>>,
    mut placement: Signal<Option<PanelPlacement>>,
    event: Event<PointerData>,
) {
    let Some(active) = drag() else {
        return;
    };
    let Some(pointer) = panel_pointer(&event) else {
        return;
    };
    let Some((viewport_width, viewport_height)) = panel_viewport() else {
        return;
    };
    placement.set(Some(clamp_panel_placement(
        apply_panel_drag(active, pointer.client_x() as f64, pointer.client_y() as f64),
        viewport_width,
        viewport_height,
    )));
}

fn finish_panel_drag(mut drag: Signal<Option<PanelDrag>>, event: Event<PointerData>) {
    if drag().is_none() {
        return;
    }
    if let Some(pointer) = panel_pointer(&event) {
        set_panel_pointer_capture(&pointer, false);
    }
    drag.set(None);
}

/// The floating command bar.
///
/// Drag and resize never leave the page: they move a DOM node, so routing them through the ECS
/// would put an IPC round trip and a Bevy frame between the pointer and the pixels. Only the
/// settled rectangle is worth telling the host about.
#[component]
pub fn CommandBarPanel() -> Element {
    // No open/ready/rendered handshake: the host pushes a payload and the panel renders. `open` is
    // the entire lifecycle.
    let mut state = use_signal(CommandBarOpenEvent::default);
    let mut open = use_signal(|| false);
    // Outlives each open, so a closed bar does not forget where it was put. Survives reopen but not
    // an app restart; that would need the host store.
    let mut placement = use_signal(|| None::<PanelPlacement>);
    let mut drag = use_signal(|| None::<PanelDrag>);

    // Every route in and out of the panel goes through here, so the host's view of whether the
    // panel holds the keyboard cannot drift from the panel's own.
    let mut set_open = move |showing: bool| {
        open.set(showing);
        set_command_bar_panel_active(showing);
    };

    let _open_listener =
        use_listener::<CommandBarOpenEvent, _>(LAYOUT_COMMAND_BAR_OPEN_EVENT, move |data| {
            state.set(data);
            set_open(true);
        });
    let _close_listener =
        use_listener::<CommandBarPanelCloseEvent, _>(LAYOUT_COMMAND_BAR_CLOSE_EVENT, move |_| {
            set_open(false)
        });
    use_drop(move || set_command_bar_panel_active(false));

    if !open() {
        return rsx! {};
    }

    let mut begin = move |event: Event<PointerData>, mode: PanelDragMode| {
        event.stop_propagation();
        let Some(pointer) = panel_pointer(&event) else {
            return;
        };
        let Some(start) = panel_card_rect(&pointer) else {
            return;
        };
        set_panel_pointer_capture(&pointer, true);
        drag.set(Some(PanelDrag {
            mode,
            pointer_x: pointer.client_x() as f64,
            pointer_y: pointer.client_y() as f64,
            start,
        }));
        placement.set(Some(start));
    };

    let placed = placement();
    let card_class = if placed.is_some() {
        "absolute"
    } else {
        "absolute left-1/2 top-[15%] w-[576px] max-w-[calc(100vw-32px)] -translate-x-1/2"
    };
    let shell_class = if placed.is_some() {
        "relative flex h-full w-full min-h-0 flex-col overflow-hidden rounded-2xl border border-border bg-background shadow-2xl"
    } else {
        "relative flex w-full flex-col overflow-hidden rounded-2xl border border-border bg-background shadow-2xl"
    };
    let card_style = placed
        .map(|p| {
            format!(
                "left:{}px;top:{}px;width:{}px;height:{}px;",
                p.left, p.top, p.width, p.height
            )
        })
        .unwrap_or_default();

    rsx! {
        div {
            class: "pointer-events-auto fixed inset-0",
            onclick: move |_| set_open(false),
            div {
                class: card_class,
                style: card_style,
                "data-command-bar-card": "",
                onclick: move |e| e.stop_propagation(),
                div {
                    id: "command-bar-shell",
                    class: shell_class,
                    div {
                        class: "flex h-3 w-full shrink-0 cursor-grab items-center justify-center active:cursor-grabbing",
                        onpointerdown: move |e| begin(e, PanelDragMode::Move),
                        onpointermove: move |e| advance_panel_drag(drag, placement, e),
                        onpointerup: move |e| finish_panel_drag(drag, e),
                        onpointercancel: move |e| finish_panel_drag(drag, e),
                        div { class: "h-1 w-8 rounded-full bg-border" }
                    }
                    div {
                        class: "flex min-h-0 flex-1 flex-col",
                        CommandPalette {
                            state: ReadSignal::from(state),
                            variant: PaletteVariant::Modal,
                            on_close: move |_| set_open(false),
                            on_dismiss: move |_| set_open(false),
                            on_activity: move |_| {},
                        }
                    }
                    div {
                        class: "absolute bottom-0 right-0 h-4 w-4 cursor-nwse-resize",
                        onpointerdown: move |e| begin(e, PanelDragMode::Resize),
                        onpointermove: move |e| advance_panel_drag(drag, placement, e),
                        onpointerup: move |e| finish_panel_drag(drag, e),
                        onpointercancel: move |e| finish_panel_drag(drag, e),
                    }
                }
            }
        }
    }
}
