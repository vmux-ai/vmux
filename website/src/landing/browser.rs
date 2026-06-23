use dioxus::prelude::*;

use crate::landing::parts::{browser_frame, headline, icon_globe, icon_mic, icon_search, tab};

#[component]
pub fn Browser() -> Element {
    rsx! {
        section {
            "data-tone": "light",
            class: "relative isolate min-h-screen overflow-hidden flex flex-col items-center justify-center px-6 bg-bg text-text",
            div { class: "pointer-events-none absolute inset-0 -z-10",
                div { class: "absolute left-1/2 top-1/2 h-[30rem] w-[30rem] -translate-x-1/2 -translate-y-1/2 rounded-full bg-aurora-cyan/20 blur-[130px] animate-aurora motion-reduce:animate-none" }
            }
            div { class: "mx-auto max-w-2xl text-center mb-12",
                {headline("Familiar on the surface", "You already", "know how.")}
                p { class: "mt-5 text-base sm:text-lg text-text-muted reveal",
                    "It looks and acts like a standard web browser. No learning curve — everyone already knows how to use it."
                }
            }
            div {
                class: "w-full max-w-3xl reveal",
                style: "transition-delay: 120ms; transform: translateY(calc(var(--sy, 0) * -0.02px))",
                {browser_frame(
                    "glass border-aurora-cyan/30 h-[56vh] min-h-[20rem]",
                    rsx! {
                        {tab(icon_globe("h-3 w-3 text-aurora-cyan"), "New Tab", true)}
                        {tab(icon_globe("h-3 w-3"), "docs", false)}
                    },
                    "Search or enter address",
                    rsx! {
                        div { class: "flex h-full flex-col items-center justify-center p-6",
                            div { class: "flex w-full items-center gap-3 rounded-full border border-text-muted/25 bg-surface/70 px-5 py-3.5",
                                {icon_search("h-4 w-4 shrink-0 text-text-muted")}
                                span { class: "flex-1 text-left text-[13px] text-text-muted", "Search the web" }
                                {icon_mic("h-4 w-4 shrink-0 text-aurora-cyan")}
                            }
                        }
                    },
                )}
            }
        }
    }
}
