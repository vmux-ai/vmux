use crate::webstore::ChromeWebStore;
use crate::{ExtensionInstallCompleted, ExtensionInstallRequest, OpenManagerRequest, store};
use bevy::prelude::*;
use bevy_cef::prelude::{
    Browsers, JsEmitEventPlugin, Receive, UiEventPlugin, UiInput, WebviewCommittedNavigationEvent,
};
use vmux_api::extension::ExtBrowseStoreRequest;

use super::super::template::Template;

pub(super) struct WebStorePlugin;

impl Plugin for WebStorePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(UiEventPlugin::<(ExtBrowseStoreRequest,)>::default())
            .add_plugins(JsEmitEventPlugin::<AddExtensionRequest>::default())
            .add_observer(browse_request)
            .add_observer(add)
            .add_systems(
                Update,
                (
                    inject_on_navigation,
                    bevy::ecs::schedule::ApplyDeferred,
                    inject_on_load.after(vmux_browser::BrowserLoadSet),
                    emit_install_result,
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

#[derive(bevy::ecs::system::SystemParam)]
struct WebStoreInjection<'w, 's> {
    browsers: NonSend<'w, Browsers>,
    injectors: Query<'w, 's, &'static WebStoreInjector>,
    profile: vmux_ecs::profile::CurrentProfile<'w, 's>,
    commands: Commands<'w, 's>,
}

impl WebStoreInjection<'_, '_> {
    fn inject(&mut self, webview: Entity, url: &str) {
        if !WebStoreUrl::matches(url) {
            self.commands.entity(webview).remove::<WebStoreInjector>();
            return;
        }
        let Some(extension_id) = ChromeWebStore::extension_id(url) else {
            self.commands.entity(webview).remove::<WebStoreInjector>();
            return;
        };
        let Some((profile, paths)) = self.profile.profile().zip(self.profile.paths()) else {
            return;
        };
        let profile = profile.clone().into_id();
        let injector = WebStoreInjector::resolve(self.injectors.get(webview).ok(), extension_id);
        let index = store::ExtensionStore::at(paths.extensions())
            .load_index()
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
        if let Ok(script) = Template::new(INJECTOR_JS).render(&replacements) {
            self.browsers.execute_js(&webview, &script);
        }
        self.commands.entity(webview).insert(injector);
    }
}

fn browse_request(
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
    mut injection: WebStoreInjection,
) {
    for event in events.read() {
        if !event.is_main_frame {
            continue;
        }
        injection.inject(event.webview, &event.url);
    }
}

fn inject_on_load(
    mut events: MessageReader<vmux_browser::WebviewLoadCompleted>,
    browser_meta: Query<&vmux_ecs::PageMetadata, With<vmux_layout::Browser>>,
    mut injection: WebStoreInjection,
) {
    for event in events.read() {
        let Ok(meta) = browser_meta.get(event.webview) else {
            continue;
        };
        injection.inject(event.webview, &meta.url);
    }
}

fn add(
    trigger: On<Receive<AddExtensionRequest>>,
    injectors: Query<&WebStoreInjector>,
    mut installs: MessageWriter<ExtensionInstallRequest>,
    mut manager: MessageWriter<OpenManagerRequest>,
) {
    let request = &trigger.payload;
    let Ok(injector) = injectors.get(trigger.event().webview) else {
        return;
    };
    let Some(id) = ChromeWebStore::extension_id(&request.id) else {
        return;
    };
    if injector.nonce != request.nonce || injector.extension_id != id {
        return;
    }
    match request.channel.as_str() {
        ADD_CHANNEL => {
            installs.write(ExtensionInstallRequest {
                source: id,
                requester: Some(trigger.event().webview),
            });
        }
        MANAGE_CHANNEL => {
            manager.write(OpenManagerRequest);
        }
        _ => {}
    }
}

fn emit_install_result(
    mut completed: MessageReader<ExtensionInstallCompleted>,
    browsers: NonSend<Browsers>,
) {
    for result in completed.read() {
        if !browsers.can_emit_to(&result.requester) {
            continue;
        }
        let detail = serde_json::json!({ "id": result.id, "success": result.success });
        let script = format!(
            "globalThis.dispatchEvent(new CustomEvent('__vmuxWebStoreInstallResult',{{detail:{detail}}}));"
        );
        browsers.execute_js(&result.requester, &script);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn web_store_injector_renders_without_page_globals() {
        let source = Template::new(INJECTOR_JS)
            .render(&[
                ("__VMUX_WEBSTORE_INSTALLED__", "[]".into()),
                ("__VMUX_WEBSTORE_NONCE__", "\"nonce\"".into()),
            ])
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
