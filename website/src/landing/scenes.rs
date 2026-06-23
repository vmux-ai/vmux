use dioxus::prelude::*;

use crate::landing::parts::{editor_pane, terminal_pane, website_pane};

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

#[derive(Clone, Copy)]
enum InputArt {
    Talk,
    Type,
    Click,
}

struct Tier {
    rank: &'static str,
    title: &'static str,
    body: &'static str,
    art: InputArt,
}

const TIERS: &[Tier] = &[
    Tier {
        rank: "01",
        title: "Talk",
        body: "Direct the whole workspace in natural language — hands-free and conversational. Just say what you want and watch it happen.",
        art: InputArt::Talk,
    },
    Tier {
        rank: "02",
        title: "Type",
        body: "Chrome-style shortcuts for browsing, tmux-style <leader> commands for layout. High velocity, near-zero learning curve.",
        art: InputArt::Type,
    },
    Tier {
        rank: "03",
        title: "Click",
        body: "Plain, intuitive point-and-click that keeps everything grounded in predictable browser behavior.",
        art: InputArt::Click,
    },
];

#[component]
pub fn InputScene() -> Element {
    rsx! {
        section { class: "relative max-w-5xl mx-auto px-6 py-24 sm:py-32",
            div { class: "text-center mb-16 sm:mb-20 reveal",
                p { class: "text-sm uppercase tracking-[0.2em] text-accent mb-3", "Input" }
                h2 { class: "text-3xl sm:text-5xl font-bold tracking-tight mb-4",
                    "Talk › Type › Click"
                }
                p { class: "text-text-muted leading-relaxed max-w-2xl mx-auto",
                    "Interaction ordered from abstract delegation down to mechanical control."
                }
            }
            div { class: "flex flex-col gap-16 sm:gap-24",
                for (i , t) in TIERS.iter().enumerate() {
                    div {
                        key: "{t.rank}",
                        class: "grid grid-cols-1 items-center gap-8 md:grid-cols-2 md:gap-12 reveal",
                        div { class: if i % 2 == 1 { "md:order-2" } else { "" },
                            div { class: "mb-3 flex items-baseline gap-3",
                                span { class: "font-mono text-sm text-accent", "{t.rank}" }
                                h3 { class: "text-2xl sm:text-3xl font-bold tracking-tight text-text", "{t.title}" }
                            }
                            p { class: "text-base sm:text-lg text-text-muted leading-relaxed", "{t.body}" }
                        }
                        div { class: if i % 2 == 1 { "md:order-1" } else { "" },
                            {input_art(t.art)}
                        }
                    }
                }
            }
        }
    }
}

fn input_art(kind: InputArt) -> Element {
    match kind {
        InputArt::Talk => talk_art(),
        InputArt::Type => type_art(),
        InputArt::Click => click_art(),
    }
}

fn talk_art() -> Element {
    rsx! {
        div { class: "relative flex h-64 items-center justify-center overflow-hidden rounded-2xl border border-accent/20 bg-[#0d0d18] shadow-xl shadow-accent/20",
            div { class: "pointer-events-none absolute left-1/2 top-1/2 h-40 w-40 -translate-x-1/2 -translate-y-1/2 rounded-full bg-accent/15 blur-[60px]" }
            div { class: "relative flex flex-col items-center gap-6",
                div { class: "relative flex h-20 w-20 items-center justify-center",
                    span { class: "absolute inset-0 rounded-full border border-accent/25 animate-ping motion-reduce:animate-none" }
                    span { class: "absolute inset-2 rounded-full border border-accent/20" }
                    div { class: "flex h-16 w-16 items-center justify-center rounded-full border border-accent/40 bg-gradient-to-br from-accent/40 to-accent/5 text-accent shadow-lg shadow-accent/40",
                        svg {
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            class: "h-7 w-7",
                            rect { x: "9", y: "2", width: "6", height: "12", rx: "3" }
                            path { d: "M19 10v1a7 7 0 0 1-14 0v-1" }
                            path { d: "M12 18v4" }
                            path { d: "M8 22h8" }
                        }
                    }
                }
                div { class: "flex h-10 items-center gap-1.5",
                    {bar("h-3", "0ms")}
                    {bar("h-6", "120ms")}
                    {bar("h-9", "240ms")}
                    {bar("h-5", "360ms")}
                    {bar("h-10", "180ms")}
                    {bar("h-4", "300ms")}
                    {bar("h-7", "60ms")}
                    {bar("h-3", "420ms")}
                    {bar("h-6", "200ms")}
                }
            }
        }
    }
}

