mod browser;
mod coworking;
mod cta;
mod hero;
mod ide;
mod parts;
mod platform;
#[cfg(target_arch = "wasm32")]
mod scroll;
mod visit;

use browser::Browser;
use coworking::Coworking;
use cta::Cta;
use dioxus::prelude::*;
use hero::Hero;
use ide::Ide;
use platform::Platform;
use visit::Visit;

pub const ICON: Asset = asset!("/assets/icon.png");
pub const GITHUB_URL: &str = "https://github.com/vmux-ai/vmux";
pub const INSTALL_CMD: &str = "curl -fsSL https://vmux.ai/install | sh";

#[component]
pub fn Landing() -> Element {
    use_effect(move || {
        #[cfg(target_arch = "wasm32")]
        scroll::init();
    });
    rsx! {
        div { id: "top", class: "bg-bg",
            {parts::nav_pill()}
            Hero {}
            Browser {}
            Visit {}
            Ide {}
            Coworking {}
            Platform {}
            Cta {}
            Footer {}
        }
    }
}

#[component]
fn Footer() -> Element {
    rsx! {
        footer {
            "data-tone": "light",
            class: "bg-bg text-text-muted text-center py-12 px-8 text-sm",
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
