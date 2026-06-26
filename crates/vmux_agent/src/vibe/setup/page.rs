#![allow(non_snake_case)]

use crate::vibe::setup::event::AgentInstallRunRequest;
use dioxus::prelude::*;
use vmux_ui::components::icon::Icon;
use vmux_ui::hooks::{try_cef_bin_emit_rkyv, use_theme};

struct Accent {
    glow_top: &'static str,
    glow_bottom: &'static str,
    badge: &'static str,
    prompt: &'static str,
    cta: &'static str,
}

fn current_agent_segment() -> String {
    web_sys::window()
        .and_then(|w| w.location().pathname().ok())
        .and_then(|path| path.split('/').find(|s| !s.is_empty()).map(str::to_string))
        .filter(|seg| vmux_core::agent_setup::display_name(seg).is_some())
        .unwrap_or_else(|| "vibe".to_string())
}

fn tagline(segment: &str) -> &'static str {
    match segment {
        "claude" => "Anthropic's coding agent, in vmux",
        "codex" => "OpenAI's coding agent, in vmux",
        _ => "Mistral's coding agent, in vmux",
    }
}

fn accent(segment: &str) -> Accent {
    match segment {
        "claude" => Accent {
            glow_top: "pointer-events-none absolute -top-1/3 left-1/2 h-[60vh] w-[60vh] -translate-x-1/2 rounded-full bg-rose-500/20 blur-[120px]",
            glow_bottom: "pointer-events-none absolute -bottom-1/4 right-1/4 h-[44vh] w-[44vh] rounded-full bg-orange-400/10 blur-[120px]",
            badge: "flex h-12 w-12 shrink-0 items-center justify-center rounded-2xl bg-gradient-to-br from-orange-400 to-rose-500 text-white shadow-lg shadow-rose-500/30",
            prompt: "select-none font-mono text-sm text-rose-400/80",
            cta: "group inline-flex w-full items-center justify-center gap-2 rounded-xl bg-gradient-to-br from-orange-400 to-rose-500 px-4 py-2.5 text-sm font-medium text-white shadow-lg shadow-rose-500/25 transition-all hover:shadow-rose-500/40 hover:brightness-110 active:scale-[0.99]",
        },
        "codex" => Accent {
            glow_top: "pointer-events-none absolute -top-1/3 left-1/2 h-[60vh] w-[60vh] -translate-x-1/2 rounded-full bg-emerald-500/20 blur-[120px]",
            glow_bottom: "pointer-events-none absolute -bottom-1/4 right-1/4 h-[44vh] w-[44vh] rounded-full bg-teal-400/10 blur-[120px]",
            badge: "flex h-12 w-12 shrink-0 items-center justify-center rounded-2xl bg-gradient-to-br from-emerald-500 to-teal-600 text-white shadow-lg shadow-emerald-500/30",
            prompt: "select-none font-mono text-sm text-emerald-400/80",
            cta: "group inline-flex w-full items-center justify-center gap-2 rounded-xl bg-gradient-to-br from-emerald-500 to-teal-600 px-4 py-2.5 text-sm font-medium text-white shadow-lg shadow-emerald-500/25 transition-all hover:shadow-emerald-500/40 hover:brightness-110 active:scale-[0.99]",
        },
        _ => Accent {
            glow_top: "pointer-events-none absolute -top-1/3 left-1/2 h-[60vh] w-[60vh] -translate-x-1/2 rounded-full bg-orange-500/20 blur-[120px]",
            glow_bottom: "pointer-events-none absolute -bottom-1/4 right-1/4 h-[44vh] w-[44vh] rounded-full bg-amber-400/10 blur-[120px]",
            badge: "flex h-12 w-12 shrink-0 items-center justify-center rounded-2xl bg-gradient-to-br from-orange-500 to-amber-600 text-white shadow-lg shadow-orange-500/30",
            prompt: "select-none font-mono text-sm text-orange-400/80",
            cta: "group inline-flex w-full items-center justify-center gap-2 rounded-xl bg-gradient-to-br from-orange-500 to-amber-600 px-4 py-2.5 text-sm font-medium text-white shadow-lg shadow-orange-500/25 transition-all hover:shadow-orange-500/40 hover:brightness-110 active:scale-[0.99]",
        },
    }
}

#[component]
pub fn Page() -> Element {
    use_theme();
    let segment = current_agent_segment();
    let name = vmux_core::agent_setup::display_name(&segment).unwrap_or("Vibe");
    let command = vmux_core::agent_setup::install_command(&segment).unwrap_or_default();
    let tagline = tagline(&segment);
    let accent = accent(&segment);
    let emit_segment = segment.clone();
    rsx! {
        main { class: "relative flex min-h-screen items-center justify-center overflow-hidden bg-background p-10 text-foreground",
            div { class: "{accent.glow_top}" }
            div { class: "{accent.glow_bottom}" }

            section { class: "relative w-full max-w-lg rounded-3xl bg-white/[0.04] p-8 ring-1 ring-inset ring-white/10 backdrop-blur-2xl shadow-[0_24px_80px_-24px_rgba(0,0,0,0.7)]",
                div { class: "mb-6 flex items-center gap-4",
                    div { class: "{accent.badge}",
                        Icon { class: "h-6 w-6",
                            path { d: "M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" }
                            path { d: "M7 10l5 5 5-5" }
                            path { d: "M12 15V3" }
                        }
                    }
                    div { class: "min-w-0",
                        h1 { class: "text-xl font-semibold leading-tight tracking-tight", "Install {name} CLI" }
                        p { class: "text-sm text-muted-foreground", "{tagline}" }
                    }
                }

                p { class: "mb-5 text-sm leading-relaxed text-muted-foreground",
                    "vmux opened this page because the local "
                    code { class: "rounded bg-white/10 px-1.5 py-0.5 font-mono text-[0.8em] text-foreground", "{segment}" }
                    " command isn't installed yet. Run the command below to get it."
                }

                div { class: "mb-5 flex items-center gap-3 rounded-xl bg-black/40 p-4 ring-1 ring-inset ring-white/10",
                    span { class: "{accent.prompt}", "$" }
                    code { class: "min-w-0 flex-1 overflow-x-auto whitespace-nowrap font-mono text-sm text-foreground", "{command}" }
                }

                button {
                    class: "{accent.cta}",
                    onclick: move |_| {
                        let _ = try_cef_bin_emit_rkyv(&AgentInstallRunRequest { agent: emit_segment.clone() });
                    },
                    Icon { class: "h-4 w-4",
                        path { d: "M5 12h14" }
                        path { d: "m12 5 7 7-7 7" }
                    }
                    "Run install command"
                }

                p { class: "mt-3 text-center text-xs text-muted-foreground/70",
                    "vmux runs it in a terminal and reloads when "
                    code { class: "font-mono", "{segment}" }
                    " is ready."
                }
            }
        }
    }
}
