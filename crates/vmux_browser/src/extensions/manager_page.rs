use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use bevy::prelude::*;
use bevy_cef::prelude::{
    BinEventEmitterPlugin, BinHostEmitEvent, BinReceive, Browsers, JsEmitEventPlugin, Receive,
    WebviewCommittedNavigationEvent,
};
use vmux_command::{AppCommand, BrowserCommand, open::OpenCommand};
use vmux_core::event::extension::{
    EXT_INSTALL_PROGRESS_EVENT, EXT_STATUS_EVENT, EXTENSIONS_LIST_EVENT, EXTENSIONS_PAGE_URL,
    ExtActionRequest, ExtBrowseStoreRequest, ExtInstallPhase, ExtInstallProgress, ExtListRequest,
    ExtOpenManagerRequest, ExtRow, ExtStatus, ExtStatusEvent, ExtToggleRequest,
    ExtUninstallRequest, ExtensionsEvent,
};
use vmux_core::extension::store;
use vmux_core::page::PrewarmPage;

const PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: "extensions",
    title: "Extensions",
    keywords: &["extension", "extensions", "chrome", "addon", "install"],
    icon: Some(vmux_core::BuiltinIcon::Puzzle),
    command_bar: true,
};

enum OutMsg {
    Progress(ExtInstallProgress),
    Status(ExtStatusEvent),
    List(ExtensionsEvent),
    WebStoreInstallResult { id: String, success: bool },
}

#[derive(Resource, Clone, Default)]
struct ExtOutbox(Arc<Mutex<Vec<(Entity, OutMsg)>>>);

#[derive(Resource, Default)]
struct ExtSubscribers(HashSet<Entity>);

struct WebStoreInjector {
    nonce: String,
    extension_id: String,
}

#[derive(Resource, Default)]
struct WebStoreInjectors(HashMap<Entity, WebStoreInjector>);

pub struct ExtensionsPlugin;

impl Plugin for ExtensionsPlugin {
    fn build(&self, app: &mut App) {
        app.world_mut().spawn((
            PAGE_MANIFEST,
            PrewarmPage {
                host: "extensions",
                url: EXTENSIONS_PAGE_URL,
                title: "Extensions",
                pool_size: 0,
            },
        ));
        vmux_core::register_host_spawn(app, "extensions");
        app.init_resource::<ExtOutbox>()
            .init_resource::<ExtSubscribers>()
            .init_resource::<WebStoreInjectors>()
            .add_plugins(BinEventEmitterPlugin::<(
                ExtToggleRequest,
                ExtUninstallRequest,
                ExtBrowseStoreRequest,
            )>::for_hosts(&["extensions"]))
            .add_plugins(BinEventEmitterPlugin::<(
                ExtListRequest,
                ExtActionRequest,
                ExtOpenManagerRequest,
            )>::for_hosts(&["extensions", "layout"]))
            .add_plugins(JsEmitEventPlugin::<AddExtensionRequest>::default())
            .add_observer(on_list_request)
            .add_observer(on_toggle_request)
            .add_observer(on_uninstall_request)
            .add_observer(on_action_request)
            .add_observer(on_open_manager_request)
            .add_observer(on_browse_store_request)
            .add_observer(on_add_extension)
            .add_systems(
                Update,
                (run_agent_installs, inject_on_cws_nav, drain_outbox),
            );
    }
}

fn push(outbox: &ExtOutbox, entity: Entity, msg: OutMsg) {
    outbox
        .0
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .push((entity, msg));
}

fn snapshot() -> ExtensionsEvent {
    let root = store::root();
    let profile = vmux_core::profile::active_profile_name();
    let idx = store::Index::load(&root).unwrap_or_default();
    let loaded = super::load::loaded_ids();
    let extensions = idx
        .entries
        .iter()
        .filter(|entry| entry.installed_for(&profile))
        .map(|e| {
            let enabled = e.enabled_for(&profile);
            let needs_approval = !e
                .grants_for(&profile)
                .covers(&e.permissions, &e.host_permissions);
            ExtRow {
                id: e.id.clone(),
                name: e.name.clone(),
                version: e.version.clone(),
                icon: e.icon.clone(),
                popup: e.popup.clone(),
                enabled,
                needs_approval,
                required_permissions: e.permissions.clone(),
                required_host_permissions: e.host_permissions.clone(),
                status: if enabled {
                    ExtStatus::Installed
                } else {
                    ExtStatus::Disabled
                },
            }
        })
        .collect();
    ExtensionsEvent {
        extensions,
        pending: idx.is_dirty_for(&profile, &loaded),
    }
}

fn broadcast_list(outbox: &ExtOutbox, subs: &ExtSubscribers) {
    let ev = snapshot();
    for &entity in &subs.0 {
        push(outbox, entity, OutMsg::List(ev.clone()));
    }
}

