use dioxus::prelude::*;

#[component]
pub fn LayoutScene() -> Element {
    rsx! {
        section { class: "relative min-h-[280vh] [scroll-timeline-name:--layout] [scroll-timeline-axis:block]",
            div { class: "sticky top-0 h-screen flex flex-col items-center justify-center px-6 overflow-hidden",
                div { class: "max-w-2xl text-center mb-10",
                    p { class: "text-sm uppercase tracking-[0.2em] text-accent mb-3", "Layout" }
                    h2 { class: "text-3xl sm:text-5xl font-bold tracking-tight mb-4",
                        "Browser simplicity, tmux power."
                    }
                    p { class: "text-text-muted leading-relaxed",
                        "At first glance it's the browser you expect. Underneath sits a malleable, tmux-inspired UI — split, stack, and tile any layout you imagine."
                    }
                }
                div { class: "relative w-full max-w-4xl aspect-video rounded-xl border border-border bg-surface/80 backdrop-blur overflow-hidden shadow-2xl",
                    div { class: "flex h-8 items-center gap-1.5 border-b border-border px-3",
                        span { class: "h-2.5 w-2.5 rounded-full bg-border" }
                        span { class: "h-2.5 w-2.5 rounded-full bg-border" }
                        span { class: "h-2.5 w-2.5 rounded-full bg-border" }
                    }
                    div { class: "flex h-[calc(100%-2rem)] gap-1 p-1",
                        div { class: "flex-1 rounded-md bg-aurora-cyan/10 border border-aurora-cyan/20" }
                        div {
                            class: "flex flex-col gap-1 overflow-hidden [animation:scene-split_linear_both] [animation-timeline:--layout] [animation-range:entry_30%_cover_60%] supports-[animation-timeline:scroll()]:basis-1/2 basis-1/2 motion-reduce:basis-1/2",
                            div { class: "flex-1 rounded-md bg-accent/10 border border-accent/20" }
                            div { class: "flex-1 rounded-md bg-aurora-violet/10 border border-aurora-violet/20 font-mono text-[10px] text-text-muted p-2",
                                "$ vmux split" }
                        }
                    }
                }
                p { class: "mt-6 text-sm text-text-muted",
                    "Flip the same panes into a live 3D scene, still interactive."
                }
            }
        }
    }
}
