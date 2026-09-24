#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_command::event::{CommandBarOpenEvent, CommandBarUiState};
use vmux_ui::components::start_hero::{START_BACKDROP_CLASS, StartBackdrop, StartHero};
use vmux_ui::hooks::{send, use_listener, use_theme, use_ui_state, use_ui_state_root};

use crate::event::{StartDataRequest, StartFocusInput};
use vmux_command::ui::{CommandPalette, focus_prompt_input};
use vmux_ui::launcher::palette::PaletteSurface;

#[component]
pub fn Page() -> Element {
    let locale = use_theme();
    let _updates = use_ui_state_root::<CommandBarUiState>();
    let state = use_ui_state::<CommandBarOpenEvent>();
    let mut mounted = use_signal(|| false);

    let _focus_listener = use_listener::<StartFocusInput, _>(move |_| {
        focus_prompt_input();
    });

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

#[component]
pub fn StartPage() -> Element {
    rsx! { Page {} }
}
