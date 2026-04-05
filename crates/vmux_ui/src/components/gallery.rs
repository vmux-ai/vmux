//! Full UI library gallery (wasm): every widget under [`vmux_ui::components`].

use crate::components::{UiText, UiTextSize, UiTextTone};
use dioxus::prelude::*;

use super::gallery_demos::GalleryDemos;

#[component]
pub fn UiLibraryGallery() -> Element {
    rsx! {
        document::Stylesheet { href: asset!("/assets/input.css") }
        div { class: "min-h-full min-w-0 bg-[linear-gradient(180deg,#16171c_0%,#0e0f12_100%)] text-foreground/90",
            header { class: "sticky top-0 z-10 border-b border-border/60 bg-card/95 px-5 py-4 backdrop-blur-sm",
                div { class: "flex min-w-0 flex-col items-stretch gap-1",
                    UiText { size: UiTextSize::Sm, tone: UiTextTone::Accent, "vmux_ui" }
                    UiText { size: UiTextSize::Xs, tone: UiTextTone::Muted,
                        "Full component gallery (DioxusLabs widgets)"
                    }
                }
            }
            GalleryDemos {}
        }
    }
}
