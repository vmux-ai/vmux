use dioxus::prelude::*;

#[component]
pub fn LayoutScene() -> Element {
    rsx! {
        section { class: "relative min-h-[280vh]", "data-scene": "1",
            div { class: "sticky top-0 h-screen flex flex-col items-center justify-center px-6 overflow-hidden",
                div { class: "max-w-2xl text-center mb-10 reveal",
                    p { class: "text-sm uppercase tracking-[0.2em] text-accent mb-3", "Layout" }
                    h2 { class: "text-3xl sm:text-5xl font-bold tracking-tight mb-4",
                        "Browser simplicity, tmux power."
                    }
                    p { class: "text-text-muted leading-relaxed",
                        "At first glance it's the browser you expect. Underneath sits a malleable, tmux-inspired UI — split, stack, and tile any layout you imagine."
                    }
                }
                div {
                    class: "relative w-full max-w-4xl aspect-video rounded-xl border border-border bg-surface/80 backdrop-blur overflow-hidden shadow-2xl",
                    style: "transform: translateY(calc((1 - var(--p, 1)) * 28px))",
                    div { class: "flex h-8 items-center gap-1.5 border-b border-border px-3",
                        span { class: "h-2.5 w-2.5 rounded-full bg-border" }
                        span { class: "h-2.5 w-2.5 rounded-full bg-border" }
                        span { class: "h-2.5 w-2.5 rounded-full bg-border" }
                    }
                    div { class: "flex h-[calc(100%-2rem)] gap-1 p-1",
                        div { class: "flex-1 rounded-md bg-aurora-cyan/10 border border-aurora-cyan/20" }
                        div { class: "flex flex-col gap-1 overflow-hidden basis-1/2",
                            div { class: "flex-1 rounded-md bg-accent/10 border border-accent/20" }
                            div {
                                class: "flex-1 rounded-md bg-aurora-violet/10 border border-aurora-violet/20 font-mono text-[10px] text-text-muted p-2",
                                style: "opacity: var(--p, 1); transform: translateY(calc((1 - var(--p, 1)) * 16px))",
                                "$ vmux split"
                            }
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

struct Tier {
    rank: &'static str,
    title: &'static str,
    body: &'static str,
}

const TIERS: &[Tier] = &[
    Tier {
        rank: "01",
        title: "Talk or type",
        body: "Direct the whole workspace in natural language. Type for precision, talk for hands-free speed.",
    },
    Tier {
        rank: "02",
        title: "Keyboard shortcuts",
        body: "Chrome-style for browsing, tmux-style <leader> commands for layout. High velocity, near-zero learning curve.",
    },
    Tier {
        rank: "03",
        title: "Mouse",
        body: "Plain, intuitive point-and-click that keeps everything grounded in predictable browser behavior.",
    },
];

#[component]
pub fn InputScene() -> Element {
    rsx! {
        section { class: "relative min-h-[280vh]", "data-scene": "1",
            div { class: "sticky top-0 h-screen flex flex-col items-center justify-center px-6",
                div { class: "max-w-2xl text-center mb-10 reveal",
                    p { class: "text-sm uppercase tracking-[0.2em] text-accent mb-3", "Input" }
                    h2 { class: "text-3xl sm:text-5xl font-bold tracking-tight mb-4",
                        "Talk, type, click."
                    }
                    p { class: "text-text-muted leading-relaxed",
                        "Interaction ordered from abstract delegation down to mechanical control."
                    }
                }
                div { class: "w-full max-w-xl flex flex-col gap-4",
                    for (i , t) in TIERS.iter().enumerate() {
                        div {
                            class: "flex items-start gap-4 rounded-xl border border-white/10 bg-white/5 backdrop-blur p-5",
                            style: "opacity: clamp(0, calc((var(--p, 1) - {i}*0.18) * 4), 1); transform: translateY(calc((1 - clamp(0, (var(--p, 1) - {i}*0.18) * 4, 1)) * 24px))",
                            span { class: "text-accent font-mono text-sm pt-0.5", "{t.rank}" }
                            div {
                                h3 { class: "font-semibold mb-1", "{t.title}" }
                                p { class: "text-sm text-text-muted leading-relaxed", "{t.body}" }
                            }
                        }
                    }
                }
            }
        }
    }
}