fn bar(h: &str, delay: &str) -> Element {
    rsx! {
        span {
            class: "w-1.5 rounded-full bg-accent/70 animate-pulse motion-reduce:animate-none {h}",
            style: "animation-delay: {delay}",
        }
    }
}

fn type_art() -> Element {
    rsx! {
        div { class: "relative flex h-64 items-center justify-center overflow-hidden rounded-2xl border border-aurora-violet/20 bg-[#120c1a] shadow-xl shadow-aurora-violet/20",
            div { class: "pointer-events-none absolute left-1/2 top-1/2 h-40 w-40 -translate-x-1/2 -translate-y-1/2 rounded-full bg-aurora-violet/15 blur-[60px]" }
            div { class: "relative flex flex-col items-center gap-4",
                div { class: "rounded-md border border-aurora-violet/40 bg-aurora-violet/15 px-2.5 py-1 font-mono text-[11px] text-text shadow-lg shadow-aurora-violet/20",
                    "⌘ K"
                }
                div { class: "flex flex-col items-center gap-1.5",
                    div { class: "flex gap-1.5",
                        {cap(false)}
                        {cap(false)}
                        {cap(false)}
                        {cap(false)}
                        {cap(false)}
                        {cap(false)}
                    }
                    div { class: "flex gap-1.5",
                        {cap(false)}
                        {cap(false)}
                        {cap(true)}
                        {cap(true)}
                        {cap(false)}
                        {cap(false)}
                    }
                    div { class: "flex justify-center",
                        span { class: "h-6 w-32 rounded-md border border-white/15 bg-gradient-to-b from-white/[0.12] to-white/[0.03] shadow-[0_1px_2px_rgba(0,0,0,0.4)]" }
                    }
                }
            }
        }
    }
}

fn cap(highlight: bool) -> Element {
    let extra = if highlight {
        "border-aurora-violet/50 bg-aurora-violet/25"
    } else {
        "border-white/15 bg-gradient-to-b from-white/[0.12] to-white/[0.03]"
    };
    rsx! {
        span { class: "h-6 w-6 rounded-md border shadow-[0_1px_2px_rgba(0,0,0,0.4)] {extra}" }
    }
}

fn click_art() -> Element {
    rsx! {
        div { class: "relative flex h-64 items-center justify-center overflow-hidden rounded-2xl border border-aurora-cyan/20 bg-[#0b1418] shadow-xl shadow-aurora-cyan/20",
            div { class: "pointer-events-none absolute left-1/2 top-1/2 h-40 w-40 -translate-x-1/2 -translate-y-1/2 rounded-full bg-aurora-cyan/15 blur-[60px]" }
            div { class: "relative",
                div { class: "flex h-10 items-center rounded-lg border border-aurora-cyan/40 bg-aurora-cyan/10 px-5 shadow-lg shadow-aurora-cyan/20",
                    div { class: "h-1.5 w-16 rounded-full bg-aurora-cyan/60" }
                }
                div { class: "absolute -bottom-3 -right-3 flex h-10 w-10 items-center justify-center",
                    span { class: "absolute inset-0 rounded-full border border-aurora-cyan/50 animate-ping motion-reduce:animate-none" }
                    span { class: "absolute inset-2 rounded-full bg-aurora-cyan/20" }
                    svg {
                        view_box: "0 0 24 24",
                        fill: "currentColor",
                        class: "relative h-7 w-7 text-text drop-shadow-lg",
                        path { d: "M5 3l14 7-6 2-2 6z" }
                    }
                }
            }
        }
    }
}
