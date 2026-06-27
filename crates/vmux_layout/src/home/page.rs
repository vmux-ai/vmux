#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_command::event::{COMMAND_BAR_OPEN_EVENT, CommandBarOpenEvent};
use vmux_ui::components::icon::Icon;
use vmux_ui::hooks::{try_cef_bin_emit_rkyv, use_bin_event_listener, use_theme};

use crate::command_bar::palette::{CommandPalette, PaletteVariant};
use crate::home::event::HomeDataRequest;

#[component]
pub fn Page() -> Element {
    use_theme();
    let mut state = use_signal(CommandBarOpenEvent::default);

    let _open_listener =
        use_bin_event_listener::<CommandBarOpenEvent, _>(COMMAND_BAR_OPEN_EVENT, move |data| {
            state.set(data);
        });

    use_effect(move || {
        let _ = try_cef_bin_emit_rkyv(&HomeDataRequest);
    });

    rsx! {
        main { class: "relative flex min-h-screen flex-col items-center justify-center overflow-hidden bg-background px-6 text-foreground",
            div { class: "pointer-events-none absolute -top-1/3 left-1/2 h-[60vh] w-[60vh] -translate-x-1/2 rounded-full bg-white/[0.05] blur-[120px]" }
            div { class: "relative flex w-full max-w-2xl flex-col items-center gap-8",
                div { class: "flex items-center gap-2.5",
                    Icon { class: "h-8 w-8 text-foreground",
                        path { d: "M15 21v-8a1 1 0 0 0-1-1h-4a1 1 0 0 0-1 1v8" }
                        path { d: "M3 10a2 2 0 0 1 .709-1.528l7-5.999a2 2 0 0 1 2.582 0l7 5.999A2 2 0 0 1 21 10v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" }
                    }
                    span { class: "text-3xl font-semibold tracking-tight", "vmux" }
                }
                div { class: "w-full overflow-hidden rounded-2xl bg-white/[0.04] ring-1 ring-inset ring-white/10 shadow-[0_24px_80px_-24px_rgba(0,0,0,0.7)] backdrop-blur-2xl",
                    CommandPalette {
                        state,
                        variant: PaletteVariant::Home,
                        on_close: move |_| {},
                        on_dismiss: move |_| {},
                        on_activity: move |_| {},
                    }
                }
            }
        }
    }
}
