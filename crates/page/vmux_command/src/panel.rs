use crate::event::{
    CommandBarOpenEvent, CommandBarPanelActiveEvent, CommandBarPanelCloseEvent,
    LAYOUT_COMMAND_BAR_CLOSE_EVENT, LAYOUT_COMMAND_BAR_OPEN_EVENT, PanelPlacement,
    clamp_panel_placement,
};
use crate::page::CommandPalette;
use dioxus::prelude::InteractionLocation;
use dioxus::prelude::*;
use vmux_ui::hooks::{send, use_listener};
use vmux_ui::launcher::palette::PaletteSurface;

fn set_command_bar_panel_active(active: bool) {
    let _ = send(&CommandBarPanelActiveEvent { active });
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PanelDragMode {
    Move,
    Resize,
}

#[derive(Clone, Copy)]
struct DragOrigin {
    mode: PanelDragMode,
    pointer_x: f64,
    pointer_y: f64,
    start: PanelPlacement,
}

#[derive(Clone, Copy)]
struct PanelDrag {
    origin: Signal<Option<DragOrigin>>,
    placement: Signal<Option<PanelPlacement>>,
}

fn use_panel_drag() -> PanelDrag {
    PanelDrag {
        origin: use_signal(|| None),
        placement: use_signal(|| None),
    }
}

impl PanelDrag {
    fn placement(&self) -> Option<PanelPlacement> {
        (self.placement)()
    }

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

    fn listeners(&self) -> Vec<Attribute> {
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

fn panel_pointer_at(event: &Event<PointerData>) -> (f64, f64) {
    let point = event.data().client_coordinates();
    (point.x, point.y)
}

fn panel_card_rect(_event: &Event<PointerData>) -> Option<PanelPlacement> {
    None
}

fn panel_viewport() -> Option<(f64, f64)> {
    None
}

#[component]
pub fn CommandBarPanel() -> Element {
    let mut state = use_signal(CommandBarOpenEvent::default);
    let mut open = use_signal(|| false);
    let mut drag = use_panel_drag();

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
                            surface: PaletteSurface::Modal,
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
