use dioxus::prelude::*;

use crate::landing::parts::{headline, icon_bot, icon_person};

#[component]
pub fn Coworking() -> Element {
    rsx! {
        section {
            "data-tone": "dark",
            class: "relative isolate overflow-hidden px-6 py-28 sm:py-36 bg-bg text-text",
            div { class: "pointer-events-none absolute inset-0 -z-10",
                div { class: "absolute left-1/2 top-1/4 h-96 w-96 -translate-x-1/2 rounded-full bg-aurora-violet/20 blur-[130px] animate-aurora motion-reduce:animate-none" }
            }
            div { class: "mx-auto max-w-5xl",
                div { class: "text-center mb-16",
                    {headline("Co-working", "People and agents,", "side by side.")}
                }
                div {
                    class: "glass rounded-2xl p-8 reveal max-w-3xl mx-auto",
                    style: "transition-delay: 120ms",
                    div { class: "flex items-center gap-4",
                        div { class: "flex flex-col items-center gap-1 shrink-0",
                            div { class: "h-10 w-10 rounded-full bg-gradient-to-br from-accent/30 to-accent/5 border border-accent/40 flex items-center justify-center text-accent",
                                {icon_person("w-5 h-5")}
                            }
                            span { class: "text-[10px] text-text-muted", "You" }
                        }
                        div { class: "flex-1",
                            div { class: "flex items-center justify-between text-xs text-text-muted mb-2",
                                span { "Hands-on pairing" }
                                span { "Full autonomy" }
                            }
                            div { class: "relative h-2 rounded-full bg-border",
                                div { class: "absolute inset-y-0 left-0 w-1/2 rounded-full bg-gradient-to-r from-accent to-aurora-violet animate-slide motion-reduce:animate-none",
                                    div { class: "absolute right-0 top-1/2 -translate-y-1/2 translate-x-1/2 h-5 w-5 rounded-full bg-accent shadow-lg shadow-accent/40" }
                                }
                            }
                        }
                        div { class: "flex flex-col items-center gap-1 shrink-0",
                            div { class: "h-10 w-10 rounded-full bg-gradient-to-br from-aurora-violet/30 to-aurora-cyan/5 border border-aurora-violet/40 flex items-center justify-center text-aurora-violet",
                                {icon_bot("w-5 h-5")}
                            }
                            span { class: "text-[10px] text-text-muted", "Agent" }
                        }
                    }
                    p { class: "mt-4 text-sm text-text-muted",
                        "Watch a run and grab the keyboard to steer, or turn agents loose in their own space."
                    }
                }
                div { class: "mt-24 text-center mb-12 reveal",
                    h3 { class: "text-2xl sm:text-3xl font-bold tracking-tight",
                        "Prompt your agents — talk or type."
                    }
                    p { class: "mt-3 text-text-muted max-w-2xl mx-auto",
                        "Talk and type are how you prompt an agent. Click stays grounded in plain browser control."
                    }
                }
                div { class: "grid grid-cols-1 md:grid-cols-3 gap-6",
                    {prompt_tier("01", "Talk", "Speak your prompt — direct the whole space hands-free.", talk_art())}
                    {prompt_tier("02", "Type", "Type your prompt, plus tmux-style <leader> commands for layout.", type_art())}
                    {prompt_tier("03", "Click", "Plain, predictable point-and-click browser control.", click_art())}
                }
            }
        }
    }
}

fn prompt_tier(rank: &str, title: &str, body: &str, art: Element) -> Element {
    rsx! {
        div { class: "reveal flex flex-col gap-4",
            {art}
            div { class: "flex items-baseline gap-3",
                span { class: "font-mono text-sm text-accent", "{rank}" }
                h4 { class: "text-xl font-bold tracking-tight", "{title}" }
            }
            p { class: "text-text-muted leading-relaxed", "{body}" }
        }
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
