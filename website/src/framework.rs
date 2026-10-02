use dioxus::prelude::*;

use crate::Route;
use crate::site::{GITHUB_URL, SiteFooter, SiteNav};

#[component]
pub fn Framework() -> Element {
    rsx! {
        div { id: "top", "data-tone": "dark", class: "min-h-screen bg-bg text-text",
            SiteNav {}
            main {
                Hero {}
                Foundation {}
                Direction {}
                Explore {}
            }
            SiteFooter {}
        }
    }
}

#[component]
fn Hero() -> Element {
    rsx! {
        section { class: "relative isolate overflow-hidden px-6 pb-24 pt-36 text-center sm:pb-32 sm:pt-44",
            div { class: "pointer-events-none absolute inset-0 -z-10",
                div { class: "absolute left-1/2 top-0 h-[34rem] w-[42rem] -translate-x-1/2 rounded-full bg-aurora-violet/20 blur-[150px]" }
            }
            div { class: "mx-auto max-w-5xl",
                p { class: "text-sm font-semibold uppercase tracking-[0.24em] text-accent",
                    "Vmux Framework"
                }
                h1 { class: "mt-5 text-5xl font-bold tracking-[-0.05em] sm:text-7xl lg:text-8xl",
                    "One IDE."
                    span { class: "block text-text-muted", "One framework. Every platform." }
                }
                p { class: "mx-auto mt-7 max-w-3xl text-lg leading-relaxed text-text-muted sm:text-xl",
                    "Use Vmux IDE and any ACP agent with any stack. Choose Vmux Framework when you want one application architecture across UI, services, agents, and platforms."
                }
                div { class: "mt-9 flex flex-wrap justify-center gap-3",
                    Link {
                        class: "rounded-xl bg-accent px-6 py-3 font-semibold text-black no-underline hover:bg-accent-hover",
                        to: Route::DocsIndex {},
                        "Read the architecture"
                    }
                    a {
                        class: "rounded-xl border border-border px-6 py-3 font-semibold text-text no-underline hover:border-accent hover:text-accent",
                        href: GITHUB_URL,
                        target: "_blank",
                        rel: "noopener noreferrer",
                        "View on GitHub"
                    }
                }
            }
        }
    }
}

#[component]
fn Foundation() -> Element {
    rsx! {
        section { class: "px-6 py-20 sm:py-28",
            div { class: "mx-auto max-w-6xl",
                div { class: "mx-auto max-w-3xl text-center",
                    p { class: "text-sm font-semibold uppercase tracking-[0.2em] text-accent",
                        "Available today"
                    }
                    h2 { class: "mt-3 text-3xl font-bold tracking-tight sm:text-5xl",
                        "The architecture already powering Vmux."
                    }
                }
                div { class: "mt-12 grid gap-4 md:grid-cols-3",
                    FoundationPoint {
                        title: "Feature plugins",
                        body: "A feature owns its state, systems, UI, commands, tools, persistence, and contracts.",
                    }
                    FoundationPoint {
                        title: "Typed boundaries",
                        body: "The same Rust contract crosses UI, process, network, CLI, and agent boundaries.",
                    }
                    FoundationPoint {
                        title: "Runtime composition",
                        body: "Desktop, mobile, services, CLI, and MCP compose the capabilities they need.",
                    }
                }
            }
        }
    }
}

#[component]
fn FoundationPoint(title: String, body: String) -> Element {
    rsx! {
        article { class: "rounded-2xl border border-border bg-surface/70 p-7",
            h3 { class: "text-xl font-semibold", "{title}" }
            p { class: "mt-3 leading-relaxed text-text-muted", "{body}" }
        }
    }
}

#[component]
fn Direction() -> Element {
    rsx! {
        section { class: "px-6 py-20 sm:py-28",
            div { class: "mx-auto max-w-5xl rounded-3xl border border-aurora-violet/30 bg-aurora-violet/10 p-8 sm:p-12",
                div { class: "inline-flex rounded-full border border-aurora-violet/40 px-3 py-1 text-xs font-semibold uppercase tracking-[0.18em] text-aurora-violet",
                    "Where Vmux is going"
                }
                h2 { class: "mt-5 text-3xl font-bold tracking-tight sm:text-5xl",
                    "A harness for building with the framework."
                }
                p { class: "mt-5 max-w-3xl text-lg leading-relaxed text-text-muted",
                    "Inspect plugin graphs, preview capabilities, and let agents create, build, run, package, and distribute cross-platform applications from one environment."
                }
                div { class: "mt-8 grid gap-4 sm:grid-cols-3",
                    DirectionPoint { title: "Plugin viewer", body: "See what each composition installs and exposes." }
                    DirectionPoint { title: "Composition tools", body: "Create and reshape applications through typed plugins." }
                    DirectionPoint { title: "Agent workflows", body: "Give ACP agents the same build and inspection surface." }
                }
            }
        }
    }
}

#[component]
fn DirectionPoint(title: String, body: String) -> Element {
    rsx! {
        div { class: "rounded-2xl border border-white/10 bg-black/20 p-5",
            h3 { class: "font-semibold text-text", "{title}" }
            p { class: "mt-2 text-sm leading-relaxed text-text-muted", "{body}" }
        }
    }
}

#[component]
fn Explore() -> Element {
    rsx! {
        section { class: "px-6 py-24 text-center sm:py-32",
            h2 { class: "text-4xl font-bold tracking-tight sm:text-6xl", "Build the whole app." }
            p { class: "mx-auto mt-5 max-w-2xl text-lg text-text-muted",
                "Vmux is the first application built on the framework—and the environment where the framework will be built."
            }
            Link {
                class: "mt-8 inline-flex rounded-xl border border-accent px-6 py-3 font-semibold text-accent no-underline hover:bg-accent hover:text-black",
                to: Route::Home {},
                "See Vmux in action"
            }
        }
    }
}
