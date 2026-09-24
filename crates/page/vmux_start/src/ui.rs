#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_ui::components::start_hero::{START_BACKDROP_CLASS, StartBackdrop, StartHero};
use vmux_ui::hooks::{send, use_theme};

use crate::event::StartDataRequest;
use vmux_command::ui::{CommandPalette, focus_prompt_input, use_command_bar_ui};
use vmux_ui::launcher::palette::PaletteSurface;

#[vmux_native::page(
    url = crate::START_PAGE_URL,
    title = "Start",
    component = Page,
    document_url = crate::START_PAGE_URL
)]
pub(crate) struct StartPage;

#[component]
pub fn Page() -> Element {
    let locale = use_theme();
    let state = use_command_bar_ui();
    let mut mounted = use_signal(|| false);

    use_effect(move || {
        locale();
        let _ = send(&StartDataRequest);
    });

    use_effect(move || {
        focus_prompt_input();
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
                            surface: PaletteSurface::Start,
                            on_close: move |_| {},
                            on_dismiss: move |_| {},
                            on_activity: move |_| {},
                        }
                    }
                }
            }
        }
    }
}
