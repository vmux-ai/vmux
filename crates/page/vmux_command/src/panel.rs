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
use dioxus::prelude::InteractionLocation;
use dioxus::prelude::*;
use vmux_ui::hooks::{send, use_listener};

/// Tell the host the panel holds a focused DOM field, so the layout shell takes
/// `KeyboardOwner`.
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

/// Where a drag started from, held for as long as it runs.
#[derive(Clone, Copy)]
struct DragOrigin {
    mode: PanelDragMode,
    pointer_x: f64,
    pointer_y: f64,
    start: PanelPlacement,
}

/// The panel's drag state, and the rectangle it produces.
///
/// A hook rather than a pair of signals the component wires together: the order the legs run in —
/// begin, advance, finish — is the whole of the behaviour, and leaving it to the caller is what
/// made a missed `finish` possible.
#[derive(Clone, Copy)]
struct PanelDrag {
    origin: Signal<Option<DragOrigin>>,
    /// Outlives each drag, and each open: a closed bar does not forget where it was put. Survives
    /// reopen but not an app restart; that would need the host store.
    placement: Signal<Option<PanelPlacement>>,
}

fn use_panel_drag() -> PanelDrag {
    PanelDrag {
        origin: use_signal(|| None),
        placement: use_signal(|| None),
    }
}

impl PanelDrag {
    /// Where the bar sits, or `None` while it is still where it opened.
    fn placement(&self) -> Option<PanelPlacement> {
        (self.placement)()
    }

    /// Take the pointer that pressed a handle, and start moving or resizing from it.
    fn begin(&mut self, event: Event<PointerData>, mode: PanelDragMode) {
        event.stop_propagation();
        let Some(start) = panel_card_rect(&event) else {
            return;
        };
        let (pointer_x, pointer_y) = panel_pointer_at(&event);

        self.origin.set(Some(DragOrigin {
            mode,
            pointer_x,
            pointer_y,
            start,
        }));
        self.placement.set(Some(start));
    }

    /// The move and end legs, mounted on the backdrop only while a drag is under way.
    ///
    /// The backdrop covers the viewport, so a drag that leaves the small grab handle keeps being
    /// tracked without capturing the pointer. That is also why these cannot simply stay written on
    /// it: the interpreter registers every bubbling listener on the page root, so a declared
    /// `pointermove` makes *every* pointer move over the window dispatch — and with the page hosted
    /// natively each one is a synchronous XHR the web content blocks on until a frame ends.
    fn listeners(&self) -> Vec<Attribute> {
        // A read, not a `peek`: the backdrop has to re-render when `begin` sets the origin, or the
        // legs never mount and the bar cannot be moved.
        if self.origin.read().is_none() {
            return Vec::new();
        }

        let mut advancing = *self;
        let mut finishing = *self;
        let mut cancelling = *self;
        vec![
            dioxus_elements::events::onpointermove(move |event| advancing.advance(event)),
            dioxus_elements::events::onpointerup(move |_| finishing.finish()),
            dioxus_elements::events::onpointercancel(move |_| cancelling.finish()),
        ]
    }

    fn advance(&mut self, event: Event<PointerData>) {
        let Some(origin) = (self.origin)() else {
            return;
        };
        let Some((viewport_width, viewport_height)) = panel_viewport() else {
            return;
        };
        let (x, y) = panel_pointer_at(&event);

        self.placement.set(Some(clamp_panel_placement(
            origin.apply(x, y),
            viewport_width,
            viewport_height,
        )));
    }

    fn finish(&mut self) {
        self.origin.set(None);
    }
}

impl DragOrigin {
    fn apply(self, pointer_x: f64, pointer_y: f64) -> PanelPlacement {
        let dx = pointer_x - self.pointer_x;
        let dy = pointer_y - self.pointer_y;
        match self.mode {
            PanelDragMode::Move => PanelPlacement {
                left: self.start.left + dx,
                top: self.start.top + dy,
                ..self.start
            },
            PanelDragMode::Resize => PanelPlacement {
                width: self.start.width + dx,
                height: self.start.height + dy,
                ..self.start
            },
        }
    }
}

/// Where the pointer is, in client coordinates.
///
/// Reads Dioxus's own `PointerData` rather than downcasting to a `web_sys::PointerEvent`, which
/// answers `None` off the web and would take the drag with it.
fn panel_pointer_at(event: &Event<PointerData>) -> (f64, f64) {
    let point = event.data().client_coordinates();
    (point.x, point.y)
}

/// The card's rectangle when a drag starts, or `None` when it cannot be measured.
///
/// `web` only. Reading it needs `closest()` and `getBoundingClientRect` on the event target, and
/// natively there is no element to ask — `MountedData` answers `NotSupported` until a
/// `RenderedElementBacking` exists. `None` leaves the panel undraggable rather than draggable to
/// the wrong place; everything else about the bar works.
fn panel_card_rect(_event: &Event<PointerData>) -> Option<PanelPlacement> {
    None
}

/// The viewport, or `None` when it cannot be read.
///
/// Never substitute a sentinel: `clamp_panel_placement` would then bound the panel against it and
/// happily let a drag carry the bar off screen, which is the one thing the clamp exists to stop.
fn panel_viewport() -> Option<(f64, f64)> {
    None
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
    let mut drag = use_panel_drag();

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

    let placed = drag.placement();
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
            ..drag.listeners(),
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
                        onpointerdown: move |e| drag.begin(e, PanelDragMode::Move),
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
                        onpointerdown: move |e| drag.begin(e, PanelDragMode::Resize),
                    }
                }
            }
        }
    }
}
