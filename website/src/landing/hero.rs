use dioxus::prelude::*;
use dioxus_primitives::toast::{ToastOptions, use_toast};

use crate::hooks::{use_clipboard_copy, use_dmg_download, use_is_mac};
use crate::landing::{GITHUB_URL, ICON, INSTALL_CMD};

#[component]
pub fn Hero() -> Element {
    let toast_api = use_toast();
    let is_mac = use_is_mac();
    let copy = use_clipboard_copy();
    let download = use_dmg_download();

    rsx! {
        section { class: "relative overflow-hidden text-center px-6 pt-24 pb-28 sm:pt-32 sm:pb-36",
            div { class: "pointer-events-none absolute inset-0 -z-10",
                div { class: "absolute left-1/2 top-24 h-[28rem] w-[28rem] -translate-x-1/2 rounded-full bg-accent/30 blur-[120px] animate-float motion-reduce:animate-none supports-[animation-timeline:scroll()]:[animation:parallax-up_linear_both] supports-[animation-timeline:scroll()]:[animation-timeline:scroll()]" }
                div { class: "absolute left-[20%] top-40 h-72 w-72 rounded-full bg-aurora-violet/25 blur-[100px] animate-float [animation-delay:-4s] motion-reduce:animate-none supports-[animation-timeline:scroll()]:[animation:parallax-up_linear_both] supports-[animation-timeline:scroll()]:[animation-timeline:scroll()]" }
                div { class: "absolute right-[18%] top-32 h-72 w-72 rounded-full bg-aurora-cyan/20 blur-[100px] animate-float [animation-delay:-8s] motion-reduce:animate-none supports-[animation-timeline:scroll()]:[animation:parallax-up_linear_both] supports-[animation-timeline:scroll()]:[animation-timeline:scroll()]" }
            }
            div { class: "relative mx-auto max-w-3xl animate-fade-up motion-reduce:animate-none",
                img {
                    src: ICON,
                    alt: "Vmux icon",
                    class: "w-24 h-24 sm:w-28 sm:h-28 mb-6 inline-block rounded-3xl shadow-2xl shadow-accent/20",
                }
                h1 { class: "text-5xl sm:text-7xl font-bold tracking-tight mb-4",
                    "Vmux"
                }
                p { class: "text-lg sm:text-2xl text-text mb-3 max-w-2xl mx-auto",
                    "The workspace that bridges chat and IDE."
                }
                p { class: "text-base sm:text-lg text-text-muted mb-10 max-w-xl mx-auto",
                    "An agent-first workspace with a browser and IDE built in — co-work with agents in one shared space."
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
                div { class: "inline-flex flex-col sm:flex-row items-center gap-2 sm:gap-3 bg-code-bg/80 backdrop-blur border border-border rounded-lg px-4 py-3 text-sm sm:text-base mb-4",
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
}
