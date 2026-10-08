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
                Problem {}
                CoreModel {}
                Benefits {}
                AgentEra {}
                LearningPath {}
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
                    "Flexible by composition."
                    span { class: "block text-text-muted", "Predictable by design." }
                }
                p { class: "mx-auto mt-7 max-w-3xl text-lg leading-relaxed text-text-muted sm:text-xl",
                    "Build extensible cross-platform applications from typed feature plugins. One architecture for people, agents, UI, services, and every runtime."
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
fn Problem() -> Element {
    rsx! {
        section { class: "px-6 py-20 sm:py-28",
            div { class: "mx-auto max-w-4xl",
                p { class: "text-sm font-semibold uppercase tracking-[0.2em] text-accent", "Why Vmux" }
                h2 { class: "mt-3 text-3xl font-bold tracking-tight sm:text-5xl",
                    "Rich applications fragment fast."
                }
                p { class: "mt-6 max-w-3xl text-lg leading-relaxed text-text-muted",
                    "Browser content, native UI, files, processes, agents, services, and mobile clients each invite another state model, registry, and integration path. Flexibility grows together with architectural complexity."
                }
                p { class: "mt-4 max-w-3xl text-lg leading-relaxed text-text-muted",
                    "Vmux keeps them inside one composition model and a small typed vocabulary. New capability extends the graph instead of creating another architecture beside it."
                }
            }
        }
    }
}

#[component]
fn CoreModel() -> Element {
    rsx! {
        section { class: "px-6 py-20 sm:py-28",
            div { class: "mx-auto max-w-6xl",
                div { class: "mx-auto max-w-3xl text-center",
                    p { class: "text-sm font-semibold uppercase tracking-[0.2em] text-accent", "One composable core" }
                    h2 { class: "mt-3 text-3xl font-bold tracking-tight sm:text-5xl",
                        "Applications are plugin graphs."
                    }
                    p { class: "mt-5 text-lg leading-relaxed text-text-muted",
                        "Feature plugins and platform adapters install typed state, systems, events, and contracts into one ECS world. Each runtime selects the graph it needs."
                    }
                }
                div { class: "mt-12 grid items-stretch gap-4 md:grid-cols-[1fr_auto_1fr_auto_1fr]",
                    ModelNode { title: "Feature plugins", body: "UI, state, systems, commands, tools, persistence" }
                    div { class: "hidden items-center text-2xl text-accent md:flex", "+" }
                    ModelNode { title: "Platform adapters", body: "Desktop, mobile, service, CLI, MCP" }
                    div { class: "hidden items-center text-2xl text-accent md:flex", "→" }
                    ModelNode { title: "ECS world", body: "Typed composition, lifecycle, schedules, boundaries" }
                }
            }
        }
    }
}

#[component]
fn ModelNode(title: String, body: String) -> Element {
    rsx! {
        div { class: "rounded-2xl border border-aurora-violet/30 bg-aurora-violet/10 p-7",
            h3 { class: "text-xl font-semibold", "{title}" }
            p { class: "mt-3 text-sm leading-relaxed text-text-muted", "{body}" }
        }
    }
}

#[component]
fn Benefits() -> Element {
    rsx! {
        section { class: "px-6 py-20 sm:py-28",
            div { class: "mx-auto max-w-6xl",
                p { class: "text-sm font-semibold uppercase tracking-[0.2em] text-accent", "Out of the box" }
                h2 { class: "mt-3 max-w-3xl text-3xl font-bold tracking-tight sm:text-5xl",
                    "Complex capability. Small set of design choices."
                }
                div { class: "mt-12 grid gap-4 md:grid-cols-2 lg:grid-cols-3",
                    Benefit { title: "Runtime composition", body: "Select plugins and adapters instead of rebuilding features for every platform." }
                    Benefit { title: "Feature ownership", body: "One crate owns a capability from UI and state through persistence and contracts." }
                    Benefit { title: "Typed boundaries", body: "Events, messages, and shared values replace callback registries and mirror DTOs." }
                    Benefit { title: "Observable lifecycle", body: "Long work has identity and state that systems, UI, tests, and agents can inspect." }
                    Benefit { title: "Host-authoritative UI", body: "ECS owns product decisions. Pages render projections and emit typed intent." }
                    Benefit { title: "Predictable changes", body: "Eight primitives tell a human or agent where each part of a feature belongs." }
                }
            }
        }
    }
}

#[component]
fn Benefit(title: String, body: String) -> Element {
    rsx! {
        article { class: "rounded-2xl border border-border bg-surface/70 p-7",
            h3 { class: "text-xl font-semibold", "{title}" }
            p { class: "mt-3 leading-relaxed text-text-muted", "{body}" }
        }
    }
}

#[component]
fn AgentEra() -> Element {
    rsx! {
        section { class: "px-6 py-20 sm:py-28",
            div { class: "mx-auto max-w-5xl rounded-3xl border border-accent/30 bg-accent/10 p-8 sm:p-12",
                p { class: "text-sm font-semibold uppercase tracking-[0.2em] text-accent", "Built for the agent era" }
                h2 { class: "mt-4 text-3xl font-bold tracking-tight sm:text-5xl",
                    "The bottleneck is trusting the diff."
                }
                p { class: "mt-5 max-w-3xl text-lg leading-relaxed text-text-muted",
                    "Crate, Plugin, Entity, Component, System, Event, UiEvent, and UiState make ownership and flow explicit. People and coding agents follow the same path from feature idea to a PR that fits—no surprise integration layer."
                }
            }
        }
    }
}

#[component]
fn LearningPath() -> Element {
    rsx! {
        section { class: "px-6 py-20 sm:py-28",
            div { class: "mx-auto max-w-4xl",
                p { class: "text-sm font-semibold uppercase tracking-[0.2em] text-accent", "Learning path" }
                h2 { class: "mt-3 text-3xl font-bold tracking-tight sm:text-5xl", "Learn the model in order." }
                p { class: "mt-5 text-lg leading-relaxed text-text-muted",
                    "One architecture document. Read it once in sequence, then use it as a map while designing changes."
                }
                ol { class: "mt-10 divide-y divide-border border-y border-border",
                    LearningStep { number: "01", title: "Understand Plugin + ECS composition", href: "/docs/architecture#applications-as-plugin-graphs" }
                    LearningStep { number: "02", title: "Learn the eight design primitives", href: "/docs/architecture#design-vocabulary" }
                    LearningStep { number: "03", title: "Trace the default feature flow", href: "/docs/architecture#feature-design-flow" }
                    LearningStep { number: "04", title: "Apply the durable invariants", href: "/docs/architecture#invariants" }
                    LearningStep { number: "05", title: "Route a concrete change", href: "/docs/architecture#change-routing" }
                }
            }
        }
    }
}

#[component]
fn LearningStep(number: String, title: String, href: String) -> Element {
    rsx! {
        li {
            a { class: "group flex items-center gap-5 py-6 text-text no-underline", href: "{href}",
                span { class: "font-mono text-sm text-text-muted", "{number}" }
                span { class: "flex-1 font-semibold group-hover:text-accent", "{title}" }
                span { class: "text-text-muted group-hover:text-accent", "→" }
            }
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
