use dioxus::prelude::*;

struct Addr {
    url: &'static str,
    hot: bool,
}

const ADDRS: &[Addr] = &[
    Addr {
        url: "vmux://agent/claude/8d1f2c…",
        hot: true,
    },
    Addr {
        url: "vmux://agent/codex/4f3a91…",
        hot: true,
    },
    Addr {
        url: "vmux://agent/vibe/2c80e7…",
        hot: true,
    },
    Addr {
        url: "vmux://terminal/?cwd=~/proj",
        hot: false,
    },
    Addr {
        url: "vmux://spaces/",
        hot: false,
    },
    Addr {
        url: "vmux://services/",
        hot: false,
    },
];

const TOOLS: &[&str] = &[
    "vmux_browser_navigate",
    "vmux_run",
    "vmux_read_layout",
    "vmux_update_layout",
    "vmux_create_space",
];

fn key(label: &str, cls: &str) -> Element {
    rsx! {
        kbd { class: "inline-flex items-center justify-center rounded-md border border-white/20 bg-gradient-to-b from-white/[0.14] to-white/[0.03] font-semibold text-text shadow-[0_1px_2px_rgba(0,0,0,0.4)] {cls}",
            "{label}"
        }
    }
}

#[component]
pub fn Agents() -> Element {
    rsx! {
        section { class: "relative max-w-5xl mx-auto px-6 py-24 sm:py-32",
            div { class: "grid grid-cols-1 md:grid-cols-2 gap-10 items-center",
                div { class: "reveal",
                    p { class: "text-sm uppercase tracking-[0.2em] text-accent mb-3", "Agents" }
                    h2 { class: "mb-5",
                        span { class: "block text-3xl sm:text-4xl font-bold tracking-tight",
                            "Every agent has a home."
                        }
                        span { class: "mt-3 flex flex-wrap items-center gap-2 text-lg sm:text-xl font-medium text-text-muted",
                            "Hit"
                            {key("⌘ L", "h-6 px-2 text-sm")}
                            "to visit."
                        }
                    }
                    p { class: "text-text-muted leading-relaxed mb-4",
                        "Every agent, terminal, and space lives at its own address, ready to share or jump back to. A background run stays anchored in its own space, so it never disrupts the one you're in."
                    }
                    p { class: "text-text-muted leading-relaxed",
                        "Under the hood, agents drive Vmux over MCP: browse, run commands in a terminal you can watch and take over, and reshape the layout declaratively."
                    }
                    div { class: "mt-6 flex flex-wrap gap-2",
                        for t in TOOLS.iter() {
                            span {
                                key: "{t}",
                                class: "font-mono text-[11px] px-2 py-1 rounded-md border border-white/10 bg-white/5 text-text-muted",
                                "{t}"
                            }
                        }
                    }
                }
                div {
                    class: "reveal rounded-2xl border border-white/10 bg-white/5 backdrop-blur p-5 font-mono text-sm shadow-xl shadow-accent/10",
                    style: "transition-delay: 120ms",
                    div { class: "flex items-center gap-1.5 mb-4",
                        span { class: "h-2.5 w-2.5 rounded-full bg-white/15" }
                        span { class: "h-2.5 w-2.5 rounded-full bg-white/15" }
                        span { class: "h-2.5 w-2.5 rounded-full bg-white/15" }
                        span { class: "ml-2 flex items-center gap-1.5 text-[11px] text-text-muted",
                            {key("⌘ L", "h-4 px-1.5 text-[10px]")}
                            "to visit"
                        }
                    }
                    div { class: "flex flex-col gap-2.5",
                        for a in ADDRS.iter() {
                            div { key: "{a.url}", class: "flex items-center gap-2",
                                span {
                                    class: if a.hot { "text-accent" } else { "text-text-muted" },
                                    "→"
                                }
                                span {
                                    class: if a.hot { "text-text" } else { "text-text-muted" },
                                    "{a.url}"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
