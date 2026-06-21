use dioxus::prelude::*;

struct Addr {
    url: &'static str,
    hot: bool,
}

const ADDRS: &[Addr] = &[
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
    "browser_navigate",
    "run",
    "read_layout",
    "update_layout",
    "create_space",
];

#[component]
pub fn Agents() -> Element {
    rsx! {
        section { class: "relative max-w-5xl mx-auto px-6 py-24 sm:py-32",
            div { class: "grid grid-cols-1 md:grid-cols-2 gap-10 items-center",
                div { class: "reveal",
                    p { class: "text-sm uppercase tracking-[0.2em] text-accent mb-3", "Agents" }
                    h2 { class: "text-3xl sm:text-4xl font-bold tracking-tight mb-4",
                        "The workspace is an API."
                    }
                    p { class: "text-text-muted leading-relaxed mb-4",
                        "Agents drive Vmux over MCP — browse, run commands in a terminal you can watch and take over, and reshape the layout declaratively."
                    }
                    p { class: "text-text-muted leading-relaxed",
                        "Every agent session is a first-class, addressable surface — anchored in its own space, so a background run never disrupts the one you're in."
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
                        span { class: "ml-2 text-[11px] text-text-muted", "addressable surfaces" }
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
