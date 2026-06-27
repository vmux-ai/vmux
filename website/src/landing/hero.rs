use dioxus::prelude::*;
use dioxus_primitives::toast::{ToastOptions, use_toast};

use crate::hooks::{use_dmg_download, use_is_mac};
use crate::landing::parts::{InstallCard, headline, scroll_cue};
use crate::landing::showcase::vmux_demo;

#[component]
pub fn Hero() -> Element {
    let toast_api = use_toast();
    let is_mac = use_is_mac();
    let download = use_dmg_download();

    rsx! {
        section {
            "data-tone": "light",
            class: "relative isolate min-h-screen overflow-hidden flex flex-col items-center justify-start px-6 pb-24 text-center bg-bg text-text",
            style: "padding-top: 7.5rem",
            div { class: "pointer-events-none absolute inset-0 -z-10",
                div { class: "absolute left-1/2 top-1/4 h-[34rem] w-[34rem] -translate-x-1/2 rounded-full bg-accent/20 blur-[140px] animate-aurora motion-reduce:animate-none" }
                div { class: "absolute left-[16%] top-1/3 h-80 w-80 rounded-full bg-aurora-cyan/20 blur-[120px] animate-aurora [animation-delay:-7s] motion-reduce:animate-none" }
                div { class: "absolute right-[16%] top-1/4 h-80 w-80 rounded-full bg-aurora-violet/20 blur-[120px] animate-aurora [animation-delay:-13s] motion-reduce:animate-none" }
            }
            div { class: "relative mx-auto max-w-2xl reveal",
                {headline("The browser", "One prompt.", "Anything, done.")}
                p { class: "mt-5 text-lg sm:text-xl text-text-muted max-w-xl mx-auto reveal",
                    "The browser + IDE that get sh*t done — booking a flight, building a website, opening a PR, all handled by your agents while you watch."
                }
            }
            div { class: "relative mt-12 w-full", {vmux_demo()} }
            div { class: "relative mt-12 flex flex-col items-center gap-4 reveal",
                InstallCard {}
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
            {scroll_cue()}
        }
    }
}
