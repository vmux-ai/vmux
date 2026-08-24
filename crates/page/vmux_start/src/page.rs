#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_command::event::CommandBarOpenEvent;
use vmux_ui::components::start_hero::{START_BACKDROP_STYLE, StartBackdrop, StartHero};
use vmux_ui::hooks::{send, use_event, use_listener, use_theme};

use crate::event::{
    START_COMMAND_BAR_OPEN_EVENT, START_FOCUS_INPUT_EVENT, StartDataRequest, StartFocusInput,
};
use vmux_command::page::{
    CommandPalette, PaletteVariant, StartInlineTransition, focus_prompt_input,
};

#[component]
pub fn Page(
    #[props(default)] on_inline_transition: Option<EventHandler<StartInlineTransition>>,
) -> Element {
    let locale = use_theme();
    let state = use_event::<CommandBarOpenEvent>(
        START_COMMAND_BAR_OPEN_EVENT,
        CommandBarOpenEvent::default,
    );
    let mut mounted = use_signal(|| false);

    let _focus_listener = use_listener::<StartFocusInput, _>(START_FOCUS_INPUT_EVENT, move |_| {
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
            class: "relative isolate flex min-h-0 flex-1 flex-col overflow-y-auto overscroll-contain bg-background px-4 py-6 text-foreground sm:px-6",
            style: START_BACKDROP_STYLE,
            StartBackdrop {}
            div { class: "m-auto w-full",
                StartHero { revealed: mounted(),
                    div { class: "relative w-full",
                        CommandPalette {
                            state,
                            variant: PaletteVariant::Start,
                            on_close: move |_| {},
                            on_dismiss: move |_| {},
                            on_activity: move |_| {},
                            on_start_inline_transition: on_inline_transition,
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub fn StartPage() -> Element {
    rsx! {
        Page {}
    }
}
