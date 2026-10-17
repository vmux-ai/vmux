#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_ui::hooks::use_theme;
use vmux_ui::i18n::translate;

use vmux_command::{CommandPalette, CommandPaletteSurface, use_command_bar_ui};

#[vmux_page::page(
    component = Page,
    states = ["ThemeUiState", "CommandBarUiState", "KeyClaimsUiState"]
)]
pub(crate) struct StartPage;

#[component]
pub fn Page() -> Element {
    use_theme();
    let state = use_command_bar_ui();

    rsx! {
        main {
            class: "flex min-h-0 flex-1 flex-col overflow-y-auto overscroll-contain bg-background px-4 py-6 text-foreground sm:px-6",
            div { class: "m-auto flex w-full max-w-2xl flex-col gap-5",
                div { class: "flex flex-col items-center gap-1.5 text-center",
                    h1 { class: "text-2xl font-semibold tracking-tight text-foreground", "vmux" }
                    p { class: "text-sm text-muted-foreground", {translate("start-tagline")} }
                }
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
