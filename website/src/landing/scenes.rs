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
                    class: "w-full max-w-4xl animate-float motion-reduce:animate-none",
                    style: "perspective: 1600px",
                    div {
                        "data-tilt": "1",
                        class: "relative w-full aspect-video rounded-xl border border-white/10 bg-surface/70 backdrop-blur shadow-[0_50px_140px_-30px_rgba(0,0,0,0.9)] [transform-style:preserve-3d] will-change-transform",
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
                p { class: "mt-8 text-sm text-text-muted",
                    "Flip the same panes into a live 3D scene, still interactive."
                }
            }
        }
    }
}

fn website_pane() -> Element {
    rsx! {
        div { class: "h-full w-full overflow-hidden rounded-md border border-aurora-cyan/25 bg-[#0b1418] shadow-xl shadow-aurora-cyan/30 flex flex-col",
            div { class: "flex items-center gap-1.5 px-2 py-1.5 border-b border-white/5",
                span { class: "h-1.5 w-1.5 rounded-full bg-aurora-cyan/50" }
                div { class: "ml-1 h-2 w-24 rounded-full bg-white/8" }
                div { class: "ml-auto flex gap-1",
                    div { class: "h-1.5 w-6 rounded bg-white/8" }
                    div { class: "h-1.5 w-6 rounded bg-white/8" }
                }
            }
            div { class: "flex-1 p-3 flex flex-col gap-2",
                div { class: "h-3.5 w-3/5 rounded bg-white/20" }
                div { class: "h-1.5 w-full rounded bg-white/8" }
                div { class: "h-1.5 w-5/6 rounded bg-white/8" }
                div { class: "mt-1 h-4 w-16 rounded-md bg-aurora-cyan/50" }
                div { class: "mt-auto grid grid-cols-3 gap-2",
                    div { class: "h-9 rounded-md bg-white/5 border border-white/5" }
                    div { class: "h-9 rounded-md bg-white/5 border border-white/5" }
                    div { class: "h-9 rounded-md bg-white/5 border border-white/5" }
                }
            }
        }
    }
}

fn editor_pane() -> Element {
    rsx! {
        div { class: "h-full w-full overflow-hidden rounded-md border border-accent/25 bg-[#0d0d18] shadow-xl shadow-accent/30 flex flex-col font-mono text-[9px] leading-[1.5]",
            div { class: "flex items-center gap-2 px-2 py-1 border-b border-white/5 text-white/30",
                span { class: "px-1.5 py-0.5 rounded bg-white/8 text-text/80", "main.rs" }
                span { "lib.rs" }
            }
            div { class: "flex-1 flex overflow-hidden",
                div { class: "px-1.5 py-1.5 text-right text-white/15 select-none",
                    for n in 1..=6 {
                        div { key: "{n}", "{n}" }
                    }
                }
                div { class: "flex-1 py-1.5 pr-2 whitespace-nowrap",
                    div {
                        span { class: "text-aurora-violet", "fn " }
                        span { class: "text-accent", "main" }
                        span { class: "text-white/50", "() {{" }
                    }
                    div { class: "pl-3",
                        span { class: "text-aurora-violet", "let " }
                        span { class: "text-text", "app" }
                        span { class: "text-white/50", " = " }
                        span { class: "text-aurora-cyan", "Vmux::new" }
                        span { class: "text-white/50", "();" }
                    }
                    div { class: "pl-3",
                        span { class: "text-text", "app" }
                        span { class: "text-white/50", "." }
                        span { class: "text-accent", "split" }
                        span { class: "text-white/50", "(Dir::" }
                        span { class: "text-aurora-cyan", "Right" }
                        span { class: "text-white/50", ");" }
                    }
                    div { class: "pl-3",
                        span { class: "text-text", "app" }
                        span { class: "text-white/50", "." }
                        span { class: "text-accent", "run" }
                        span { class: "text-white/50", "();" }
                    }
                    div {
                        span { class: "text-white/50", "}}" }
                    }
                }
            }
        }
    }
}

fn terminal_pane() -> Element {
    rsx! {
        div { class: "h-full w-full overflow-hidden rounded-md border border-aurora-violet/25 bg-[#120c1a] shadow-xl shadow-aurora-violet/30 p-2 font-mono text-[9px] leading-[1.6] text-white/45",
            div {
                span { class: "text-aurora-violet", "$ " }
                span { class: "text-text", "vmux split" }
            }
            div { class: "text-white/35", "→ pane created" }
            div {
                span { class: "text-aurora-violet", "$ " }
                span { class: "text-text", "cargo run" }
            }
            div { class: "text-aurora-cyan/70", "  Compiling vmux v0.1.0" }
            div {
                span { class: "text-aurora-violet", "$ " }
                span { class: "inline-block w-1.5 h-2.5 bg-text/70 align-middle animate-pulse" }
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
        section { class: "relative max-w-3xl mx-auto px-6 py-24 sm:py-32",
            div { class: "text-center mb-12 reveal",
                p { class: "text-sm uppercase tracking-[0.2em] text-accent mb-3", "Input" }
                h2 { class: "text-3xl sm:text-5xl font-bold tracking-tight mb-4",
                    "Talk, type, click."
                }
                p { class: "text-text-muted leading-relaxed",
                    "Interaction ordered from abstract delegation down to mechanical control."
                }
            }
            div { class: "flex flex-col gap-4 reveal",
                for (i , t) in TIERS.iter().enumerate() {
                    div {
                        key: "{t.rank}",
                        class: "relative flex items-start gap-4 rounded-xl border border-white/10 bg-white/5 backdrop-blur p-5 pl-6 overflow-hidden",
                        div {
                            class: "absolute left-0 top-0 h-full w-1 bg-accent animate-cue motion-reduce:animate-none",
                            style: "animation-delay: {i}s",
                        }
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
