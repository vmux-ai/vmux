use dioxus::prelude::*;
use dioxus_primitives::toast::{ToastOptions, use_toast};

use crate::hooks::{use_dmg_download, use_is_mac};
use crate::landing::ICON;
use crate::landing::parts::{InstallCard, scroll_cue};

#[component]
pub fn Hero() -> Element {
    let toast_api = use_toast();
    let is_mac = use_is_mac();
    let download = use_dmg_download();

    rsx! {
        section {
            "data-tone": "light",
            class: "relative isolate min-h-screen overflow-hidden flex flex-col items-center justify-center px-6 text-center bg-bg text-text",
            div {
                class: "pointer-events-none absolute inset-0 -z-10",
                style: "transform: translateY(calc(var(--sy, 0) * -0.04px))",
                video {
                    "data-hero-video": "1",
                    class: "absolute inset-0 h-full w-full object-cover opacity-60 mix-blend-screen motion-reduce:hidden",
                    autoplay: true,
                    muted: true,
                    "loop": true,
                    "playsinline": true,
                }
                div { class: "absolute left-1/2 top-1/4 h-[34rem] w-[34rem] -translate-x-1/2 rounded-full bg-accent/25 blur-[130px] animate-aurora motion-reduce:animate-none" }
                div { class: "absolute left-[18%] top-1/3 h-80 w-80 rounded-full bg-aurora-cyan/30 blur-[110px] animate-aurora [animation-delay:-7s] motion-reduce:animate-none" }
                div { class: "absolute right-[16%] top-1/4 h-80 w-80 rounded-full bg-aurora-violet/25 blur-[110px] animate-aurora [animation-delay:-13s] motion-reduce:animate-none" }
            }
            div { class: "relative mx-auto max-w-3xl reveal",
                img {
                    src: ICON,
                    alt: "Vmux icon",
                    class: "w-20 h-20 mb-8 inline-block rounded-3xl shadow-2xl shadow-accent/20",
                }
                h1 { class: "font-bold tracking-tight leading-[1.02] mb-6",
                    span { class: "block text-2xl sm:text-3xl text-text-muted", "It starts as" }
                    span { class: "block text-6xl sm:text-8xl text-text", "just a browser." }
                }
                p { class: "text-lg sm:text-2xl text-text-muted mb-10 max-w-xl mx-auto",
                    "The browser that bridges chat and IDE."
                }
                InstallCard {}
                div { class: "mt-6 flex justify-center",
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
}
