use super::{CommandPalette, use_command_bar_ui};
use dioxus::prelude::*;
use vmux_api::command_bar::CommandBarPanelRequest;
use vmux_ui::hooks::send;

use crate::CommandPaletteSurface;

#[component]
pub fn CommandBarPanel() -> Element {
    let state = use_command_bar_ui();
    let close = EventHandler::new(move |()| {
        let _ = send(&CommandBarPanelRequest { active: false });
    });
    use_effect(move || {
        if state().open_id.is_open() {
            let _ = send(&CommandBarPanelRequest { active: true });
        }
    });
    use_drop(move || close.call(()));

    if !state().open_id.is_open() {
        return rsx! {};
    }

    rsx! {
        div {
            class: "pointer-events-auto fixed inset-0",
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
                    class: "relative flex w-full flex-col overflow-hidden rounded-2xl border border-border bg-background shadow-2xl",
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
