#[cfg(feature = "build")]
pub mod build;

#[cfg(not(target_arch = "wasm32"))]
pub use vmux_core::page::{
    PAGE_READY_BIN_EVENT_ID, PageManifest, PageReady, ServerEmbedSet, ServerPlugin,
    mark_webview_page_ready,
};

#[cfg(all(target_arch = "wasm32", feature = "web"))]
use dioxus::prelude::*;

#[cfg(all(target_arch = "wasm32", feature = "web"))]
struct WebPageManifest {
    host: &'static str,
    render: fn() -> Element,
}

#[cfg(all(target_arch = "wasm32", feature = "web"))]
macro_rules! web_pages {
    ($($render:ident: $host:literal => $page:path),+ $(,)?) => {
        $(
            #[allow(non_snake_case)]
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

#[cfg(all(target_arch = "wasm32", feature = "web"))]
web_pages! {
    render_layout: "layout" => vmux_layout::page::Page,
    render_command_bar: "command-bar" => vmux_layout::command_bar::page::Page,
    render_terminal: "terminal" => vmux_terminal::page::Page,
    render_services: "services" => vmux_service::page::Page,
    render_history: "history" => vmux_history::page::Page,
    render_spaces: "spaces" => vmux_space::page::Page,
    render_settings: "settings" => vmux_setting::page::Page,
}

#[cfg(all(target_arch = "wasm32", feature = "web"))]
#[allow(non_snake_case)]
pub fn App() -> Element {
    let host = current_host();
    WEB_PAGE_MANIFESTS
        .iter()
        .find(|manifest| manifest.host == host)
        .map(|manifest| (manifest.render)())
        .unwrap_or_else(|| rsx! { UnknownPage { host } })
}

#[cfg(all(target_arch = "wasm32", feature = "web"))]
fn current_host() -> String {
    web_sys::window()
        .and_then(|window| window.location().host().ok())
        .unwrap_or_default()
}

#[cfg(all(target_arch = "wasm32", feature = "web"))]
#[component]
fn UnknownPage(host: String) -> Element {
    rsx! {
        div { class: "flex h-screen items-center justify-center bg-background text-foreground",
            div { class: "text-sm text-muted-foreground", "Unknown vmux app host: {host}" }
        }
    }
}
