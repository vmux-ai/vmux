#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_api::error::ErrorPageData;
use vmux_ui::hooks::use_theme;
use vmux_ui::i18n::translate;

#[vmux_page::page(
    page = "error",
    component = Page,
    states = ["ThemeUiState"],
    takes = vmux_api::error::ErrorPageData
)]
pub struct ErrorPage;

#[component]
pub fn Page() -> Element {
    use_theme();
    let failure = use_hook(|| try_consume_context::<ErrorPageData>().unwrap_or_default());

    let title = if failure.title_message_id.is_empty() {
        translate("error-title")
    } else {
        translate(&failure.title_message_id)
    };

    rsx! {
        div { class: "flex h-full min-h-0 items-center justify-center bg-background p-10 text-foreground",
            section { class: "max-w-[640px]",
                h1 { class: "mb-3 text-[28px] font-semibold leading-tight", "{title}" }
                if !failure.message.is_empty() {
                    p { class: "text-sm leading-relaxed text-muted-foreground", "{failure.message}" }
                }
                if !failure.url.is_empty() {
                    code { class: "mt-4 block whitespace-pre-wrap break-words rounded-md bg-card p-3 text-sm text-foreground",
                        "{failure.url}"
                    }
                }
            }
        }
    }
}
