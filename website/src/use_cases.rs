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
                    "Four prompts."
                    span { class: "block text-text-muted", "Four things done." }
                }
                p { class: "mx-auto mt-7 max-w-3xl text-lg leading-relaxed text-text-muted sm:text-xl",
                    "Give Vmux a real job, follow the work in the browser, and take over whenever you want."
                }
            }
        }
    }
}

#[component]
fn Cases() -> Element {
    rsx! {
        section { class: "px-6 pb-16 sm:pb-24",
            div { class: "mx-auto max-w-6xl space-y-8",
                UseCase {
                    number: "01",
                    label: "Software",
                    title: "Fix anything in Vmux.",
                    prompt: "Fix this issue in Vmux. Reproduce it, patch it, run the right checks, and leave the branch ready for review.",
                    result: "Issue, source, terminal, running app, and diff stay in one workspace. Review the evidence—not an agent summary.",
                    steps: vec!["Inspect the issue and code".to_string(), "Edit and run focused checks".to_string(), "Review the app and diff".to_string()],
                }
                UseCase {
                    number: "02",
                    label: "Travel",
                    title: "Find the best Paris → Tokyo flight.",
                    prompt: "Find the best round trip from Paris to Tokyo. Max one stop, checked bag included, sensible departure times, under €900. Compare the real total before I book.",
                    result: "Keep the live search pages and constraints visible, compare the final price—not the teaser fare—and verify the shortlist yourself.",
                    steps: vec!["Search live options".to_string(), "Compare price, duration, and rules".to_string(), "Keep the best three open".to_string()],
                }
                UseCase {
                    number: "03",
                    label: "Forward-deployed engineering",
                    title: "Ship a client PoC before tomorrow.",
                    prompt: "Build a working dispatch dashboard from this spreadsheet and these API docs. Deploy a shareable PoC for tomorrow's client review.",
                    result: "Research, implementation, terminal, live preview, and deployment stay together while you steer around client-specific constraints.",
                    steps: vec!["Read the client's data and docs".to_string(), "Build the smallest useful workflow".to_string(), "Deploy a link the client can try".to_string()],
                }
                UseCase {
                    number: "04",
                    label: "Small business",
                    title: "Launch your restaurant website.",
                    prompt: "Build and deploy a bilingual website for my new restaurant. Use these photos and menu, then add reservations, a map, opening hours, and a great mobile layout.",
                    result: "Turn the owner's real assets and decisions into a working site, then keep the browser open for the final visual review.",
                    steps: vec!["Research the neighborhood and references".to_string(), "Build from the real menu and photos".to_string(), "Review mobile and deploy".to_string()],
                }
            }
        }
    }
}

#[component]
fn UseCase(
    number: String,
    label: String,
    title: String,
    prompt: String,
    result: String,
    steps: Vec<String>,
) -> Element {
    rsx! {
        article { class: "overflow-hidden rounded-3xl border border-border bg-surface/60",
            div { class: "grid gap-8 p-7 sm:p-10 lg:grid-cols-[minmax(0,1fr)_minmax(22rem,0.9fr)] lg:items-center",
                div {
                    div { class: "flex items-center gap-3 font-mono text-xs font-semibold uppercase tracking-[0.16em] text-accent",
                        span { "{number}" }
                        span { class: "h-px w-8 bg-accent/50" }
                        span { "{label}" }
                    }
                    h2 { class: "mt-5 text-3xl font-bold tracking-tight sm:text-5xl", "{title}" }
                    div { class: "mt-7 rounded-2xl border border-white/10 bg-black/30 p-5",
                        p { class: "mb-2 text-xs font-semibold uppercase tracking-[0.16em] text-text-muted", "Prompt" }
                        p { class: "font-mono text-sm leading-relaxed text-text sm:text-base", "“{prompt}”" }
                    }
                    p { class: "mt-6 text-lg leading-relaxed text-text-muted", "{result}" }
                }
                DemoPlaceholder { steps }
            }
        }
    }
}

#[component]
fn DemoPlaceholder(steps: Vec<String>) -> Element {
    rsx! {
        div { class: "relative aspect-video overflow-hidden rounded-2xl border border-white/10 bg-black/50 p-6 sm:p-8",
            div { class: "absolute inset-0 bg-[radial-gradient(circle_at_top_right,rgba(124,138,255,0.18),transparent_55%)]" }
            div { class: "relative flex h-full flex-col justify-between",
                div { class: "flex items-center justify-between",
                    p { class: "text-xs font-semibold uppercase tracking-[0.16em] text-text-muted", "Demo coming soon" }
                    span { class: "rounded-full border border-white/10 px-3 py-1 font-mono text-xs text-text-muted", "30–45 sec" }
                }
                ol { class: "space-y-3",
                    for (index , step) in steps.iter().enumerate() {
                        li { class: "flex items-center gap-3 text-sm sm:text-base",
                            span { class: "grid h-7 w-7 shrink-0 place-items-center rounded-full border border-accent/40 font-mono text-xs text-accent",
                                "{index + 1}"
                            }
                            span { class: "font-semibold", "{step}" }
                        }
                    }
                }
                p { class: "text-xs text-text-muted", "Full-screen Vmux capture · no mockup · works without audio" }
            }
        }
    }
}

#[component]
fn Start() -> Element {
    rsx! {
        section { class: "px-6 py-24 text-center sm:py-32",
            h2 { class: "text-4xl font-bold tracking-tight sm:text-6xl", "What would you get done?" }
            p { class: "mx-auto mt-5 max-w-2xl text-lg text-text-muted",
                "Start in the browser. Bring in any ACP agent. Use the full IDE only when the job needs it."
            }
            div { class: "mt-8 flex flex-wrap justify-center gap-3",
                DownloadButton {}
                Link {
                    class: "rounded-xl border border-border px-6 py-3 font-semibold text-text no-underline hover:border-accent hover:text-accent",
                    to: Route::Home {},
                    "Watch Vmux"
                }
            }
        }
    }
}
