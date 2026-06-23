use dioxus::prelude::*;

use crate::landing::parts::{editor_pane, headline, terminal_pane, website_pane};

const TOOLS: &[&str] = &[
    "vmux_browser_navigate",
    "vmux_run",
    "vmux_read_layout",
    "vmux_update_layout",
    "vmux_create_space",
];

#[component]
pub fn Ide() -> Element {
    rsx! {
        section { class: "relative min-h-[300vh]", "data-scene": "1", "data-tone": "dark",
            div { class: "sticky top-0 h-screen overflow-hidden flex flex-col items-center justify-center px-6 bg-bg text-text",
                div { class: "pointer-events-none absolute inset-0 -z-10",
                    div { class: "absolute left-1/2 top-1/2 h-[36rem] w-[36rem] -translate-x-1/2 -translate-y-1/2 rounded-full bg-accent/25 blur-[140px] animate-aurora motion-reduce:animate-none" }
                    div { class: "absolute left-[20%] top-1/3 h-72 w-72 rounded-full bg-aurora-cyan/20 blur-[120px] animate-aurora [animation-delay:-6s] motion-reduce:animate-none" }
                    div { class: "absolute right-[18%] bottom-1/4 h-72 w-72 rounded-full bg-aurora-violet/25 blur-[120px] animate-aurora [animation-delay:-11s] motion-reduce:animate-none" }
                }
                div { class: "max-w-2xl text-center mb-8",
                    {headline("The reveal", "Then it", "splits into an IDE.")}
                    p { class: "mt-4 text-text-muted reveal", "Browser simplicity, tmux power." }
                }
                div { class: "w-full max-w-4xl", style: "perspective: 1600px",
                    div {
                        "data-tilt": "1",
                        class: "relative w-full aspect-video rounded-xl glass [transform-style:preserve-3d] will-change-transform",
                        style: "transform: rotateX(calc(var(--rx, 0) * -7deg)) rotateY(calc(var(--ry, 0) * 11deg + (var(--p, 0) - 0.4) * 16deg))",
                        div { class: "flex h-8 items-center gap-1.5 border-b border-white/10 px-3",
                            span { class: "h-2.5 w-2.5 rounded-full bg-white/15" }
                            span { class: "h-2.5 w-2.5 rounded-full bg-white/15" }
                            span { class: "h-2.5 w-2.5 rounded-full bg-white/15" }
                        }
                        div { class: "flex h-[calc(100%-2rem)] gap-2 p-2 [transform-style:preserve-3d]",
                            div {
                                class: "flex-[1.5]",
                                style: "transform: translateZ(calc(var(--p, 0) * 30px))",
                                {website_pane()}
                            }
                            div { class: "flex-1 flex flex-col gap-2 [transform-style:preserve-3d]",
                                div {
                                    class: "flex-1",
                                    style: "transform: translateZ(calc(var(--p, 0) * 60px))",
                                    {editor_pane()}
                                }
                                div {
                                    class: "flex-1",
                                    style: "transform: translateZ(calc(var(--p, 0) * 90px))",
                                    {terminal_pane()}
                                }
                            }
                        }
                    }
                }
                div { class: "mt-8 flex flex-wrap justify-center gap-2 reveal",
                    for t in TOOLS.iter() {
                        span {
                            key: "{t}",
                            class: "font-mono text-[11px] px-2 py-1 rounded-md border border-accent/25 bg-accent/10 text-text-muted shadow-lg shadow-accent/10",
                            "{t}"
                        }
                    }
                }
            }
        }
    }
}
