mod hooks;

use dioxus::prelude::*;
use dioxus_primitives::toast::{ToastOptions, ToastProvider, use_toast};

use hooks::{use_clipboard_copy, use_dmg_download, use_is_mac};

const ICON: Asset = asset!("/assets/icon.png");
const GITHUB_URL: &str = "https://github.com/vmux-ai/vmux";
const INSTALL_CMD: &str = "curl -fsSL https://vmux.ai/install | sh";

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    rsx! {
        ToastProvider {
            Hero {}
            Features {}
            Footer {}
        }
    }
}

#[component]
fn Hero() -> Element {
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
            p { class: "text-base sm:text-xl text-text-muted mb-10 max-w-md mx-auto",
                "Vibe Multiplexer — AI-native workspace combining browser and terminal panes."
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
            p { class: "text-sm text-text-muted",
                "Requires macOS 13.0 (Ventura) or later."
            }
        }
    }
}

#[component]
fn Features() -> Element {
    let features = [
        (
            "Vibe Driven Development",
            "Talk to your workspace. Browse, run commands, edit files — all in one place.",
        ),
        (
            "Tmux-like Tiling",
            "Split, arrange, and manage browser and terminal panes in a single window.",
        ),
        (
            "Built-in Chromium",
            "Browse the web, read docs, and use web apps right next to your terminal.",
        ),
        (
            "3D Workspace",
            "Powered by Bevy game engine. Your workspace lives in a GPU-rendered 3D scene.",
        ),
    ];

    rsx! {
        section { class: "max-w-3xl mx-auto py-12 px-8",
            h2 { class: "text-center text-3xl mb-8", "Features" }
            div { class: "grid grid-cols-1 md:grid-cols-2 gap-5",
                for (title, desc) in features {
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
