use dioxus::prelude::*;

use crate::landing::parts::headline;

#[component]
pub fn Showcase() -> Element {
    rsx! {
        section {
            "data-tone": "dark",
            class: "relative isolate overflow-hidden px-6 py-28 sm:py-36 bg-bg text-text",
            div { class: "pointer-events-none absolute inset-0 -z-10",
                div { class: "absolute left-1/2 top-1/3 h-[32rem] w-[32rem] -translate-x-1/2 rounded-full bg-accent/15 blur-[140px] animate-aurora motion-reduce:animate-none" }
            }
            div { class: "mx-auto max-w-2xl text-center mb-12",
                {headline("Ask for anything", "One prompt.", "Anything, done.")}
                p { class: "mt-5 text-base sm:text-lg text-text-muted reveal",
                    "Travel, a website, a pull request — your agents do the work while you watch."
                }
            }
            div { class: "mx-auto w-full max-w-4xl reveal", style: "transition-delay: 120ms",
                div { class: "scene-stack relative h-[32rem] sm:h-[26rem]",
                    {scene_flight()}
                    {scene_site()}
                    {scene_pr()}
                }
                div { class: "scene-dots mt-6 flex flex-col items-center gap-3",
                    div { class: "flex items-center gap-2",
                        span { class: "scene-dot h-1.5 w-7 rounded-full bg-accent" }
                        span { class: "scene-dot h-1.5 w-7 rounded-full bg-accent", style: "animation-delay: -4s" }
                        span { class: "scene-dot h-1.5 w-7 rounded-full bg-accent", style: "animation-delay: -8s" }
                    }
                    p { class: "text-xs text-text-muted", "flight → restaurant site → theme PR" }
                }
            }
        }
    }
}

fn shell(delay: &str, tabs: Element, body: Element, status: Element) -> Element {
    rsx! {
        div {
            class: "scene-cycle glass flex h-full flex-col overflow-hidden rounded-xl border border-white/10",
            style: "animation-delay: {delay}",
            div { class: "flex h-7 items-center gap-1.5 border-b border-white/10 px-3",
                span { class: "h-2 w-2 rounded-full bg-white/15" }
                span { class: "h-2 w-2 rounded-full bg-white/15" }
                span { class: "h-2 w-2 rounded-full bg-white/15" }
                div { class: "ml-2 flex gap-1.5", {tabs} }
            }
            div { class: "flex min-h-0 flex-1 gap-1.5 p-1.5", {body} }
            {status}
        }
    }
}

fn sc_tab(dot: &str, label: &str, active: bool) -> Element {
    let cls = if active {
        "flex items-center gap-1.5 rounded-md border border-white/10 bg-white/[0.06] px-2 py-0.5 text-[10px] text-text"
    } else {
        "flex items-center gap-1.5 rounded-md px-2 py-0.5 text-[10px] text-text-muted"
    };
    rsx! {
        div { class: "{cls}",
            span { class: "h-1.5 w-1.5 rounded-full {dot}" }
            span { class: "max-w-[80px] truncate", "{label}" }
        }
    }
}

fn agent_pane(ask: &str, reply: &str, flex: &str) -> Element {
    rsx! {
        div { class: "{flex} flex min-w-0 flex-col overflow-hidden rounded-lg border border-accent/40 bg-[#0d0d18]",
            div { class: "border-b border-white/5 px-2.5 py-1 text-[8px] font-bold uppercase tracking-wide text-accent",
                "agent · vibe"
            }
            div { class: "flex flex-1 flex-col gap-2 p-2.5",
                div { class: "self-end max-w-[92%] rounded-xl rounded-br-sm border border-accent/30 bg-accent/20 px-2.5 py-1.5 text-[10px] leading-snug text-text",
                    "{ask}"
                }
                div { class: "self-start max-w-[92%] rounded-xl rounded-bl-sm border border-white/10 bg-white/5 px-2.5 py-1.5 text-[10px] leading-snug text-text-muted",
                    span { class: "font-bold text-emerald-300", "✓ " }
                    "{reply}"
                }
                div { class: "mt-auto flex items-center gap-1.5 rounded-lg border border-white/10 bg-black/40 px-2.5 py-1.5",
                    span { class: "text-[8px] font-bold text-accent", "⌘K" }
                    span { class: "text-[9px] text-text-muted", "Ask anything" }
                    span { class: "ml-0.5 inline-block h-2.5 w-px bg-accent animate-pulse motion-reduce:animate-none" }
                }
            }
        }
    }
}

fn status(space: &str, windows: Element) -> Element {
    rsx! {
        div { class: "flex h-5 items-center gap-2 border-t border-accent/20 bg-accent/10 px-2 font-mono text-[8px] text-accent",
            span { class: "rounded-sm bg-accent px-1.5 font-bold text-black", "{space}" }
            {windows}
            span { class: "ml-auto text-text-muted", "⌘K" }
        }
    }
}

fn pane_head(color: &str, label: &str) -> Element {
    rsx! {
        div { class: "border-b border-white/5 px-2.5 py-1 text-[8px] font-bold uppercase tracking-wide {color}",
            "{label}"
        }
    }
}

