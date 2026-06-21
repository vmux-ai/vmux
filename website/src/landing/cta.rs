use dioxus::prelude::*;
use dioxus_primitives::toast::{ToastOptions, use_toast};

use crate::hooks::{use_clipboard_copy, use_dmg_download, use_is_mac};
use crate::landing::INSTALL_CMD;

#[component]
pub fn Cta() -> Element {
    let toast_api = use_toast();
    let is_mac = use_is_mac();
    let copy = use_clipboard_copy();
    let download = use_dmg_download();

    rsx! {
        section {
            id: "install",
            class: "relative overflow-hidden scroll-mt-20 px-6 py-28 sm:py-36 text-center",
            div { class: "pointer-events-none absolute inset-0 -z-10",
                div { class: "absolute left-1/2 top-1/2 h-[26rem] w-[26rem] -translate-x-1/2 -translate-y-1/2 rounded-full bg-accent/25 blur-[120px]" }
            }
            h2 { class: "text-4xl sm:text-6xl font-bold tracking-tight mb-6",
                "Install Vmux."
            }
            div { class: "inline-flex flex-col sm:flex-row items-center gap-2 sm:gap-3 bg-code-bg/80 backdrop-blur border border-border rounded-lg px-4 py-3 text-sm sm:text-base mb-6",
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
            div { class: "flex justify-center",
                button {
                    class: "inline-flex items-center px-7 py-3.5 rounded-lg text-base font-semibold border border-transparent bg-accent text-black cursor-pointer transition-colors hover:bg-accent-hover",
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
            }
            p { class: "mt-5 text-sm text-text-muted", "Requires macOS 13.0 (Ventura) or later." }
        }
    }
}
