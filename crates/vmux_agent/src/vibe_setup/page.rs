#![allow(non_snake_case)]

use crate::vibe_setup::event::VibeInstallRunRequest;
use dioxus::prelude::*;
use vmux_ui::hooks::{try_cef_bin_emit_rkyv, use_theme};

#[component]
pub fn Page() -> Element {
    use_theme();
    rsx! {
        main { class: "flex min-h-screen items-center justify-center bg-background p-10 text-foreground",
            section { class: "max-w-2xl",
                h1 { class: "mb-3 text-2xl font-semibold leading-tight", "Install Vibe CLI" }
                p { class: "mb-4 text-sm leading-relaxed text-muted-foreground",
                    "Vmux opens this page through the local "
                    code { class: "rounded bg-muted px-1.5 py-0.5 text-foreground", "vibe" }
                    " command. Install Vibe, then run it below."
                }
                code {
                    class: "mb-5 block whitespace-pre-wrap break-words rounded-md bg-muted p-3 text-sm text-foreground",
                    "curl -LsSf https://mistral.ai/vibe/install.sh | bash"
                }
                button {
                    class: "rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground transition-opacity hover:opacity-90",
                    onclick: move |_| {
                        let _ = try_cef_bin_emit_rkyv(&VibeInstallRunRequest);
                    },
                    "Want me to run?"
                }
            }
        }
    }
}
