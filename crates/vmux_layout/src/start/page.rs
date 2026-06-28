#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_command::event::{COMMAND_BAR_OPEN_EVENT, CommandBarOpenEvent};
use vmux_ui::hooks::{try_cef_bin_emit_rkyv, use_bin_event_listener, use_theme};
use wasm_bindgen::JsCast;

use crate::command_bar::palette::{CommandPalette, PaletteVariant};
use crate::start::event::{START_FOCUS_INPUT_EVENT, StartDataRequest, StartFocusInput};

/// The `vmux://start/` launcher page: a cinematic centered hero that requests its
/// entries on mount and renders [`CommandPalette`] in [`PaletteVariant::Start`].
#[component]
pub fn Page() -> Element {
    use_theme();
    let mut state = use_signal(CommandBarOpenEvent::default);
    let mut mounted = use_signal(|| false);

    let _open_listener =
        use_bin_event_listener::<CommandBarOpenEvent, _>(COMMAND_BAR_OPEN_EVENT, move |data| {
            state.set(data);
        });

    let _focus_listener =
        use_bin_event_listener::<StartFocusInput, _>(START_FOCUS_INPUT_EVENT, move |_| {
            focus_start_input();
        });

    use_effect(move || {
        if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
            doc.set_title("Start");
        }
        let _ = try_cef_bin_emit_rkyv(&StartDataRequest);
        mounted.set(true);
    });

    let reveal = if mounted() {
        "opacity-100 blur-0 translate-y-0"
    } else {
        "opacity-0 blur-sm translate-y-4"
    };

    rsx! {
        main {
            class: "relative isolate flex min-h-screen flex-col items-center justify-center overflow-hidden bg-background px-6 text-foreground",
            style: "background-image:radial-gradient(140% 100% at 50% -12%, rgba(129,140,248,0.13), transparent 55%), radial-gradient(120% 90% at 50% 116%, rgba(34,211,238,0.06), transparent 60%);",
            div { class: "pointer-events-none absolute inset-0 -z-10 overflow-hidden",
                div { class: "absolute left-1/2 top-[16%] h-[36rem] w-[36rem] -translate-x-1/2 rounded-full bg-indigo-500/15 blur-[150px]" }
                div { class: "absolute left-[12%] top-1/3 h-80 w-80 rounded-full bg-cyan-400/10 blur-[130px]" }
                div { class: "absolute right-[12%] top-1/4 h-80 w-80 rounded-full bg-violet-500/12 blur-[130px]" }
                div { class: "absolute inset-x-0 bottom-0 h-1/3 bg-gradient-to-t from-black/40 to-transparent" }
            }
            div {
                class: "relative flex w-full max-w-2xl flex-col items-center gap-8 transition-all duration-700 ease-out motion-reduce:transition-none {reveal}",
                div { class: "flex flex-col items-center gap-2",
                    h1 { class: "bg-gradient-to-b from-white to-white/55 bg-clip-text text-6xl font-semibold leading-none tracking-tight text-transparent",
                        "vmux"
                    }
                    p { class: "text-base text-muted-foreground", "One prompt. Anything, done." }
                }
                div { class: "w-full overflow-hidden rounded-2xl bg-white/[0.05] ring-1 ring-inset ring-white/10 shadow-[0_40px_120px_-32px_rgba(0,0,0,0.85)] backdrop-blur-2xl",
                    CommandPalette {
                        state,
                        variant: PaletteVariant::Start,
                        on_close: move |_| {},
                        on_dismiss: move |_| {},
                        on_activity: move |_| {},
                    }
                }
            }
        }
    }
}

fn focus_start_input() {
    let Some(el) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id("command-bar-input"))
    else {
        return;
    };
    let input: web_sys::HtmlInputElement = el.unchecked_into();
    let _ = input.focus();
    let len = input.value().len() as u32;
    let _ = input.set_selection_range(0, len);
}
