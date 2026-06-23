use dioxus::prelude::*;
use dioxus_primitives::toast::{ToastOptions, use_toast};

use crate::hooks::{use_dmg_download, use_is_mac};
use crate::landing::parts::InstallCard;

#[component]
pub fn Cta() -> Element {
    let toast_api = use_toast();
    let is_mac = use_is_mac();
    let download = use_dmg_download();

    rsx! {
        section {
            id: "install",
            "data-tone": "light",
            class: "relative isolate overflow-hidden scroll-mt-20 px-6 py-32 sm:py-40 text-center bg-bg text-text reveal",
            div { class: "pointer-events-none absolute inset-0 -z-10",
                div { class: "absolute left-1/2 top-1/2 h-[28rem] w-[28rem] -translate-x-1/2 -translate-y-1/2 rounded-full bg-accent/20 blur-[130px] animate-aurora motion-reduce:animate-none" }
                div { class: "absolute left-[24%] top-1/3 h-72 w-72 rounded-full bg-aurora-cyan/20 blur-[110px] animate-aurora [animation-delay:-7s] motion-reduce:animate-none" }
            }
            h2 { class: "text-5xl sm:text-7xl font-bold tracking-tight mb-8", "Install vmux." }
            div { class: "flex justify-center mb-6",
                InstallCard {}
            }
            div { class: "flex justify-center",
                button {
                    class: "inline-flex items-center px-7 py-3.5 rounded-xl text-base font-semibold bg-accent text-black cursor-pointer transition-colors hover:bg-accent-hover",
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
