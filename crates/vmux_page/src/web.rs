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
    render_debug: "debug" => vmux_layout::debug_page::Page,
    render_error: "error" => vmux_layout::error_page::Page,
    render_terminal: "terminal" => vmux_terminal::page::Page,
    render_agent: "agent" => vmux_chat::page::Page,
    render_files: "files" => vmux_editor::page::Page,
    render_lsp: "lsp" => vmux_editor::lsp_page::Page,
    render_vault: "vault" => vmux_layout::vault_page::Page,
    render_extensions: "extensions" => vmux_layout::extensions_page::Page,
    render_start: "start" => StartAgentPage,
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

#[component]
fn StartAgentPage() -> Element {
    let mut transition = use_signal(InlineAgentWindow::pending);
    if let Some(active) = transition() {
        return rsx! {
            vmux_chat::page::Page {
                agent_override: Some(InlineAgentWindow::agent_id(&active.target_url)),
                transition_prompt: Some(active.prompt),
                transition_attachments: Some(active.attachments),
            }
        };
    }
    rsx! {
        vmux_layout::start::page::Page {
            on_inline_transition: move |next: vmux_command::page::StartInlineTransition| {
                vmux_layout::start::page::begin_agent_transition();
                InlineAgentWindow::set(&next.target_url);
                transition.set(Some(next));
            },
        }
    }
}

/// The start page hands off to an agent by navigating, which would drop the target. The agent
/// URL rides across the navigation in `window.name`.
struct InlineAgentWindow;

impl InlineAgentWindow {
    const PREFIX: &'static str = "vmux-inline-agent:";

    /// The transition this window was opened for, if it was opened for one.
    fn pending() -> Option<vmux_command::page::StartInlineTransition> {
        let name = web_sys::window()?.name().ok()?;
        let agent_url = name.strip_prefix(Self::PREFIX)?;
        Some(vmux_command::page::StartInlineTransition {
            target_url: agent_url.to_string(),
            prompt: String::new(),
            attachments: Vec::new(),
        })
    }

    fn set(agent_url: &str) {
        let Some(window) = web_sys::window() else {
            return;
        };
        let _ = window.set_name(&format!("{}{agent_url}", Self::PREFIX));
    }

    /// The `<id>` in `vmux://agent/<id>/...`.
    fn agent_id(agent_url: &str) -> String {
        let Some(path) = agent_url.strip_prefix("vmux://agent/") else {
            return "agent".to_string();
        };
        match path.split('/').next() {
            Some(segment) if !segment.is_empty() => segment.to_string(),
            _ => "agent".to_string(),
        }
    }
}
