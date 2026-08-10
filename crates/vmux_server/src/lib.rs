//! Serves embedded webview page bundles over `vmux://` URLs on the host, and on wasm
//! dispatches the web build to the correct per-host Dioxus page.

#[cfg(feature = "build")]
pub mod build;

#[cfg(not(web))]
pub use vmux_core::page::{
    PAGE_READY_BIN_EVENT_ID, PageManifest, PageReady, ServerEmbedSet, ServerPlugin,
    mark_webview_page_ready,
};

#[cfg(all(web, feature = "web"))]
use dioxus::prelude::*;

#[cfg(all(web, feature = "web"))]
#[allow(non_snake_case)]
#[component]
pub fn App() -> Element {
    let host = current_host();
    WEB_PAGE_MANIFESTS
        .iter()
        .find(|manifest| manifest.host == host)
        .map(|manifest| (manifest.render)())
        .unwrap_or_else(|| rsx! { UnknownPage { host } })
}

#[cfg(all(web, feature = "web"))]
struct WebPageManifest {
    host: &'static str,
    render: fn() -> Element,
}

#[cfg(all(web, feature = "web"))]
const INLINE_AGENT_WINDOW_PREFIX: &str = "vmux-inline-agent:";

#[cfg(all(web, feature = "web"))]
fn inline_transition() -> Option<vmux_layout::command_bar::palette::StartInlineTransition> {
    web_sys::window()
        .and_then(|window| window.name().ok())
        .and_then(|name| {
            name.strip_prefix(INLINE_AGENT_WINDOW_PREFIX)
                .map(str::to_string)
        })
        .map(
            |agent_url| vmux_layout::command_bar::palette::StartInlineTransition {
                target_url: agent_url,
                prompt: String::new(),
                attachments: Vec::new(),
            },
        )
}

#[cfg(all(web, feature = "web"))]
fn set_inline_agent_url(agent_url: &str) {
    if let Some(window) = web_sys::window() {
        let _ = window.set_name(&format!("{INLINE_AGENT_WINDOW_PREFIX}{agent_url}"));
    }
}

#[cfg(all(web, feature = "web"))]
fn inline_agent_id(agent_url: &str) -> String {
    agent_url
        .strip_prefix("vmux://agent/")
        .and_then(|path| path.split('/').next())
        .filter(|segment| !segment.is_empty())
        .unwrap_or("agent")
        .to_string()
}

#[cfg(all(web, feature = "web"))]
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

#[cfg(all(web, feature = "web"))]
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

#[cfg(all(web, feature = "web"))]
web_pages! {
    render_layout: "layout" => vmux_layout::page::Page,
    render_command_bar: "command-bar" => vmux_layout::command_bar::page::Page,
    render_debug: "debug" => vmux_layout::debug_page::Page,
    render_error: "error" => vmux_layout::error_page::Page,
    render_terminal: "terminal" => vmux_terminal::page::Page,
    render_services: "services" => vmux_service::page::Page,
    render_history: "history" => vmux_history::page::Page,
    render_spaces: "spaces" => vmux_space::page::Page,
    render_team: "team" => vmux_team::page::Page,
    render_settings: "settings" => vmux_setting::page::Page,
    render_agent: "agent" => vmux_chat::page::Page,
    render_agents: "agents" => vmux_agent::page::Page,
    render_files: "files" => vmux_editor::page::Page,
    render_lsp: "lsp" => vmux_editor::lsp_page::Page,
    render_tools: "tools" => vmux_layout::tools_page::Page,
    render_vault: "vault" => vmux_layout::vault_page::Page,
    render_extensions: "extensions" => vmux_layout::extensions_page::Page,
    render_start: "start" => StartAgentPage,
}

#[cfg(all(web, feature = "web"))]
fn current_host() -> String {
    let loc = web_sys::window().map(|window| window.location());
    let protocol = loc
        .as_ref()
        .and_then(|l| l.protocol().ok())
        .unwrap_or_default();
    let host = loc.as_ref().and_then(|l| l.host().ok()).unwrap_or_default();
    host_for(&protocol, &host)
}

#[cfg(any(test, all(web, feature = "web")))]
fn host_for(protocol: &str, host: &str) -> String {
    if protocol == "https:" && host == "vault.vmux.ai" {
        "vault".to_string()
    } else if matches!(protocol, "http:" | "https:") {
        "remote".to_string()
    } else if protocol == "file:" {
        "files".to_string()
    } else {
        host.to_string()
    }
}

#[cfg(all(not(web), test))]
mod host_tests {
    use super::*;

    #[test]
    fn files_protocol_routes_to_files_host() {
        assert_eq!(host_for("file:", ""), "files");
        assert_eq!(host_for("vmux:", "terminal"), "terminal");
        assert_eq!(host_for("https:", "vault.vmux.ai"), "vault");
        assert_eq!(host_for("https:", "example.com"), "remote");
        assert_eq!(host_for("https:", "mac.example.ts.net"), "remote");
    }
}

#[cfg(all(web, feature = "web"))]
#[allow(non_snake_case)]
#[component]
fn StartAgentPage() -> Element {
    let mut transition = use_signal(inline_transition);
    if let Some(active) = transition() {
        return rsx! {
            vmux_chat::page::Page {
                agent_override: Some(inline_agent_id(&active.target_url)),
                transition_prompt: Some(active.prompt),
                transition_attachments: Some(active.attachments),
            }
        };
    }
    rsx! {
        vmux_layout::start::page::Page {
            on_inline_transition: move |next: vmux_layout::command_bar::palette::StartInlineTransition| {
                vmux_layout::start::page::begin_agent_transition();
                set_inline_agent_url(&next.target_url);
                transition.set(Some(next));
            },
        }
    }
}
