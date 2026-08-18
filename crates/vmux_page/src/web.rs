//! The web build's entry point: dispatches the window to the page its URL names.

#![allow(non_snake_case)]

use dioxus::prelude::*;

use crate::page_host::PageHost;

/// Renders whichever page this window's URL names.
#[component]
pub fn App() -> Element {
    let host = PageHost::current();
    for manifest in WEB_PAGE_MANIFESTS {
        if manifest.host == host.as_str() {
            return (manifest.render)();
        }
    }
    rsx! { UnknownPage { host: host.as_str().to_string() } }
}

impl PageHost {
    /// The page host of the window this build is running in.
    fn current() -> Self {
        let Some(location) = web_sys::window().map(|window| window.location()) else {
            return Self::of("", "");
        };
        let protocol = location.protocol().unwrap_or_default();
        let host = location.host().unwrap_or_default();
        Self::of(&protocol, &host)
    }
}

struct WebPageManifest {
    host: &'static str,
    render: fn() -> Element,
}

macro_rules! web_pages {
    ($($render:ident: $host:literal => $page:path),+ $(,)?) => {
        $(
            fn $render() -> Element {
                rsx! { $page {} }
            }
        )+

        const WEB_PAGE_MANIFESTS: &[WebPageManifest] = &[
            $(
                WebPageManifest {
                    host: $host,
                    render: $render,
                },
            )+
        ];
    };
}

web_pages! {
    render_layout: "layout" => vmux_layout::page::Page,
    render_terminal: "terminal" => vmux_terminal::page::Page,
    render_files: "files" => vmux_editor::page::Page,
    render_lsp: "lsp" => vmux_editor::lsp_page::Page,
    render_vault: "vault" => vmux_layout::vault_page::Page,
}

#[component]
fn UnknownPage(host: String) -> Element {
    use vmux_ui::i18n::{TranslationValue, translate_with};

    vmux_ui::hooks::use_theme();
    rsx! {
        div { class: "flex h-screen items-center justify-center bg-background text-foreground",
            div {
                class: "text-sm text-muted-foreground",
                {translate_with(
                    "error-unknown-host",
                    &[("host", TranslationValue::String(&host))],
                )}
            }
        }
    }
}
