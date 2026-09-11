use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use bevy::prelude::*;
use bevy_cef::prelude::{
    BinEventEmitterPlugin, BinHostEmitEvent, BinReceive, Browsers, HostWindow, JsEmitEventPlugin,
    Receive, WebviewCommittedNavigationEvent,
};
use vmux_command::{AppCommand, BrowserCommand, open::OpenCommand};
use vmux_core::KeyboardOwner;
use vmux_core::event::{
    EXT_INSTALL_PROGRESS_EVENT, EXT_STATUS_EVENT, EXTENSION_POPUP_EVENT, EXTENSIONS_LIST_EVENT,
    EXTENSIONS_PAGE_URL, ExtActionRequest, ExtBrowseStoreRequest, ExtInstallPhase,
    ExtInstallProgress, ExtListRequest, ExtOpenManagerRequest, ExtPinRequest, ExtRow, ExtStatus,
    ExtStatusEvent, ExtToggleRequest, ExtUninstallRequest, ExtensionPopupBoundsRequest,
    ExtensionPopupCloseRequest, ExtensionPopupEvent, ExtensionsEvent,
};
use vmux_core::extension::store;
use vmux_core::overlay::WindowOverlay;
use vmux_flex::prelude::Visibility;
use vmux_layout::{Browser, LayoutCef};

#[derive(Component, Default)]
pub struct Extensions;

impl vmux_layout::native_open::HostedPage for Extensions {
    const HOST: &'static str = "extensions";
    const URL: &'static str = EXTENSIONS_PAGE_URL;
    const TITLE: &'static str = "Extensions";
}

pub struct ExtensionsPlugin;

impl Plugin for ExtensionsPlugin {
    fn build(&self, app: &mut App) {
        app.world_mut().spawn(PAGE_MANIFEST);
        app.add_plugins(vmux_layout::native_open::HostedPagePlugin::<Extensions>::default());
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
                ExtPinRequest,
                ExtOpenManagerRequest,
                ExtensionPopupBoundsRequest,
                ExtensionPopupCloseRequest,
            )>::for_hosts(&["extensions", "layout"]))
            .add_plugins(JsEmitEventPlugin::<AddExtensionRequest>::default())
            .add_observer(on_list_request)
            .add_observer(on_toggle_request)
            .add_observer(on_uninstall_request)
            .add_observer(on_action_request)
            .add_observer(on_popup_bounds_request)
            .add_observer(on_popup_close_request)
            .add_observer(on_pin_request)
            .add_observer(on_open_manager_request)
            .add_observer(on_browse_store_request)
            .add_observer(on_add_extension)
            .add_systems(
                Update,
                (
                    run_agent_installs,
                    inject_on_cws_nav,
                    inject_on_cws_load_complete.after(crate::page_life::drain_loading_state),
                    drain_outbox,
                ),
            );
    }
}

#[derive(Component)]
pub(crate) struct ExtensionPopup {
    owner: Entity,
}

#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub(crate) struct ExtensionPopupBounds {
    pub(crate) left: f32,
    pub(crate) top: f32,
    pub(crate) width: f32,
    pub(crate) height: f32,
}

impl ExtensionPopupBounds {
    fn of(request: &ExtensionPopupBoundsRequest) -> Option<Self> {
        if !request.left.is_finite()
            || !request.top.is_finite()
            || !request.width.is_finite()
            || !request.height.is_finite()
            || request.width <= 0.0
            || request.height <= 0.0
        {
            return None;
        }
        Some(Self {
            left: request.left,
            top: request.top,
            width: request.width,
            height: request.height,
        })
    }
}

#[derive(Component)]
pub(crate) struct ExtensionPopupPresented;

const PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: "extensions",
    title: "Extensions",
    title_message_id: Some("extensions-title"),
    replaces_command: None,
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

#[derive(Clone)]
struct WebStoreInjector {
    nonce: String,
    extension_id: String,
}

#[derive(Resource, Default)]
struct WebStoreInjectors(HashMap<Entity, WebStoreInjector>);

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
    snapshot_from_index(&idx, &profile, &loaded)
}

