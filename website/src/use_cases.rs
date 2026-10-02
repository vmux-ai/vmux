use dioxus::prelude::*;

use crate::Route;
use crate::site::{DownloadButton, SiteFooter, SiteNav};

#[component]
pub fn UseCases() -> Element {
    rsx! {
        div { id: "top", "data-tone": "dark", class: "min-h-screen bg-bg text-text",
            SiteNav {}
            main {
                Hero {}
                Cases {}
                Fit {}
                Start {}
            }
            SiteFooter {}
        }
    }
}

#[component]
fn Hero() -> Element {
    rsx! {
        section { class: "relative isolate overflow-hidden px-6 pb-20 pt-36 text-center sm:pb-28 sm:pt-44",
            div { class: "pointer-events-none absolute inset-0 -z-10",
                div { class: "absolute left-1/2 top-0 h-[34rem] w-[42rem] -translate-x-1/2 rounded-full bg-aurora-cyan/15 blur-[150px]" }
            }
            div { class: "mx-auto max-w-5xl",
                p { class: "text-sm font-semibold uppercase tracking-[0.24em] text-accent", "Use cases" }
                h1 { class: "mt-5 text-5xl font-bold tracking-[-0.05em] sm:text-7xl lg:text-8xl",
                    "Start with the browser."
                    span { class: "block text-text-muted", "Finish the task." }
                }
                p { class: "mx-auto mt-7 max-w-3xl text-lg leading-relaxed text-text-muted sm:text-xl",
                    "Research, build, review, and keep work moving with your team, any ACP agent, and full IDE support in one workspace."
                }
            }
        }
    }
}

#[component]
fn Cases() -> Element {
    rsx! {
        section { class: "px-6 py-16 sm:py-24",
            div { class: "mx-auto max-w-6xl space-y-4",
                UseCase {
                    number: "01",
                    title: "Research, compare, and act",
                    body: "Start with real browser pages. Keep sources visible while an agent summarizes options, checks details, or turns what you found into the next action.",
                    result: "The evidence and the work stay together.",
                }
                UseCase {
                    number: "02",
                    title: "Turn an idea into working software",
                    body: "Bring in an ACP agent when the task becomes code. Inspect files, run commands, open the product, and review the diff without moving the project into another app.",
                    result: "Browser simplicity with a full IDE underneath.",
                }
                UseCase {
                    number: "03",
                    title: "Run several jobs without losing your place",
                    body: "Keep each project, agent session, page, pane, and process in its own Space. Switch tasks and return to the same working state.",
                    result: "Parallel work without context collapse.",
                }
                UseCase {
                    number: "04",
                    title: "Hand work between people and agents",
                    body: "Review what an agent changed, take over manually, then hand the task back. Pages, files, terminals, and running work remain part of the same workspace.",
                    result: "Collaboration without a lossy handoff.",
                }
                UseCase {
                    number: "05",
                    title: "Pick up the same workspace remotely",
                    body: "Pair the iPhone app with your Mac and reconnect to supported pages and sessions through an end-to-end encrypted connection.",
                    result: "The work keeps its identity when the screen changes.",
                }
            }
        }
    }
}

#[component]
fn UseCase(number: String, title: String, body: String, result: String) -> Element {
    rsx! {
        article { class: "grid gap-5 rounded-3xl border border-border bg-surface/60 p-7 sm:p-10 md:grid-cols-[5rem_1fr]",
            div { class: "font-mono text-sm font-semibold text-accent", "{number}" }
            div {
                h2 { class: "text-2xl font-bold tracking-tight sm:text-4xl", "{title}" }
                p { class: "mt-4 max-w-3xl text-lg leading-relaxed text-text-muted", "{body}" }
                p { class: "mt-5 font-semibold text-text", "{result}" }
            }
        }
    }
}

#[component]
fn Fit() -> Element {
    rsx! {
        section { class: "px-6 py-20 sm:py-28",
            div { class: "mx-auto max-w-5xl text-center",
                h2 { class: "text-3xl font-bold tracking-tight sm:text-5xl", "Use only the depth the task needs." }
                div { class: "mt-10 grid gap-4 text-left sm:grid-cols-3",
                    FitPoint { title: "Browser first", body: "Open the web, keep sources visible, and work from familiar pages." }
                    FitPoint { title: "Agent when useful", body: "Choose any ACP-compatible agent and keep control of the workspace." }
                    FitPoint { title: "IDE when needed", body: "Drop into files, editor, terminal, Git, commands, and tools without switching products." }
                }
            }
        }
    }
}

#[component]
fn FitPoint(title: String, body: String) -> Element {
    rsx! {
        article { class: "rounded-2xl border border-border bg-black/20 p-6",
            h3 { class: "text-lg font-semibold", "{title}" }
            p { class: "mt-3 leading-relaxed text-text-muted", "{body}" }
        }
    }
}

#[component]
fn Start() -> Element {
    rsx! {
        section { class: "px-6 py-24 text-center sm:py-32",
            h2 { class: "text-4xl font-bold tracking-tight sm:text-6xl", "One prompt. Anything, done." }
            div { class: "mt-8 flex flex-wrap justify-center gap-3",
                DownloadButton {}
                Link {
                    class: "rounded-xl border border-border px-6 py-3 font-semibold text-text no-underline hover:border-accent hover:text-accent",
                    to: Route::Home {},
                    "Watch the demo"
                }
            }
        }
    }
}
