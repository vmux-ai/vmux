use super::{CommandPalette, use_command_bar_ui};
use dioxus::prelude::*;
use vmux_api::command_bar::{DeactivateRequest, DismissRequest};
use vmux_ui::components::popup::Dismissal;
use vmux_ui::hooks::send;

use crate::CommandPaletteSurface;

#[component]
pub fn CommandBarPanel() -> Element {
    let state = use_command_bar_ui();
    let close = EventHandler::new(move |()| {
        let _ = send(&DismissRequest);
    });

    let open = state().snapshot.open_id.is_open();
    Dismissal::use_listener(
        open,
        EventHandler::new(move |()| {
            let _ = send(&DeactivateRequest);
        }),
    );
    if !open {
        return rsx! {};
    }

    rsx! {
        div {
            class: "pointer-events-auto fixed inset-0 bg-scrim/50",
            onmousedown: move |event| {
                event.prevent_default();
                close.call(());
            },
            div {
                class: "absolute left-1/2 top-1/2 w-[576px] max-w-[calc(100vw-32px)] -translate-x-1/2 -translate-y-1/2",
                "data-command-bar-card": "",
                onmousedown: move |event| event.stop_propagation(),
                div {
                    id: "command-bar-shell",
                    class: "relative flex w-full flex-col overflow-hidden rounded-2xl border border-foreground/15 bg-popover shadow-[0_24px_80px_-16px_rgb(0_0_0_/_55%),0_8px_24px_rgb(0_0_0_/_25%)]",
                    div {
                        class: "flex min-h-0 flex-1 flex-col",
                        CommandPalette {
                            state: ReadSignal::from(state),
                            surface: CommandPaletteSurface::Modal,
                            on_close: close,
                            on_activity: move |_| {},
                        }
                    }
                }
            }
        }
    }
}
