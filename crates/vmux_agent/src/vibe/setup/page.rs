#![allow(non_snake_case)]

use crate::vibe::setup::event::AgentInstallRunRequest;
use dioxus::prelude::*;
use vmux_ui::agent_accent::agent_accent;
use vmux_ui::components::icon::Icon;
use vmux_ui::favicon::Favicon;
use vmux_ui::hooks::{try_cef_bin_emit_rkyv, use_theme};

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

#[component]
pub fn Page() -> Element {
    use_theme();
    let segment = current_agent_segment();
    let name = vmux_core::agent_setup::display_name(&segment).unwrap_or("Vibe");
    let command = vmux_core::agent_setup::install_command(&segment).unwrap_or_default();
    let tagline = tagline(&segment);
    let accent = agent_accent(&segment);
    let prompt_class = format!("select-none font-mono text-sm {}", accent.accent_text);
    let cta_class = format!(
        "group inline-flex w-full items-center justify-center gap-2 rounded-xl bg-gradient-to-br {} px-4 py-2.5 text-sm font-medium text-white {} transition-all hover:brightness-110 active:scale-[0.99]",
        accent.grad, accent.cta_shadow
    );
    let emit_segment = segment.clone();
    rsx! {
        main { class: "relative flex min-h-screen items-center justify-center overflow-hidden bg-background p-10 text-foreground",
            div { class: "{accent.glow_top}" }
            div { class: "{accent.glow_bottom}" }

            section { class: "relative w-full max-w-lg rounded-3xl bg-white/[0.04] p-8 ring-1 ring-inset ring-white/10 backdrop-blur-2xl shadow-[0_24px_80px_-24px_rgba(0,0,0,0.7)]",
                div { class: "mb-6 flex items-center gap-4",
                    div { class: "flex h-12 w-12 shrink-0 items-center justify-center rounded-2xl bg-white/[0.06] ring-1 ring-inset ring-white/10",
                        Favicon {
                            favicon_url: "".to_string(),
                            url: format!("vmux://agent/{segment}/cli/"),
                            class: "h-7 w-7 shrink-0 rounded-lg object-contain".to_string(),
                            globe_class: "h-7 w-7 text-muted-foreground".to_string(),
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
                    span { class: "{prompt_class}", "$" }
                    code { class: "min-w-0 flex-1 overflow-x-auto whitespace-nowrap font-mono text-sm text-foreground", "{command}" }
                }

                button {
                    class: "{cta_class}",
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
