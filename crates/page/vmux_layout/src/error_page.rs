#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_ui::hooks::use_theme;
use vmux_ui::i18n::translate;
use vmux_wire::error::ErrorPageData;

/// What is shown where a page failed to open.
///
/// The failure arrives as a root context rather than over IPC: the host builds this page's
/// `VirtualDom` and already knows what went wrong, so there is nothing to ask for and no first
/// render with nothing to show.
#[component]
pub fn Page() -> Element {
    use_theme();
    let failure = use_hook(|| try_consume_context::<ErrorPageData>().unwrap_or_default());

    let title = match failure.heading_message_id() {
        Some(id) => translate(id),
        None if failure.title.is_empty() => translate("error-title"),
        None => failure.title.clone(),
    };

    rsx! {
        document::Title { "{title}" }
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
