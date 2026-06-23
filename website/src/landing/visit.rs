use dioxus::prelude::*;

use crate::landing::parts::{
    avatar_bot, avatar_you, browser_frame, headline, icon_bot, icon_globe, tab,
};

const TOOLS: &[&str] = &[
    "vmux_browser_navigate",
    "vmux_run",
    "vmux_read_layout",
    "vmux_update_layout",
];

#[component]
pub fn Visit() -> Element {
    rsx! {
        section { class: "relative min-h-[240vh]", "data-scene": "1", "data-tone": "light",
            div { class: "sticky top-0 h-screen overflow-hidden flex flex-col items-center justify-center px-6 bg-bg text-text",
                div { class: "pointer-events-none absolute inset-0 -z-10",
                    div { class: "absolute left-1/2 top-1/3 h-[30rem] w-[30rem] -translate-x-1/2 rounded-full bg-accent/20 blur-[130px] animate-aurora motion-reduce:animate-none" }
                }
                div { class: "mx-auto max-w-2xl text-center mb-10",
                    {headline("The pivot", "Hit ⌘L.", "Visit an agent.")}
                    p { class: "mt-5 text-base sm:text-lg text-text-muted reveal",
                        "Every agent, terminal, and space lives at its own address — ready to share or jump back to."
                    }
                }
                div {
                    class: "w-full max-w-3xl",
                    "data-tilt": "1",
                    style: "transform: perspective(1600px) rotateX(calc((var(--p,0) - 0.5) * -6deg)) scale(calc(0.96 + var(--p,0) * 0.04))",
                    {browser_frame(
                        "glass border-accent/30 h-[56vh] min-h-[20rem]",
                        rsx! {
                            {tab(icon_bot("h-3 w-3 text-accent"), "vibe", true)}
                            {tab(icon_globe("h-3 w-3"), "example.com", false)}
                        },
                        "vmux://agent/vibe/2c80e7…",
                        rsx! {
                            div { class: "relative h-full",
                                div {
                                    class: "absolute inset-0 flex items-center justify-center p-6",
                                    style: "opacity: calc(1 - min(var(--p,0) * 2.2, 1))",
                                    div { class: "flex w-full items-center gap-3 rounded-full border border-text-muted/25 bg-surface/70 px-5 py-3.5",
                                        span { class: "flex-1 text-left text-[13px] text-text-muted", "Search the web" }
                                    }
                                }
                                div {
                                    class: "absolute inset-0 flex flex-col justify-center gap-3 p-6",
                                    style: "opacity: calc(max(var(--p,0) * 2.2 - 1, 0))",
                                    div { class: "flex items-end gap-2",
                                        {avatar_bot()}
                                        div { class: "rounded-xl rounded-bl-sm bg-surface/80 px-3 py-2 text-[12px] text-text",
                                            "Tests pass — ship it?"
                                        }
                                    }
                                    div { class: "flex items-end justify-end gap-2",
                                        div { class: "rounded-xl rounded-br-sm border border-accent/30 bg-accent/20 px-3 py-2 text-[12px] text-text",
                                            "Ship it."
                                        }
                                        {avatar_you()}
                                    }
                                }
                            }
                        },
                    )}
                }
                div { class: "mt-6 flex flex-wrap justify-center gap-2 reveal",
                    for t in TOOLS.iter() {
                        span {
                            key: "{t}",
                            class: "font-mono text-[11px] px-2 py-1 rounded-md border border-text-muted/20 bg-surface/60 text-text-muted",
                            "{t}"
                        }
                    }
                }
            }
        }
    }
}
