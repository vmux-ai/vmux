use dioxus::prelude::*;

use crate::Route;
use crate::site::{DownloadButton, InstallCard, SiteFooter, SiteNav};

#[component]
pub fn Landing() -> Element {
    rsx! {
        div { id: "top", "data-tone": "dark", class: "min-h-screen bg-bg text-text",
            SiteNav {}
            main {
                Hero {}
                Proof {}
                Install {}
                FrameworkTeaser {}
            }
            SiteFooter {}
        }
    }
}

#[component]
fn Hero() -> Element {
    rsx! {
        section { class: "relative isolate overflow-hidden px-6 pb-20 pt-32 text-center sm:pb-28 sm:pt-40",
            div { class: "pointer-events-none absolute inset-0 -z-10",
                div { class: "absolute left-1/2 top-0 h-[34rem] w-[42rem] -translate-x-1/2 rounded-full bg-accent/20 blur-[150px]" }
                div { class: "absolute left-[12%] top-64 h-72 w-72 rounded-full bg-aurora-cyan/15 blur-[120px]" }
                div { class: "absolute right-[12%] top-48 h-72 w-72 rounded-full bg-aurora-violet/15 blur-[120px]" }
            }
            div { class: "mx-auto max-w-4xl",
                p { class: "mb-5 text-sm font-semibold uppercase tracking-[0.24em] text-accent",
                    "The browser that gets sh*t done."
                }
                h1 { class: "text-5xl font-bold tracking-[-0.05em] sm:text-7xl lg:text-8xl",
                    "One prompt."
                    span { class: "block text-text-muted", "Anything, done." }
                }
                p { class: "mx-auto mt-7 max-w-2xl text-lg leading-relaxed text-text-muted sm:text-xl",
                    "One browser. Your team. Any ACP agent. Full IDE support."
                }
                div { class: "mt-9 flex flex-wrap items-center justify-center gap-3",
                    DownloadButton {}
                    a {
                        class: "rounded-xl border border-border px-6 py-3 font-semibold text-text no-underline transition-colors hover:border-accent hover:text-accent",
                        href: "#demo",
                        "Watch the demo"
                    }
                }
            }
            DemoVideo {}
        }
    }
}

#[component]
fn DemoVideo() -> Element {
    rsx! {
        figure { id: "demo", class: "mx-auto mt-16 max-w-6xl scroll-mt-24",
            div { class: "overflow-hidden rounded-2xl border border-white/10 bg-black shadow-2xl shadow-black/60",
                video {
                    class: "block h-auto w-full",
                    src: "/vmux-demo.mp4",
                    poster: "/vmux-demo.jpg",
                    controls: true,
                    muted: true,
                    r#loop: true,
                    playsinline: true,
                    preload: "metadata",
                    aria_label: "Vmux agent, browser, editor, and terminal demo",
                }
            }
            figcaption { class: "mt-4 text-sm text-text-muted",
                "A real Vmux session: inspect the work, split the workspace, and keep the agent moving."
            }
        }
    }
}

#[component]
fn Proof() -> Element {
    rsx! {
        section { class: "px-6 py-20 sm:py-28",
            div { class: "mx-auto max-w-6xl",
                h2 { class: "mx-auto max-w-3xl text-center text-3xl font-bold tracking-tight sm:text-5xl",
                    "A browser, an open agent harness, and a full IDE."
                }
                div { class: "mt-12 grid gap-4 md:grid-cols-3",
                    ProofPoint {
                        title: "Your team",
                        body: "People and agents work in the same spaces, pages, files, and running processes.",
                    }
                    ProofPoint {
                        title: "Any ACP agent",
                        body: "Use open ACP-compatible agents without locking your workspace to one provider.",
                    }
                    ProofPoint {
                        title: "Full IDE support",
                        body: "Browser, editor, terminal, Git, files, commands, and tools stay in one workspace.",
                    }
                }
            }
        }
    }
}

#[component]
fn ProofPoint(title: String, body: String) -> Element {
    rsx! {
        article { class: "rounded-2xl border border-border bg-surface/70 p-7",
            h3 { class: "text-xl font-semibold", "{title}" }
            p { class: "mt-3 leading-relaxed text-text-muted", "{body}" }
        }
    }
}

#[component]
fn Install() -> Element {
    rsx! {
        section { id: "install", class: "scroll-mt-20 px-6 py-20 text-center sm:py-28",
            div { class: "mx-auto max-w-3xl rounded-3xl border border-accent/25 bg-accent/10 px-6 py-14 sm:px-12",
                h2 { class: "text-4xl font-bold tracking-tight sm:text-6xl", "Start getting sh*t done." }
                p { class: "mx-auto mt-4 max-w-xl text-text-muted",
                    "Open-source early access for macOS 13.0 and later."
                }
                div { class: "mt-8 flex flex-col items-center gap-4",
                    InstallCard {}
                    DownloadButton {}
                }
            }
        }
    }
}

#[component]
fn FrameworkTeaser() -> Element {
    rsx! {
        section { class: "px-6 pb-24 pt-10 sm:pb-32",
            div { class: "mx-auto flex max-w-5xl flex-col gap-6 rounded-3xl border border-border bg-surface/60 p-8 sm:flex-row sm:items-center sm:justify-between sm:p-12",
                div { class: "max-w-2xl",
                    p { class: "text-sm font-semibold uppercase tracking-[0.2em] text-accent",
                        "Built on the Vmux Framework"
                    }
                    h2 { class: "mt-3 text-3xl font-bold tracking-tight sm:text-4xl",
                        "One IDE. One framework. Every platform."
                    }
                    p { class: "mt-4 leading-relaxed text-text-muted",
                        "Use Vmux with any stack, or build cross-platform applications from the same plugin architecture that powers Vmux itself."
                    }
                }
                Link {
                    class: "shrink-0 rounded-xl border border-accent px-5 py-3 font-semibold text-accent no-underline hover:bg-accent hover:text-black",
                    to: Route::FrameworkPage {},
                    "Explore the framework →"
                }
            }
        }
    }
}
