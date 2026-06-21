mod coworking;
mod cta;
mod hero;
mod pillars;
mod platform;
mod scenes;

use dioxus::prelude::*;

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
    rsx! {
        div { id: "top",
            Banner {}
            Hero {}
            Features {}
            Footer {}
        }
    }
}

#[component]
fn Hero() -> Element {
    use crate::hooks::{use_clipboard_copy, use_dmg_download, use_is_mac};
    use dioxus_primitives::toast::{ToastOptions, use_toast};

    let toast_api = use_toast();
    let is_mac = use_is_mac();
    let copy = use_clipboard_copy();
    let download = use_dmg_download();

    rsx! {
        section { class: "text-center max-w-3xl mx-auto pt-16 pb-12 px-6 sm:pt-24 sm:pb-16 sm:px-8",
            img {
                src: ICON,
                alt: "Vmux icon",
                class: "w-32 h-32 mb-6 inline-block rounded-3xl",
            }
            h1 { class: "text-4xl sm:text-5xl font-bold mb-2 tracking-tight", "Vmux" }
            p { class: "text-base sm:text-xl text-text-muted mb-10 max-w-xl mx-auto",
                "Vibe Multiplexer — an agent-first workspace with a browser and IDE built in."
            }
            div { class: "flex flex-wrap justify-center gap-3 mb-6",
                button {
                    class: "inline-flex items-center px-6 py-3 rounded-lg text-base font-semibold border border-transparent bg-accent text-black cursor-pointer transition-colors hover:bg-accent-hover",
                    onclick: move |_| {
                        if is_mac {
                            download(());
                        } else {
                            toast_api
                                .info(
                                    "Not supported".to_string(),
                                    ToastOptions::new()
                                        .description("Windows/Linux not supported yet — see GitHub Releases"),
                                );
                        }
                    },
                    "Download .dmg"
                }
                a {
                    class: "inline-flex items-center px-6 py-3 rounded-lg text-base font-semibold no-underline border border-border bg-transparent text-text transition-colors hover:border-accent hover:text-accent",
                    href: GITHUB_URL,
                    target: "_blank",
                    "GitHub"
                }
                Link {
                    class: "inline-flex items-center px-6 py-3 rounded-lg text-base font-semibold no-underline border border-border bg-transparent text-text transition-colors hover:border-accent hover:text-accent",
                    to: crate::Route::DocsIndex {},
                    "Docs"
                }
            }
            div { class: "inline-flex flex-col sm:flex-row items-center gap-2 sm:gap-3 bg-code-bg border border-border rounded-lg px-4 py-3 text-sm sm:text-base mb-4",
                code { class: "font-mono text-accent", "{INSTALL_CMD}" }
                button {
                    class: "bg-accent text-black border-0 rounded px-3 py-1.5 text-sm font-semibold cursor-pointer transition-colors hover:bg-accent-hover",
                    onclick: move |_| {
                        copy(INSTALL_CMD.to_string());
                        toast_api.success("Copied!".to_string(), ToastOptions::new());
                    },
                    "Copy"
                }
            }
            p { class: "text-sm text-text-muted", "Requires macOS 13.0 (Ventura) or later." }
        }
    }
}

#[component]
fn Features() -> Element {
    let features = [
        (
            "Co-work with agents",
            "People and agents build side by side in one shared space — from hands-on pairing to full autonomy, you set the balance.",
        ),
        (
            "Browser simplicity, tmux power",
            "Looks like the browser you already know; split, stack, and tile panes like tmux underneath.",
        ),
        (
            "IDE power underneath",
            "Keyboard-driven workflows and deep environment control — and agents drive the whole workspace over MCP.",
        ),
        (
            "3D workspace",
            "Powered by Bevy. Flip your panes into a live, GPU-rendered 3D scene — same workspace, still interactive.",
        ),
    ];

    rsx! {
        section { class: "max-w-3xl mx-auto py-12 px-8",
            h2 { class: "text-center text-3xl mb-8", "Features" }
            div { class: "grid grid-cols-1 md:grid-cols-2 gap-5",
                for (title , desc) in features {
                    div { class: "bg-surface border border-border rounded-xl p-6",
                        h3 { class: "text-base mb-2 text-accent", "{title}" }
                        p { class: "text-sm text-text-muted leading-relaxed", "{desc}" }
                    }
                }
            }
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
                    "GitHub"
                }
                " · "
                a {
                    class: "text-text-muted no-underline hover:text-text",
                    href: "https://github.com/vmux-ai/vmux/blob/main/LICENSE",
                    target: "_blank",
                    "MIT License"
                }
            }
        }
    }
}
