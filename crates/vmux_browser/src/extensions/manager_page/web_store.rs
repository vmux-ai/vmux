use bevy::prelude::*;
use bevy_cef::prelude::{
    Browsers, JsEmitEventPlugin, Receive, UiEventPlugin, UiInput, WebviewCommittedNavigationEvent,
};
use vmux_core::event::ExtBrowseStoreRequest;

use super::catalog::InstallRequest;

pub(super) struct WebStorePlugin;

impl Plugin for WebStorePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(UiEventPlugin::<(ExtBrowseStoreRequest,)>::default())
            .add_plugins(JsEmitEventPlugin::<AddExtensionRequest>::default())
            .add_observer(on_browse_request)
            .add_observer(on_add_extension)
            .add_systems(
                Update,
                (
                    inject_on_navigation,
                    bevy::ecs::schedule::ApplyDeferred,
                    inject_on_load.after(crate::page_life::drain_loading_state),
                )
                    .chain(),
            );
    }
}

#[derive(Component, Clone)]
struct WebStoreInjector {
    nonce: String,
    extension_id: String,
}

impl WebStoreInjector {
    fn resolve(current: Option<&Self>, extension_id: String) -> Self {
        if let Some(current) = current
            && current.extension_id == extension_id
        {
            return current.clone();
        }
        Self {
            nonce: uuid::Uuid::new_v4().to_string(),
            extension_id,
        }
    }
}

#[derive(serde::Deserialize)]
struct AddExtensionRequest {
    channel: String,
    id: String,
    nonce: String,
}

const ADD_CHANNEL: &str = "vmux-add-extension";
const MANAGE_CHANNEL: &str = "vmux-manage-extension";
const WEB_STORE_URL: &str = "https://chromewebstore.google.com/category/extensions";
const INJECTOR_JS: &str = include_str!("../add_to_vmux.js");

fn on_browse_request(
    trigger: On<UiInput<ExtBrowseStoreRequest>>,
    mut requests: MessageWriter<vmux_layout::stack::OpenRequest>,
) {
    let query = trigger.event().payload.query.trim();
    let url = if query.is_empty() {
        WEB_STORE_URL.to_string()
    } else {
        format!(
            "https://chromewebstore.google.com/search/{}",
            EncodedQuery::from(query)
        )
    };
    requests.write(vmux_layout::stack::OpenRequest { url: Some(url) });
}

struct EncodedQuery(String);

impl From<&str> for EncodedQuery {
    fn from(query: &str) -> Self {
        let mut encoded = String::with_capacity(query.len());
        for byte in query.bytes() {
            match byte {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                    encoded.push(byte as char)
                }
                _ => encoded.push_str(&format!("%{byte:02X}")),
            }
        }
        Self(encoded)
    }
}

impl std::fmt::Display for EncodedQuery {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

fn inject_page(
    webview: Entity,
    url: &str,
    browsers: &Browsers,
    current: Option<&WebStoreInjector>,
    commands: &mut Commands,
) {
    if !WebStoreUrl::matches(url) {
        commands.entity(webview).remove::<WebStoreInjector>();
        return;
    }
    let Some(extension_id) = vmux_core::extension::webstore::extension_id(url) else {
        commands.entity(webview).remove::<WebStoreInjector>();
        return;
    };
    let injector = WebStoreInjector::resolve(current, extension_id);
    let profile = vmux_core::profile::active_profile_name();
    let index = vmux_core::extension::store::Index::load(&vmux_core::extension::store::root())
        .unwrap_or_default();
    let installed = index
        .entries
        .iter()
        .filter(|entry| entry.installed_for(&profile))
        .map(|entry| entry.id.as_str())
        .collect::<Vec<_>>();
    let replacements = [
        (
            "__VMUX_WEBSTORE_INSTALLED__",
            serde_json::to_string(&installed).expect("serializable extension list"),
        ),
        (
            "__VMUX_WEBSTORE_NONCE__",
            serde_json::to_string(&injector.nonce).expect("serializable web store nonce"),
        ),
    ];
    if let Ok(script) = super::super::template::render(INJECTOR_JS, &replacements) {
        browsers.execute_js(&webview, &script);
    }
    commands.entity(webview).insert(injector);
}

struct WebStoreUrl;

impl WebStoreUrl {
    fn matches(url: &str) -> bool {
        url.strip_prefix("https://")
            .and_then(|rest| rest.split(['/', '?', '#']).next())
            .is_some_and(|authority| authority == "chromewebstore.google.com")
    }
}

fn inject_on_navigation(
    mut events: MessageReader<WebviewCommittedNavigationEvent>,
    browsers: NonSend<Browsers>,
    injectors: Query<&WebStoreInjector>,
    mut commands: Commands,
) {
    for event in events.read() {
        if !event.is_main_frame {
            continue;
        }
        inject_page(
            event.webview,
            &event.url,
            &browsers,
            injectors.get(event.webview).ok(),
            &mut commands,
        );
    }
}

fn inject_on_load(
    mut events: MessageReader<crate::WebviewLoadCompleted>,
    browsers: NonSend<Browsers>,
    browser_meta: Query<&vmux_core::PageMetadata, With<vmux_layout::Browser>>,
    injectors: Query<&WebStoreInjector>,
    mut commands: Commands,
) {
    for event in events.read() {
        let Ok(meta) = browser_meta.get(event.webview) else {
            continue;
        };
        inject_page(
            event.webview,
            &meta.url,
            &browsers,
            injectors.get(event.webview).ok(),
            &mut commands,
        );
    }
}

fn on_add_extension(
    trigger: On<Receive<AddExtensionRequest>>,
    injectors: Query<&WebStoreInjector>,
    mut installs: MessageWriter<InstallRequest>,
    mut pages: MessageWriter<vmux_layout::stack::OpenRequest>,
) {
    let request = &trigger.payload;
    let Ok(injector) = injectors.get(trigger.event().webview) else {
        return;
    };
    let Some(id) = vmux_core::extension::webstore::extension_id(&request.id) else {
        return;
    };
    if injector.nonce != request.nonce || injector.extension_id != id {
        return;
    }
    match request.channel.as_str() {
        ADD_CHANNEL => {
            installs.write(InstallRequest::web_store(id, trigger.event().webview));
        }
        MANAGE_CHANNEL => {
            pages.write(vmux_layout::stack::OpenRequest {
                url: Some("vmux://tools/extensions".to_string()),
            });
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn web_store_injector_renders_without_page_globals() {
        let source = super::super::super::template::render(
            INJECTOR_JS,
            &[
                ("__VMUX_WEBSTORE_INSTALLED__", "[]".into()),
                ("__VMUX_WEBSTORE_NONCE__", "\"nonce\"".into()),
            ],
        )
        .unwrap();

        assert!(!source.contains("__VMUX_"));
        assert!(!source.contains("window.__VMUX_NONCE__"));
        assert!(!source.contains("window.__VMUX_INSTALLED__"));
    }

    #[test]
    fn web_store_injector_reuses_nonce_for_same_extension() {
        let first = WebStoreInjector::resolve(None, "a".repeat(32));
        let same = WebStoreInjector::resolve(Some(&first), "a".repeat(32));
        let different = WebStoreInjector::resolve(Some(&first), "b".repeat(32));

        assert_eq!(same.nonce, first.nonce);
        assert_ne!(different.nonce, first.nonce);
    }
}
