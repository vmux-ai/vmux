use dioxus::prelude::*;

#[derive(Clone, Copy)]
enum Art {
    Coworking,
    Browser,
    Terminal,
}

struct Pillar {
    title: &'static str,
    body: &'static str,
    art: Art,
}

const PILLARS: &[Pillar] = &[
    Pillar {
        title: "Co-working",
        body: "People and agents work in one shared space — from hands-on pairing to full autonomy. Watch a run and grab the keyboard, or turn agents loose.",
        art: Art::Coworking,
    },
    Pillar {
        title: "Known by heart",
        body: "It looks and acts like a standard web browser. No learning curve — everyone already knows how to use it.",
        art: Art::Browser,
    },
    Pillar {
        title: "IDE power",
        body: "Beneath the surface: advanced tools, keyboard-driven workflows, and deep environment control for when you want it.",
        art: Art::Terminal,
    },
];

#[component]
pub fn Pillars() -> Element {
    rsx! {
        section { class: "relative max-w-5xl mx-auto px-6 py-24 sm:py-32",
            p { class: "text-center text-sm uppercase tracking-[0.2em] text-accent mb-3",
                "Two worlds, one workspace"
            }
            h2 { class: "text-center text-3xl sm:text-4xl font-bold tracking-tight mb-16 sm:mb-20 max-w-2xl mx-auto",
                "Vmux bridges chat-first tools and developer IDEs."
            }
            div { class: "flex flex-col gap-16 sm:gap-24",
                for (i , p) in PILLARS.iter().enumerate() {
                    div {
                        key: "{p.title}",
                        class: "grid grid-cols-1 items-center gap-8 md:grid-cols-2 md:gap-12 reveal",
                        div { class: if i % 2 == 1 { "md:order-2" } else { "" },
                            h3 { class: "text-2xl sm:text-3xl font-bold tracking-tight text-accent mb-3", "{p.title}" }
                            p { class: "text-base sm:text-lg text-text-muted leading-relaxed", "{p.body}" }
                        }
                        div { class: if i % 2 == 1 { "md:order-1" } else { "" },
                            {art(p.art)}
                        }
                    }
                }
            }
        }
    }
}

fn svg_icon(class: &str, body: Element) -> Element {
    rsx! {
        svg {
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "2",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            class: "{class}",
            {body}
        }
    }
}

fn icon_globe(class: &str) -> Element {
    svg_icon(
        class,
        rsx! {
            circle { cx: "12", cy: "12", r: "10" }
            path { d: "M12 2a14.5 14.5 0 0 0 0 20 14.5 14.5 0 0 0 0-20" }
            path { d: "M2 12h20" }
        },
    )
}

fn icon_term(class: &str) -> Element {
    svg_icon(
        class,
        rsx! {
            path { d: "M4 17 10 11 4 5" }
            path { d: "M12 19h8" }
        },
    )
}

fn icon_search(class: &str) -> Element {
    svg_icon(
        class,
        rsx! {
            circle { cx: "11", cy: "11", r: "8" }
            path { d: "m21 21-4.3-4.3" }
        },
    )
}

fn icon_mic(class: &str) -> Element {
    svg_icon(
        class,
        rsx! {
            rect { x: "9", y: "2", width: "6", height: "12", rx: "3" }
            path { d: "M19 10v1a7 7 0 0 1-14 0v-1" }
            path { d: "M12 18v4" }
            path { d: "M8 22h8" }
        },
    )
}

fn icon_person(class: &str) -> Element {
    svg_icon(
        class,
        rsx! {
            path { d: "M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2" }
            circle { cx: "12", cy: "7", r: "4" }
        },
    )
}

fn icon_bot(class: &str) -> Element {
    svg_icon(
        class,
        rsx! {
            path { d: "M12 8V4H8" }
            rect { width: "16", height: "12", x: "4", y: "8", rx: "2" }
            path { d: "M2 14h2" }
            path { d: "M20 14h2" }
            path { d: "M15 13v2" }
            path { d: "M9 13v2" }
        },
    )
}

fn avatar_you() -> Element {
    rsx! {
        div { class: "flex h-5 w-5 shrink-0 items-center justify-center rounded-full border border-accent/40 bg-accent/15 text-accent",
            {icon_person("h-3 w-3")}
        }
    }
}

fn avatar_bot() -> Element {
    rsx! {
        div { class: "flex h-5 w-5 shrink-0 items-center justify-center rounded-full border border-aurora-violet/40 bg-aurora-violet/15 text-aurora-violet",
            {icon_bot("h-3 w-3")}
        }
    }
}

fn nav_icon(paths: &[&str]) -> Element {
    rsx! {
        span { class: "flex h-5 w-5 shrink-0 items-center justify-center rounded text-text-muted",
            svg {
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                class: "h-3 w-3",
                for d in paths.iter() {
                    path { key: "{d}", d: "{d}" }
                }
            }
        }
    }
}

fn tab(icon: Element, title: &str, active: bool) -> Element {
    let class = if active {
        "flex items-center gap-1.5 rounded-md bg-white/[0.06] border border-white/10 px-2 py-1 text-[10px] text-text"
    } else {
        "flex items-center gap-1.5 rounded-md px-2 py-1 text-[10px] text-text-muted"
    };
    rsx! {
        div { class: "{class}",
            {icon}
            span { class: "max-w-[72px] truncate", "{title}" }
        }
    }
}

