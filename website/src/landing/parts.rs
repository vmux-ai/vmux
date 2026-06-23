use dioxus::prelude::*;
use dioxus_primitives::toast::{ToastOptions, use_toast};

use crate::hooks::use_clipboard_copy;
use crate::landing::INSTALL_CMD;

#[component]
pub fn InstallCard() -> Element {
    let toast = use_toast();
    let copy = use_clipboard_copy();
    rsx! {
        div { class: "glass inline-flex flex-col sm:flex-row items-center gap-2 sm:gap-3 rounded-xl px-4 py-3 text-sm sm:text-base",
            code { class: "font-mono text-accent", "{INSTALL_CMD}" }
            button {
                class: "bg-accent text-black border-0 rounded px-3 py-1.5 text-sm font-semibold cursor-pointer transition-colors hover:bg-accent-hover",
                onclick: move |_| {
                    copy(INSTALL_CMD.to_string());
                    toast.success("Copied!".to_string(), ToastOptions::new());
                },
                "Copy"
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

pub fn icon_globe(class: &str) -> Element {
    svg_icon(
        class,
        rsx! {
            circle { cx: "12", cy: "12", r: "10" }
            path { d: "M12 2a14.5 14.5 0 0 0 0 20 14.5 14.5 0 0 0 0-20" }
            path { d: "M2 12h20" }
        },
    )
}

pub fn icon_term(class: &str) -> Element {
    svg_icon(
        class,
        rsx! {
            path { d: "M4 17 10 11 4 5" }
            path { d: "M12 19h8" }
        },
    )
}

pub fn icon_search(class: &str) -> Element {
    svg_icon(
        class,
        rsx! {
            circle { cx: "11", cy: "11", r: "8" }
            path { d: "m21 21-4.3-4.3" }
        },
    )
}

pub fn icon_mic(class: &str) -> Element {
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

pub fn icon_person(class: &str) -> Element {
    svg_icon(
        class,
        rsx! {
            path { d: "M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2" }
            circle { cx: "12", cy: "7", r: "4" }
        },
    )
}

pub fn icon_bot(class: &str) -> Element {
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

pub fn avatar_you() -> Element {
    rsx! {
        div { class: "flex h-5 w-5 shrink-0 items-center justify-center rounded-full border border-accent/40 bg-accent/15 text-accent",
            {icon_person("h-3 w-3")}
        }
    }
}

pub fn avatar_bot() -> Element {
    rsx! {
        div { class: "flex h-5 w-5 shrink-0 items-center justify-center rounded-full border border-aurora-violet/40 bg-aurora-violet/15 text-aurora-violet",
            {icon_bot("h-3 w-3")}
        }
    }
}

pub fn nav_icon(paths: &[&str]) -> Element {
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

pub fn tab(icon: Element, title: &str, active: bool) -> Element {
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

pub fn browser_frame(frame: &str, tabs: Element, address: &str, body: Element) -> Element {
    rsx! {
        div { class: "flex min-h-[16rem] flex-col overflow-hidden rounded-lg border {frame}",
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

pub fn website_pane() -> Element {
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

pub fn editor_pane() -> Element {
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

pub fn terminal_pane() -> Element {
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

pub fn headline(eyebrow: &str, lead: &str, punch: &str) -> Element {
    rsx! {
        div { class: "reveal",
            if !eyebrow.is_empty() {
                p { class: "text-sm uppercase tracking-[0.25em] text-accent mb-4", "{eyebrow}" }
            }
            h2 { class: "font-bold tracking-tight leading-[1.05]",
                span { class: "block text-2xl sm:text-3xl text-text-muted", "{lead}" }
                span { class: "block text-4xl sm:text-7xl text-text", "{punch}" }
            }
        }
    }
}

pub fn scroll_cue() -> Element {
    rsx! {
        div { class: "mt-12 flex justify-center",
            span { class: "inline-flex h-9 w-6 items-start justify-center rounded-full border border-text-muted/40 p-1.5",
                span { class: "h-2 w-1 rounded-full bg-text-muted/70 animate-cue motion-reduce:animate-none" }
            }
        }
    }
}
