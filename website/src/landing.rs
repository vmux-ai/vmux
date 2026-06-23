mod agents;
mod browser;
mod coworking;
mod cta;
mod hero;
mod parts;
mod pillars;
mod platform;
mod scenes;
#[cfg(target_arch = "wasm32")]
mod scroll;

use agents::Agents;
use coworking::Coworking;
use cta::Cta;
use dioxus::prelude::*;
use hero::Hero;
use pillars::Pillars;
use platform::Platform;
use scenes::{InputScene, LayoutScene};

pub const ICON: Asset = asset!("/assets/icon.png");
pub const GITHUB_URL: &str = "https://github.com/vmux-ai/vmux";
pub const INSTALL_CMD: &str = "curl -fsSL https://vmux.ai/install | sh";

#[component]
fn Banner() -> Element {
    rsx! {
        header { class: "sticky top-0 z-50 backdrop-blur-md bg-bg/70 border-b border-border/60",
            nav { class: "max-w-5xl mx-auto flex items-center justify-between px-5 py-3",
                a {
                    class: "flex items-center gap-2 font-bold tracking-tight text-text no-underline hover:text-accent",
                    href: "#top",
                    img { src: ICON, alt: "Vmux", class: "w-6 h-6 rounded-md" }
                    "Vmux"
                }
                div { class: "flex items-center gap-2 sm:gap-3 text-sm",
                    a {
                        class: "no-underline text-text-muted hover:text-text px-2 py-1",
                        href: GITHUB_URL,
                        target: "_blank",
                        rel: "noopener noreferrer",
                        "GitHub"
                    }
                    Link {
                        class: "no-underline text-text-muted hover:text-text px-2 py-1",
                        to: crate::Route::DocsIndex {},
                        "Docs"
                    }
                    a {
                        class: "no-underline bg-accent text-black font-semibold rounded-lg px-4 py-1.5 hover:bg-accent-hover",
                        href: "#install",
                        "Install"
                    }
                }
            }
        }
    }
}

#[component]
pub fn Landing() -> Element {
    use_effect(move || {
        #[cfg(target_arch = "wasm32")]
        scroll::init();
    });
    rsx! {
        div { id: "top",
            Banner {}
            Hero {}
            Pillars {}
            Coworking {}
            Agents {}
            LayoutScene {}
            InputScene {}
            Platform {}
            Cta {}
            Footer {}
        }
    }
}

#[component]
fn Footer() -> Element {
    rsx! {
        footer { class: "text-center py-12 px-8 text-text-muted text-sm",
            p {
                a {
                    class: "text-text-muted no-underline hover:text-text",
                    href: GITHUB_URL,
                    target: "_blank",
                    rel: "noopener noreferrer",
                    "GitHub"
                }
                " · "
                a {
                    class: "text-text-muted no-underline hover:text-text",
                    href: "https://github.com/vmux-ai/vmux/blob/main/LICENSE",
                    target: "_blank",
                    rel: "noopener noreferrer",
                    "GPL-3.0 License"
                }
            }
        }
    }
}
