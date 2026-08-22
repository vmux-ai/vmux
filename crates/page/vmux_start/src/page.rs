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
        focus_prompt_input();
    });

    // Reading `locale` is the subscription: the host titles the entries, so a language change has
    // to ask for them again rather than re-render what the last language produced.
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
            // Grown by flex rather than sized against the viewport, because a page fills what it
            // was given and does not get to assume it was given the screen — the phone hands it
            // what is left under a status header, which `h-dvh` would overlap. Not `h-full`
            // either: a percentage height needs every ancestor to have resolved one, and where
            // that chain breaks the box collapses to its content and `m-auto` has no room left to
            // centre in. Both hosts put this in a flex column, so growing into it always works.
            class: "relative isolate flex min-h-0 flex-1 flex-col overflow-y-auto overscroll-contain bg-background px-4 py-6 text-foreground sm:px-6",
            style: START_BACKDROP_STYLE,
            StartBackdrop {}
            // `m-auto` rather than `justify-center`: a centred flex item whose content outgrows
            // the container has its overflow clipped above the scroll origin, and auto margins are
            // what centre it while still yielding when there is no room.
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

/// The launcher as a host surface renders it: no props, because a surface names a component and
/// has nothing to hand it.
///
/// The inline agent transition is a `web` affordance — it swaps this document for an agent page in
/// place — and a natively-hosted launcher has no document of its own to swap, so it declines and
/// the pane is replaced the ordinary way.
#[component]
pub fn StartPage() -> Element {
    rsx! {
        Page {}
    }
}
