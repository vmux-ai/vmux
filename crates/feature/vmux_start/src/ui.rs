#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_ui::components::start_hero::{START_BACKDROP_CLASS, StartBackdrop, StartHero};
use vmux_ui::hooks::use_theme;

use vmux_command::{CommandPalette, CommandPaletteSurface, use_command_bar_ui};

#[vmux_page::page(
    component = Page
)]
pub(crate) struct StartPage;

#[component]
pub fn Page() -> Element {
    use_theme();
    let state = use_command_bar_ui();
    let mut mounted = use_signal(|| false);

    use_effect(move || {
        mounted.set(true);
    });

    rsx! {
        main {
            class: "relative isolate flex min-h-0 flex-1 flex-col overflow-y-auto overscroll-contain bg-background px-4 py-6 text-foreground sm:px-6 {START_BACKDROP_CLASS}",
            StartBackdrop {}
            div { class: "m-auto w-full",
                StartHero { revealed: mounted(),
                    div { class: "relative w-full",
                        CommandPalette {
                            state,
                            surface: CommandPaletteSurface::Start,
                            on_close: move |_| {},
                            on_activity: move |_| {},
                        }
                    }
                }
            }
        }
    }
}