fn chrome(frame: &str, tabs: Element, address: &str, body: Element) -> Element {
    rsx! {
        div { class: "flex h-64 flex-col overflow-hidden rounded-lg border {frame}",
            div { class: "flex items-center gap-1 px-2 pt-2",
                {tabs}
                {nav_icon(&["M5 12h14", "M12 5v14"])}
            }
            div { class: "flex items-center gap-1.5 border-b border-t border-white/10 bg-white/[0.03] px-2 py-1.5",
                {nav_icon(&["M19 12H5", "M12 19l-7-7 7-7"])}
                {nav_icon(&["M5 12h14", "M12 5l7 7-7 7"])}
                {nav_icon(&["M21 12a9 9 0 11-3-6.7L21 8", "M21 3v5h-5"])}
                div { class: "ml-1 flex h-6 min-w-0 flex-1 items-center rounded-md border border-white/10 bg-black/40 px-2",
                    span { class: "truncate font-mono text-[10px] text-text-muted", "{address}" }
                }
            }
            div { class: "min-h-0 flex-1 overflow-hidden",
                {body}
            }
        }
    }
}

fn art(kind: Art) -> Element {
    match kind {
        Art::Coworking => chrome(
            "border-accent/20 bg-[#0d0d18] shadow-xl shadow-accent/20",
            rsx! {
                {tab(icon_bot("h-3 w-3 text-aurora-violet"), "vibe", true)}
                {tab(icon_globe("h-3 w-3"), "example.com", false)}
            },
            "vmux://agent/vibe/2c80e7…",
            rsx! {
                div { class: "flex h-full flex-col justify-center gap-3 p-4",
                    div { class: "flex items-end gap-2",
                        {avatar_bot()}
                        div { class: "rounded-xl rounded-bl-sm bg-white/10 px-3 py-2 text-[11px] leading-snug text-text",
                            "Tests pass — ship it?"
                        }
                    }
                    div { class: "flex items-end justify-end gap-2",
                        div { class: "rounded-xl rounded-br-sm border border-accent/30 bg-accent/25 px-3 py-2 text-[11px] leading-snug text-text",
                            "Ship it."
                        }
                        {avatar_you()}
                    }
                }
            },
        ),
        Art::Browser => chrome(
            "border-aurora-cyan/20 bg-[#0b1418] shadow-xl shadow-aurora-cyan/20",
            rsx! {
                {tab(icon_globe("h-3 w-3 text-aurora-cyan"), "New Tab", true)}
                {tab(icon_globe("h-3 w-3"), "docs", false)}
            },
            "Search or enter address",
            rsx! {
                div { class: "flex h-full flex-col items-center justify-center p-4",
                    div { class: "flex w-full items-center gap-3 rounded-full border border-white/20 bg-white/[0.08] px-5 py-3.5",
                        {icon_search("h-4 w-4 shrink-0 text-text-muted")}
                        span { class: "flex-1 text-left text-[12px] text-text-muted", "Search the web" }
                        {icon_mic("h-4 w-4 shrink-0 text-aurora-cyan/70")}
                    }
                }
            },
        ),
        Art::Terminal => chrome(
            "border-aurora-violet/20 bg-[#120c1a] shadow-xl shadow-aurora-violet/20",
            rsx! {
                {tab(icon_term("h-3 w-3 text-aurora-violet"), "pillars.rs", true)}
            },
            "file://.worktrees/website-agents-section/…/pillars.rs",
            rsx! {
                div { class: "flex h-full gap-2 overflow-hidden p-3",
                    div { class: "flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden rounded-md border border-accent/30 bg-accent/[0.04] font-mono text-[9px] leading-relaxed shadow-lg shadow-accent/10",
                        div { class: "flex items-center gap-1.5 border-b border-white/10 px-2 py-1 text-text-muted",
                            {icon_bot("h-2.5 w-2.5 text-accent")}
                            span { "vibe" }
                        }
                        div { class: "flex-1 space-y-1 p-2",
                            div {
                                span { class: "text-accent", "› " }
                                span { class: "text-text", "add agents section" }
                            }
                            div { class: "text-text-muted", "● editing pillars.rs" }
                            div { class: "text-text-muted", "  + alternating rows" }
                            div { class: "text-aurora-cyan/80", "✓ done" }
                            div {
                                span { class: "text-accent", "› " }
                                span { class: "inline-block w-1.5 h-2.5 align-middle bg-accent animate-pulse motion-reduce:animate-none" }
                            }
                        }
                    }
                    div { class: "flex w-1/2 min-h-0 flex-col gap-2",
                        div { class: "flex min-h-0 flex-[1.6] flex-col overflow-hidden rounded-md border border-aurora-violet/30 bg-[#0d0d18] font-mono text-[9px] leading-relaxed",
                            div { class: "border-b border-white/10 px-2 py-1 text-text-muted", "pillars.rs" }
                            div { class: "flex-1 p-2",
                                div {
                                    span { class: "text-aurora-violet", "fn " }
                                    span { class: "text-accent", "art" }
                                    span { class: "text-text-muted", "(kind) {{" }
                                }
                                div { class: "pl-2",
                                    span { class: "text-aurora-violet", "match " }
                                    span { class: "text-text", "kind" }
                                    span { class: "text-text-muted", " {{ … }}" }
                                }
                                div {
                                    span { class: "text-text-muted", "}}" }
                                }
                            }
                        }
                        div { class: "flex min-h-0 flex-1 flex-col overflow-hidden rounded-md border border-white/10 bg-black/30 font-mono text-[9px]",
                            div { class: "border-b border-white/10 px-2 py-1 text-text-muted", "zsh" }
                            div { class: "flex-1 p-2",
                                div {
                                    span { class: "text-text-muted", "$ " }
                                    span { class: "text-text", "dx serve" }
                                }
                                div {
                                    span { class: "text-text-muted", "$ " }
                                    span { class: "inline-block w-1.5 h-2.5 align-middle bg-aurora-violet animate-pulse motion-reduce:animate-none" }
                                }
                            }
                        }
                    }
                }
            },
        ),
    }
}
