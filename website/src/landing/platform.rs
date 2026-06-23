use dioxus::prelude::*;

use crate::landing::parts::headline;

#[component]
pub fn Platform() -> Element {
    rsx! {
        section {
            "data-tone": "dark",
            class: "relative isolate overflow-hidden px-6 py-28 sm:py-36 text-center bg-bg text-text",
            div { class: "pointer-events-none absolute inset-0 -z-10",
                div { class: "absolute left-1/2 top-1/2 h-80 w-80 -translate-x-1/2 -translate-y-1/2 rounded-full bg-aurora-violet/15 blur-[120px] animate-aurora motion-reduce:animate-none" }
            }
            div { class: "max-w-2xl mx-auto mb-14",
                {headline("Platform", "More", "OS than app.")}
                p { class: "mt-5 text-text-muted leading-relaxed",
                    "An OS-like layer for everything you do — the same space and agents, reshaped to the device in front of you."
                }
            }
            div { class: "flex items-end justify-center gap-4 sm:gap-8",
                div { class: "glass h-40 w-56 rounded-xl animate-float motion-reduce:animate-none",
                    div { class: "p-2 text-xs text-text-muted text-left", "Desktop" }
                }
                div { class: "glass h-52 w-32 rounded-2xl animate-float [animation-delay:-5s] motion-reduce:animate-none",
                    div { class: "p-2 text-xs text-text-muted text-left", "Phone" }
                }
                div { class: "glass h-36 w-44 rounded-xl animate-float [animation-delay:-9s] motion-reduce:animate-none",
                    div { class: "p-2 text-xs text-text-muted text-left", "AR / VR" }
                }
            }
            p { class: "mt-12 text-sm text-text-muted",
                "Today it runs on macOS (lead) and Linux — with a portable core ready to follow."
            }
        }
    }
}