fn flight_row(route: &str, price: &str, hot: bool) -> Element {
    let row = if hot {
        "flex items-center justify-between border-t border-aurora-cyan/20 bg-aurora-cyan/10 px-2.5 py-1.5 text-[9px]"
    } else {
        "flex items-center justify-between border-t border-white/5 px-2.5 py-1.5 text-[9px]"
    };
    rsx! {
        div { class: "{row}",
            span { class: "text-text-muted", "{route}" }
            span { class: "font-bold text-text", "{price}" }
        }
    }
}

fn scene_flight() -> Element {
    shell(
        "0s",
        rsx! {
            {sc_tab("bg-accent", "vibe", true)}
            {sc_tab("bg-aurora-cyan", "flights", false)}
        },
        rsx! {
            {agent_pane(
                "Find me a flight to Tokyo from Paris next month",
                "3 options, €548–612",
                "flex-[1.1]",
            )}
            div { class: "flex flex-1 min-w-0 flex-col overflow-hidden rounded-lg border border-aurora-cyan/35 bg-[#0b1418]",
                {pane_head("text-aurora-cyan", "browser")}
                div { class: "mx-2.5 mt-2 rounded-md border border-white/10 bg-black/40 px-2 py-1 font-mono text-[8px] text-text-muted",
                    "google.com/flights"
                }
                div { class: "mt-1",
                    {flight_row("CDG → HND · May 18", "€548", true)}
                    {flight_row("CDG → HND · May 14", "€612", false)}
                    {flight_row("CDG → NRT · May 21", "€599", false)}
                }
            }
        },
        status(
            "tokyo",
            rsx! {
                span { class: "text-text", "0:vibe" }
                span { class: "text-text-muted", "1:flights" }
            },
        ),
    )
}

fn scene_site() -> Element {
    shell(
        "-4s",
        rsx! {
            {sc_tab("bg-accent", "vibe", true)}
            {sc_tab("bg-aurora-cyan", "preview", false)}
        },
        rsx! {
            {agent_pane(
                "Make me a website for my new restaurant",
                "Osteria Lina — menu + booking",
                "flex-[1.1]",
            )}
            div { class: "flex flex-1 min-w-0 flex-col overflow-hidden rounded-lg border border-aurora-cyan/35 bg-[#0b1418]",
                {pane_head("text-aurora-cyan", "preview")}
                div { class: "flex flex-1 flex-col gap-2 p-3",
                    div { class: "text-sm font-bold tracking-tight text-text", "Osteria Lina" }
                    div { class: "text-[8px] uppercase tracking-[0.2em] text-aurora-cyan", "Menu · Reserve · Find us" }
                    div { class: "mt-1 h-16 rounded-md bg-gradient-to-br from-aurora-cyan/25 to-accent/15" }
                    div { class: "h-1.5 w-4/5 rounded-full bg-white/10" }
                    div { class: "h-1.5 w-3/5 rounded-full bg-white/10" }
                }
            }
        },
        status(
            "lina",
            rsx! {
                span { class: "text-text", "0:vibe" }
                span { class: "text-text-muted", "1:preview" }
            },
        ),
    )
}

fn scene_pr() -> Element {
    shell(
        "-8s",
        rsx! {
            {sc_tab("bg-accent", "vibe", true)}
            {sc_tab("bg-accent", "theme.rs", false)}
            {sc_tab("bg-aurora-violet", "term", false)}
        },
        rsx! {
            div { class: "flex flex-1 flex-col gap-1.5",
                div { class: "flex min-h-0 flex-[1.4] gap-1.5",
                    {agent_pane("Add theme support to vmux. Open a PR.", "PR #182 opened", "flex-1")}
                    div { class: "flex flex-1 min-w-0 flex-col overflow-hidden rounded-lg border border-accent/30 bg-[#0d0d18] font-mono text-[9px] leading-[1.7]",
                        {pane_head("text-accent", "theme.rs")}
                        div { class: "flex-1 overflow-hidden py-1",
                            div { class: "bg-red-500/12 px-2.5 text-red-300", "- bg: 0x0a0a0a" }
                            div { class: "bg-emerald-500/12 px-2.5 text-emerald-300", "+ enum Theme {{" }
                            div { class: "bg-emerald-500/12 px-2.5 text-emerald-300", "+   Light, Dark, Device," }
                            div { class: "bg-emerald-500/12 px-2.5 text-emerald-300", "+ }}" }
                        }
                    }
                }
                div { class: "flex min-h-0 flex-[0.7] flex-col overflow-hidden rounded-lg border border-aurora-violet/35 bg-[#120c1a] px-2.5 py-2 font-mono text-[9px] leading-[1.7] text-text-muted",
                    div {
                        span { class: "text-aurora-violet", "$ " }
                        span { class: "text-text", "gh pr create" }
                    }
                    div { class: "text-aurora-cyan", "→ PR #182 opened · light/dark/device" }
                }
            }
        },
        status(
            "theme",
            rsx! {
                span { class: "text-text", "0:vibe" }
                span { class: "text-text-muted", "1:edit" }
                span { class: "text-text-muted", "2:term" }
            },
        ),
    )
}