fn spawn_install(outbox: &ExtOutbox, subs: Vec<Entity>, source: String, requester: Option<Entity>) {
    let sink = outbox.clone();
    std::thread::spawn(move || {
        let key = source.clone();
        let prog_sink = sink.clone();
        let prog_subs = subs.clone();
        let result = super::install::install(
            &source,
            super::install::DEFAULT_PRODVERSION,
            |phase, pct, m| {
                for &entity in &prog_subs {
                    push(
                        &prog_sink,
                        entity,
                        OutMsg::Progress(ExtInstallProgress {
                            key: key.clone(),
                            phase,
                            pct,
                            message: m.to_string(),
                        }),
                    );
                }
            },
        );
        match result {
            Ok(entry) => {
                for &entity in &subs {
                    push(
                        &sink,
                        entity,
                        OutMsg::Status(ExtStatusEvent {
                            id: entry.id.clone(),
                            status: if entry.enabled_for(&vmux_core::profile::active_profile_name())
                            {
                                ExtStatus::Installed
                            } else {
                                ExtStatus::Disabled
                            },
                            version: Some(entry.version.clone()),
                        }),
                    );
                }
                if let Some(entity) = requester {
                    push(
                        &sink,
                        entity,
                        OutMsg::WebStoreInstallResult {
                            id: entry.id,
                            success: true,
                        },
                    );
                }
            }
            Err(e) => {
                for &entity in &subs {
                    push(
                        &sink,
                        entity,
                        OutMsg::Progress(ExtInstallProgress {
                            key: key.clone(),
                            phase: ExtInstallPhase::Failed,
                            pct: None,
                            message: e.clone(),
                        }),
                    );
                }
                if let Some(entity) = requester {
                    push(
                        &sink,
                        entity,
                        OutMsg::WebStoreInstallResult {
                            id: key.clone(),
                            success: false,
                        },
                    );
                }
            }
        }
        let ev = snapshot();
        for &entity in &subs {
            push(&sink, entity, OutMsg::List(ev.clone()));
        }
    });
}

fn on_list_request(
    trigger: On<BinReceive<ExtListRequest>>,
    mut subs: ResMut<ExtSubscribers>,
    outbox: Res<ExtOutbox>,
) {
    let entity = trigger.event().webview;
    subs.0.insert(entity);
    push(&outbox, entity, OutMsg::List(snapshot()));
}

fn on_toggle_request(
    trigger: On<BinReceive<ExtToggleRequest>>,
    subs: Res<ExtSubscribers>,
    outbox: Res<ExtOutbox>,
) {
    let req = trigger.event().payload.clone();
    let profile = vmux_core::profile::active_profile_name();
    let _ = store::update_index(&store::root(), |idx| {
        idx.set_enabled_for(&profile, &req.id, req.enabled, req.approve_permissions);
    });
    broadcast_list(&outbox, &subs);
}

fn on_uninstall_request(
    trigger: On<BinReceive<ExtUninstallRequest>>,
    subs: Res<ExtSubscribers>,
    outbox: Res<ExtOutbox>,
) {
    let profile = vmux_core::profile::active_profile_name();
    let _ = store::uninstall_for_profile(&store::root(), &profile, &trigger.event().payload.id);
    broadcast_list(&outbox, &subs);
}

fn on_action_request(
    trigger: On<BinReceive<ExtActionRequest>>,
    mut cmd: MessageWriter<AppCommand>,
) {
    let id = trigger.event().payload.id.clone();
    let idx = store::Index::load(&store::root()).unwrap_or_default();
    let Some(entry) = idx.entries.into_iter().find(|e| e.id == id) else {
        return;
    };
    if !entry.enabled_for(&vmux_core::profile::active_profile_name()) {
        return;
    }
    let Some(popup) = entry.popup else {
        return;
    };
    cmd.write(AppCommand::Browser(BrowserCommand::Open(
        OpenCommand::InNewStack {
            url: Some(format!("chrome-extension://{id}/{popup}")),
        },
    )));
}

fn on_open_manager_request(
    _trigger: On<BinReceive<ExtOpenManagerRequest>>,
    mut cmd: MessageWriter<AppCommand>,
) {
    cmd.write(AppCommand::Browser(BrowserCommand::Open(
        OpenCommand::InNewStack {
            url: Some(EXTENSIONS_PAGE_URL.to_string()),
        },
    )));
}

const WEB_STORE_URL: &str = "https://chromewebstore.google.com/category/extensions";

