use dioxus::prelude::*;
use dioxus_primitives::toast::{ToastOptions, use_toast};

use crate::Route;
use crate::hooks::{use_clipboard_copy, use_dmg_download, use_is_mac};

pub const GITHUB_URL: &str = "https://github.com/vmux-ai/vmux";
pub const INSTALL_CMD: &str = "curl -fsSL https://vmux.ai/install | sh";

const ICON: Asset = asset!("/assets/icon.png");

#[component]
pub fn SiteNav() -> Element {
    rsx! {
        header { class: "fixed inset-x-0 top-4 z-50 flex justify-center px-4",
            nav { class: "glass flex items-center gap-1 rounded-full px-3 py-2 text-sm sm:gap-3 sm:px-4",
                Link {
                    class: "flex items-center gap-2 px-2 font-bold tracking-tight text-text no-underline hover:text-accent",
                    to: Route::Home {},
                    img { src: ICON, alt: "Vmux", class: "h-6 w-6 rounded-md" }
                    "Vmux"
                }
                Link {
                    class: "px-2 py-1 text-text-muted no-underline hover:text-text",
                    to: Route::UseCasesPage {},
                    "Use cases"
                }
                Link {
                    class: "hidden px-2 py-1 text-text-muted no-underline hover:text-text md:block",
                    to: Route::FrameworkPage {},
                    "Framework"
                }
                Link {
                    class: "hidden px-2 py-1 text-text-muted no-underline hover:text-text sm:block",
                    to: Route::DocsIndex {},
                    "Docs"
                }
                a {
                    class: "hidden px-2 py-1 text-text-muted no-underline hover:text-text sm:block",
                    href: GITHUB_URL,
                    target: "_blank",
                    rel: "noopener noreferrer",
                    "GitHub"
                }
                a {
                    class: "rounded-full bg-accent px-4 py-1.5 font-semibold text-black no-underline hover:bg-accent-hover",
                    href: "/#install",
                    "Install"
                }
            }
        }
    }
}

#[component]
pub fn InstallCard() -> Element {
    let toast = use_toast();
    let copy = use_clipboard_copy();

    rsx! {
        div { class: "glass inline-flex max-w-full flex-col items-center gap-2 rounded-xl px-4 py-3 text-sm sm:flex-row sm:gap-3 sm:text-base",
            code { class: "max-w-full overflow-x-auto font-mono text-accent", "{INSTALL_CMD}" }
            button {
                class: "rounded bg-accent px-3 py-1.5 text-sm font-semibold text-black hover:bg-accent-hover",
                onclick: move |_| {
                    copy(INSTALL_CMD.to_string());
                    toast.success("Copied!".to_string(), ToastOptions::new());
                },
                "Copy"
            }
        }
    }
}

#[component]
pub fn DownloadButton() -> Element {
    let toast = use_toast();
    let is_mac = use_is_mac();
    let download = use_dmg_download();

    rsx! {
        button {
            class: "rounded-xl bg-accent px-6 py-3 font-semibold text-black transition-colors hover:bg-accent-hover",
            onclick: move |_| {
                if is_mac {
                    download(());
                } else {
                    toast
                        .info(
                            "Not supported".to_string(),
                            ToastOptions::new()
                                .description("Windows and Linux packages are not available yet."),
                        );
                }
            },
            "Download for macOS"
        }
    }
}

#[component]
pub fn SiteFooter() -> Element {
    rsx! {
        footer { class: "border-t border-border px-8 py-10 text-center text-sm text-text-muted",
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
