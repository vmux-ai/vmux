#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_command::event::CommandBarOpenEvent;
use vmux_ui::components::start_hero::{START_BACKDROP_STYLE, StartBackdrop, StartHero};
use vmux_ui::hooks::{send, use_event, use_listener, use_theme};
use vmux_ui::i18n::translate;

use crate::start::event::{
    START_COMMAND_BAR_OPEN_EVENT, START_FOCUS_INPUT_EVENT, StartDataRequest, StartFocusInput,
};
use crate::start::focus::StartFocus;
use vmux_command::page::{CommandPalette, PaletteVariant, StartInlineTransition};

/// The `vmux://start/` launcher page: a cinematic centered hero that requests its
/// entries on mount and renders [`CommandPalette`] in [`PaletteVariant::Start`].
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
        StartFocus::request();
    });

    use_effect(move || {
        locale();
        let _ = send(&StartDataRequest);
        StartFocus::claim_on_mount();
        mounted.set(true);
    });

    use_effect(|| StartFocus::install());

    rsx! {
        document::Title { {translate("start-title")} }
        main {
            class: "relative isolate flex h-screen items-center justify-center overflow-hidden bg-background px-4 text-foreground sm:px-6",
            style: START_BACKDROP_STYLE,
            StartBackdrop {}
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

/// Disable launcher-only focus capture before switching this document to an agent page.
pub fn begin_agent_transition() {
    StartFocus::release_for_agent_transition();
}