fn encode_query(q: &str) -> String {
    let mut out = String::with_capacity(q.len());
    for b in q.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn on_browse_store_request(
    trigger: On<BinReceive<ExtBrowseStoreRequest>>,
    mut cmd: MessageWriter<AppCommand>,
) {
    let query = trigger.event().payload.query.trim();
    let url = if query.is_empty() {
        WEB_STORE_URL.to_string()
    } else {
        format!(
            "https://chromewebstore.google.com/search/{}",
            encode_query(query)
        )
    };
    cmd.write(AppCommand::Browser(BrowserCommand::Open(
        OpenCommand::InNewStack { url: Some(url) },
    )));
}

fn run_agent_installs(
    mut reader: MessageReader<vmux_layout::ExtensionInstallRequest>,
    subs: Res<ExtSubscribers>,
    outbox: Res<ExtOutbox>,
) {
    for req in reader.read() {
        spawn_install(
            &outbox,
            subs.0.iter().copied().collect(),
            req.source.clone(),
            None,
        );
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

fn is_webstore_url(url: &str) -> bool {
    url.strip_prefix("https://")
        .and_then(|rest| rest.split(['/', '?', '#']).next())
        .map(|authority| authority == "chromewebstore.google.com")
        .unwrap_or(false)
}

const INJECTOR_JS: &str = include_str!("add_to_vmux.js");

fn inject_on_cws_nav(
    mut events: MessageReader<WebviewCommittedNavigationEvent>,
    browsers: NonSend<Browsers>,
    mut injectors: ResMut<WebStoreInjectors>,
) {
    for ev in events.read() {
        if !ev.is_main_frame {
            continue;
        }
        if is_webstore_url(&ev.url) {
            let Some(extension_id) = vmux_core::extension::webstore::extension_id(&ev.url) else {
                injectors.0.remove(&ev.webview);
                continue;
            };
            let profile = vmux_core::profile::active_profile_name();
            let idx = store::Index::load(&store::root()).unwrap_or_default();
            let installed = idx
                .entries
                .iter()
                .filter(|entry| entry.installed_for(&profile))
                .map(|entry| entry.id.as_str())
                .collect::<Vec<_>>();
            let nonce = uuid::Uuid::new_v4().to_string();
            injectors.0.insert(
                ev.webview,
                WebStoreInjector {
                    nonce: nonce.clone(),
                    extension_id,
                },
            );
            let replacements = [
                (
                    "__VMUX_WEBSTORE_INSTALLED__",
                    serde_json::to_string(&installed).expect("serializable extension list"),
                ),
                (
                    "__VMUX_WEBSTORE_NONCE__",
                    serde_json::to_string(&nonce).expect("serializable web store nonce"),
                ),
            ];
            if let Ok(js) = super::template::render(INJECTOR_JS, &replacements) {
                browsers.execute_js(&ev.webview, &js);
            }
        } else {
            injectors.0.remove(&ev.webview);
        }
    }
}

fn on_add_extension(
    trigger: On<Receive<AddExtensionRequest>>,
    subs: Res<ExtSubscribers>,
    outbox: Res<ExtOutbox>,
    injectors: Res<WebStoreInjectors>,
    mut cmd: MessageWriter<AppCommand>,
) {
    let req = &trigger.payload;
    let Some(injector) = injectors.0.get(&trigger.event().webview) else {
        return;
    };
    let Some(id) = vmux_core::extension::webstore::extension_id(&req.id) else {
        return;
    };
    if injector.nonce != req.nonce || injector.extension_id != id {
        return;
    }
    match req.channel.as_str() {
        ADD_CHANNEL => {
            spawn_install(
                &outbox,
                subs.0.iter().copied().collect(),
                id,
                Some(trigger.event().webview),
            );
        }
        MANAGE_CHANNEL => {
            cmd.write(AppCommand::Browser(BrowserCommand::Open(
                OpenCommand::InNewStack {
                    url: Some(EXTENSIONS_PAGE_URL.to_string()),
                },
            )));
        }
        _ => {}
    }
}

fn drain_outbox(outbox: Res<ExtOutbox>, browsers: NonSend<Browsers>, mut commands: Commands) {
    let drained: Vec<(Entity, OutMsg)> = {
        let mut q = outbox.0.lock().unwrap_or_else(|e| e.into_inner());
        q.drain(..).collect()
    };
    for (entity, msg) in drained {
        if !browsers.has_browser(entity) || !browsers.host_emit_ready(&entity) {
            continue;
        }
        match msg {
            OutMsg::List(ev) => commands.trigger(BinHostEmitEvent::from_rkyv(
                entity,
                EXTENSIONS_LIST_EVENT,
                &ev,
            )),
            OutMsg::Progress(ev) => commands.trigger(BinHostEmitEvent::from_rkyv(
                entity,
                EXT_INSTALL_PROGRESS_EVENT,
                &ev,
            )),
            OutMsg::Status(ev) => {
                commands.trigger(BinHostEmitEvent::from_rkyv(entity, EXT_STATUS_EVENT, &ev))
            }
            OutMsg::WebStoreInstallResult { id, success } => {
                let detail = serde_json::json!({ "id": id, "success": success });
                let script = format!(
                    "globalThis.dispatchEvent(new CustomEvent('__vmuxWebStoreInstallResult',{{detail:{detail}}}));"
                );
                browsers.execute_js(&entity, &script);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn web_store_injector_renders_without_page_globals() {
        let source = super::super::template::render(
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
}
