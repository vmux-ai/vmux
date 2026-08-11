//! Why the agent stopped, shown inline in the transcript where the turn died.

use crate::clipboard::copy_to_clipboard;
use crate::event::ChatOpenPage;
use dioxus::prelude::*;
use vmux_ui::hooks::send;
use vmux_ui::i18n::translate;

/// Why the agent stopped, and the way out when the cause is a bad package version.
#[component]
pub(super) fn ChatErrorCard(message: String) -> Element {
    let is_startup = message.to_lowercase().contains("startup");
    let title = if is_startup {
        translate("agent-error-startup-title")
    } else {
        translate("common-error")
    };
    let copy_label = translate("common-copy");
    let copy_text = message.clone();
    rsx! {
        div { class: "flex flex-col gap-2 rounded-xl bg-red-500/[0.07] px-4 py-3 ring-1 ring-inset ring-red-500/20",
            div { class: "flex items-center gap-2",
                svg {
                    class: "h-4 w-4 shrink-0 text-red-500",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "1.8",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    path { d: "M10.3 3.9 1.8 18a2 2 0 0 0 1.7 3h17a2 2 0 0 0 1.7-3L13.7 3.9a2 2 0 0 0-3.4 0Z" }
                    path { d: "M12 9v4" }
                    path { d: "M12 17h.01" }
                }
                span { class: "text-sm font-semibold text-red-600 dark:text-red-300", "{title}" }
                button {
                    class: "ml-auto flex h-6 w-6 items-center justify-center rounded-md text-red-500/70 transition hover:bg-red-500/10 hover:text-red-500",
                    title: "{copy_label}",
                    aria_label: "{copy_label}",
                    onclick: move |_| copy_to_clipboard(&copy_text),
                    svg {
                        class: "h-3.5 w-3.5",
                        view_box: "0 0 24 24",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "1.8",
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        rect { x: "9", y: "9", width: "13", height: "13", rx: "2" }
                        path { d: "M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" }
                    }
                }
            }
            div { class: "max-h-40 overflow-auto whitespace-pre-wrap break-words rounded-lg bg-red-500/[0.06] px-3 py-2 font-mono text-[11px] leading-relaxed text-red-700/90 dark:text-red-200/80",
                "{message}"
            }
        }
        if is_version_error(&message) {
            div { class: "flex items-start gap-3 rounded-xl bg-foreground/[0.04] px-4 py-3 ring-1 ring-inset ring-foreground/10",
                div { class: "flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-amber-500/15 text-amber-500",
                    svg {
                        class: "h-4 w-4",
                        view_box: "0 0 24 24",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "1.8",
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        path { d: "M9 18h6" }
                        path { d: "M10 22h4" }
                        path { d: "M12 2a7 7 0 0 0-4 12.7c.6.5 1 1.3 1 2.1h6c0-.8.4-1.6 1-2.1A7 7 0 0 0 12 2Z" }
                    }
                }
                div { class: "flex min-w-0 flex-1 flex-col gap-2.5",
                    p { class: "text-sm leading-relaxed text-foreground", {translate("agent-error-version-suggestion")} }
                    button {
                        class: "vmux-gradient-outline inline-flex items-center gap-2 self-end rounded-xl px-6 py-3 text-sm font-semibold transition hover:-translate-y-0.5 hover:shadow-lg active:scale-[0.98]",
                        onclick: move |_| {
                            let _ = send(&ChatOpenPage { url: "vmux://agents".to_string() });
                        },
                        svg {
                            class: "h-4 w-4 text-indigo-500",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "1.8",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            path { d: "M15 3h6v6" }
                            path { d: "M10 14 21 3" }
                            path { d: "M21 14v5a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5" }
                        }
                        span { class: "bg-gradient-to-r from-indigo-500 via-purple-500 to-pink-500 bg-clip-text text-transparent",
                            {translate("agent-error-open-agents")}
                        }
                    }
                }
            }
        }
    }
}

/// Whether a startup/run error looks like a package registry/version block (npm 403, security
/// policy, forbidden version) — where the fix is usually pinning a different version.
fn is_version_error(message: &str) -> bool {
    let lower = message.to_lowercase();
    [
        "403",
        "404",
        "forbidden",
        "security policy",
        "blocked",
        "eacces",
        "invalid tag",
        "einvalidtagname",
        "etarget",
        "no matching version",
        "notarget",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}