fn snapshot_from_index(idx: &store::Index, profile: &str, loaded: &[String]) -> ExtensionsEvent {
    let extensions = idx
        .entries
        .iter()
        .filter(|entry| entry.installed_for(profile))
        .map(|e| {
            let enabled = e.enabled_for(profile);
            let needs_approval = !e
                .grants_for(profile)
                .covers(&e.permissions, &e.host_permissions);
            ExtRow {
                id: e.id.clone(),
                name: e.name.clone(),
                version: e.version.clone(),
                icon: e.icon.clone(),
                popup: e.popup.clone(),
                enabled,
                pinned: e.pinned_for(profile),
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
        pending: idx.is_dirty_for(profile, loaded),
    }
}

fn broadcast_list(outbox: &ExtOutbox, subs: &ExtSubscribers) {
    broadcast_snapshot(outbox, subs, snapshot());
}

fn broadcast_snapshot(outbox: &ExtOutbox, subs: &ExtSubscribers, event: ExtensionsEvent) {
    for &entity in &subs.0 {
        push(outbox, entity, OutMsg::List(event.clone()));
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
    layouts: Query<(Entity, Option<&HostWindow>), With<LayoutCef>>,
    host_windows: Query<&HostWindow>,
    popups: Query<(Entity, &ExtensionPopup)>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
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
    let url = format!("chrome-extension://{id}/{popup}");
    let source = trigger.event().webview;
    let source_window = host_windows.get(source).ok().map(|host| host.0);
    let owner = layouts.iter().find_map(|(layout, host)| {
        (layout == source || source_window.is_none_or(|window| host.is_some_and(|h| h.0 == window)))
            .then_some(layout)
    });
    let Some(owner) = owner else {
        return;
    };
    close_popup(owner, &popups, &browsers, &mut commands);
    commands
        .spawn(Browser::new_with_title(&url, &entry.name))
        .insert((
            Name::new(format!("Extension popup: {}", entry.name)),
            WindowOverlay,
            ExtensionPopup { owner },
            Visibility::Hidden,
        ));
    commands.trigger(BinHostEmitEvent::from_rkyv(
        owner,
        EXTENSION_POPUP_EVENT,
        &ExtensionPopupEvent {
            id,
            name: entry.name,
            icon: entry.icon,
            anchor: trigger.event().payload.anchor,
        },
    ));
}

fn on_popup_bounds_request(
    trigger: On<BinReceive<ExtensionPopupBoundsRequest>>,
    popups: Query<(Entity, &ExtensionPopup)>,
    mut commands: Commands,
) {
    let Some(bounds) = ExtensionPopupBounds::of(&trigger.event().payload) else {
        return;
    };
    for (entity, popup) in &popups {
        if popup.owner == trigger.event().webview {
            commands
                .entity(entity)
                .insert((bounds, Visibility::Visible, KeyboardOwner));
        }
    }
}

fn on_popup_close_request(
    trigger: On<BinReceive<ExtensionPopupCloseRequest>>,
    popups: Query<(Entity, &ExtensionPopup)>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    close_popup(trigger.event().webview, &popups, &browsers, &mut commands);
}

fn close_popup(
    owner: Entity,
    popups: &Query<(Entity, &ExtensionPopup)>,
    browsers: &Browsers,
    commands: &mut Commands,
) {
    for (entity, popup) in popups {
        if popup.owner != owner {
            continue;
        }
        browsers.hide_child_window(&entity);
        commands.entity(entity).try_despawn();
    }
}

fn on_pin_request(
    trigger: On<BinReceive<ExtPinRequest>>,
    subs: Res<ExtSubscribers>,
    outbox: Res<ExtOutbox>,
) {
    let request = trigger.event().payload.clone();
    let recipients = subs.0.iter().copied().collect::<Vec<_>>();
    let outbox = outbox.clone();
    std::thread::spawn(move || {
        let profile = vmux_core::profile::active_profile_name();
        let loaded = super::load::loaded_ids();
        let result = store::update_index_if_changed(&store::root(), |index| {
            index
                .set_pinned_for(&profile, &request.id, request.pinned)
                .then(|| snapshot_from_index(index, &profile, &loaded))
        });
        match result {
            Ok(Some(snapshot)) => {
                for entity in recipients {
                    push(&outbox, entity, OutMsg::List(snapshot.clone()));
                }
            }
            Ok(None) => {}
            Err(error) => {
                bevy::log::warn!(
                    extension = request.id,
                    "extension pin update failed: {error}"
                );
                for entity in recipients {
                    push(
                        &outbox,
                        entity,
                        OutMsg::Progress(ExtInstallProgress {
                            key: request.id.clone(),
                            phase: ExtInstallPhase::Failed,
                            pct: None,
                            message: error.clone(),
                        }),
                    );
                }
            }
        }
    });
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

fn webstore_injector(current: Option<&WebStoreInjector>, extension_id: String) -> WebStoreInjector {
    if let Some(current) = current
        && current.extension_id == extension_id
    {
        return current.clone();
    }
    WebStoreInjector {
        nonce: uuid::Uuid::new_v4().to_string(),
        extension_id,
    }
}

fn inject_webstore_page(
    webview: Entity,
    url: &str,
    browsers: &Browsers,
    injectors: &mut WebStoreInjectors,
) {
    if !is_webstore_url(url) {
        injectors.0.remove(&webview);
        return;
    }
    let Some(extension_id) = vmux_core::extension::webstore::extension_id(url) else {
        injectors.0.remove(&webview);
        return;
    };
    let injector = webstore_injector(injectors.0.get(&webview), extension_id);
    let profile = vmux_core::profile::active_profile_name();
    let idx = store::Index::load(&store::root()).unwrap_or_default();
    let installed = idx
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
    injectors.0.insert(webview, injector);
    if let Ok(js) = super::template::render(INJECTOR_JS, &replacements) {
        browsers.execute_js(&webview, &js);
    }
}

fn inject_on_cws_nav(
    mut events: MessageReader<WebviewCommittedNavigationEvent>,
    browsers: NonSend<Browsers>,
    mut injectors: ResMut<WebStoreInjectors>,
) {
    for ev in events.read() {
        if !ev.is_main_frame {
            continue;
        }
        inject_webstore_page(ev.webview, &ev.url, &browsers, &mut injectors);
    }
}

fn inject_on_cws_load_complete(
    mut events: MessageReader<crate::WebviewLoadCompleted>,
    browsers: NonSend<Browsers>,
    browser_meta: Query<&vmux_core::PageMetadata, With<vmux_layout::Browser>>,
    mut injectors: ResMut<WebStoreInjectors>,
) {
    for event in events.read() {
        let Ok(meta) = browser_meta.get(event.webview) else {
            continue;
        };
        inject_webstore_page(event.webview, &meta.url, &browsers, &mut injectors);
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
        if !browsers.can_emit_to(&entity) {
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

    #[test]
    fn web_store_injector_reuses_nonce_for_same_extension() {
        let first = webstore_injector(None, "a".repeat(32));
        let same = webstore_injector(Some(&first), "a".repeat(32));
        let different = webstore_injector(Some(&first), "b".repeat(32));

        assert_eq!(same.nonce, first.nonce);
        assert_ne!(different.nonce, first.nonce);
    }
}
