use super::PaletteSurface;
use super::{CommandPalette, use_command_bar_ui};
use dioxus::prelude::*;
use vmux_api::command_bar::CommandBarPanelRequest;
use vmux_ui::hooks::send;

#[component]
pub fn CommandBarPanel() -> Element {
    let state = use_command_bar_ui();
    use_drop(move || {
        let _ = send(&CommandBarPanelRequest { active: false });
    });

    if !state().open_id.is_open() {
        return rsx! {};
    }

    let close = move || {
        let _ = send(&CommandBarPanelRequest { active: false });
    };

    rsx! {
        div {
            class: "pointer-events-auto fixed inset-0",
            onclick: move |_| close(),
            div {
                class: "absolute left-1/2 top-1/2 w-[576px] max-w-[calc(100vw-32px)] -translate-x-1/2 -translate-y-1/2",
                "data-command-bar-card": "",
                onclick: move |e| e.stop_propagation(),
                div {
                    id: "command-bar-shell",
                    class: "relative flex w-full flex-col overflow-hidden rounded-2xl border border-border bg-background shadow-2xl",
                    div {
                        class: "flex min-h-0 flex-1 flex-col",
                        CommandPalette {
                            state: ReadSignal::from(state),
                            surface: PaletteSurface::Modal,
                            on_close: move |_| close(),
                            on_activity: move |_| {},
                        }
                    }
                }
            }
        }
    }
}
