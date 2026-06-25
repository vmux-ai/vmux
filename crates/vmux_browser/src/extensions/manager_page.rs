use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use bevy::prelude::*;
use bevy_cef::prelude::{
    BinEventEmitterPlugin, BinHostEmitEvent, BinReceive, Browsers, JsEmitEventPlugin, Receive,
    WebviewCommittedNavigationEvent,
};
use vmux_command::{AppCommand, BrowserCommand, open::OpenCommand};
use vmux_core::event::extension::{
    EXT_INSTALL_PROGRESS_EVENT, EXT_STATUS_EVENT, EXTENSIONS_LIST_EVENT, EXTENSIONS_PAGE_URL,
    ExtActionRequest, ExtInstallPhase, ExtInstallProgress, ExtListRequest, ExtOpenManagerRequest,
    ExtRow, ExtStatus, ExtStatusEvent, ExtToggleRequest, ExtUninstallRequest, ExtensionsEvent,
};
use vmux_core::extension::store;
use vmux_core::{CefPageAttachRequest, PageOpenError, PageOpenHandled, PageOpenSet, PageOpenTask};

const PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: "extensions",
    title: "Extensions",
    keywords: &["extension", "extensions", "chrome", "addon", "install"],
    icon: "puzzle",
    command_bar: true,
};

enum OutMsg {
    Progress(ExtInstallProgress),
    Status(ExtStatusEvent),
    List(ExtensionsEvent),
}

#[derive(Resource, Clone, Default)]
struct ExtOutbox(Arc<Mutex<Vec<(Entity, OutMsg)>>>);

#[derive(Resource, Default)]
struct ExtSubscribers(HashSet<Entity>);

pub struct ExtensionsPlugin;

impl Plugin for ExtensionsPlugin {
    fn build(&self, app: &mut App) {
        app.world_mut().spawn(PAGE_MANIFEST);
        app.init_resource::<ExtOutbox>()
            .init_resource::<ExtSubscribers>()
            .add_message::<CefPageAttachRequest>()
            .add_plugins(BinEventEmitterPlugin::<(
                ExtListRequest,
                ExtToggleRequest,
                ExtUninstallRequest,
            )>::default())
            .add_plugins(BinEventEmitterPlugin::<(
                ExtActionRequest,
                ExtOpenManagerRequest,
            )>::default())
            .add_plugins(JsEmitEventPlugin::<AddExtensionRequest>::default())
            .add_observer(on_list_request)
            .add_observer(on_toggle_request)
            .add_observer(on_uninstall_request)
            .add_observer(on_action_request)
            .add_observer(on_open_manager_request)
            .add_observer(on_add_extension)
            .add_systems(
                Update,
                handle_extensions_page_open.in_set(PageOpenSet::HandleKnownPages),
            )
            .add_systems(
                Update,
                (run_agent_installs, inject_on_cws_nav, drain_outbox),
            );
    }
}

type PendingPageOpen = (Without<PageOpenHandled>, Without<PageOpenError>);

fn handle_extensions_page_open(
    tasks: Query<(Entity, &PageOpenTask), PendingPageOpen>,
    mut attach_writer: MessageWriter<CefPageAttachRequest>,
    mut commands: Commands,
) {
    for (entity, task) in &tasks {
        if task.url != EXTENSIONS_PAGE_URL {
            continue;
        }
        attach_writer.write(CefPageAttachRequest {
            stack: task.stack,
            url: task.url.clone(),
            title: "Extensions".to_string(),
            bg_color: None,
        });
        commands.entity(entity).insert(PageOpenHandled);
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
    let idx = store::Index::load(&root).unwrap_or_default();
    let loaded = super::load::loaded_ids();
    let extensions = idx
        .entries
        .iter()
        .map(|e| ExtRow {
            id: e.id.clone(),
            name: e.name.clone(),
            version: e.version.clone(),
            icon: e.icon.clone(),
            popup: e.popup.clone(),
            enabled: e.enabled,
            status: if e.enabled {
                ExtStatus::Installed
            } else {
                ExtStatus::Disabled
            },
        })
        .collect();
    ExtensionsEvent {
        extensions,
        pending: idx.is_dirty(&loaded),
    }
}

fn broadcast_list(outbox: &ExtOutbox, subs: &ExtSubscribers) {
    let ev = snapshot();
    for &entity in &subs.0 {
        push(outbox, entity, OutMsg::List(ev.clone()));
    }
}

fn spawn_install(outbox: &ExtOutbox, subs: Vec<Entity>, source: String) {
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
                            status: ExtStatus::Installed,
                            version: Some(entry.version.clone()),
                        }),
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
    let _ = store::update_index(&store::root(), |idx| idx.set_enabled(&req.id, req.enabled));
    broadcast_list(&outbox, &subs);
}

fn on_uninstall_request(
    trigger: On<BinReceive<ExtUninstallRequest>>,
    subs: Res<ExtSubscribers>,
    outbox: Res<ExtOutbox>,
) {
    let _ = store::uninstall(&store::root(), &trigger.event().payload.id);
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
        );
    }
}

#[derive(serde::Deserialize)]
struct AddExtensionRequest {
    channel: String,
    id: String,
}

const ADD_CHANNEL: &str = "vmux-add-extension";

fn is_webstore_url(url: &str) -> bool {
    url.strip_prefix("https://")
        .and_then(|rest| rest.split(['/', '?', '#']).next())
        .map(|authority| authority == "chromewebstore.google.com")
        .unwrap_or(false)
}

const INJECTOR_JS: &str = r#"(function(){
  if (window.__VMUX_EXT__) return;
  window.__VMUX_EXT__ = true;
  function extId(){
    var s = location.pathname.split('/');
    for (var i=0;i<s.length;i++){ if (/^[a-p]{32}$/.test(s[i])) return s[i]; }
    return null;
  }
  function findAddBtn(){
    var els = document.querySelectorAll('button, a, [role=button]');
    for (var i=0;i<els.length;i++){
      var el = els[i];
      if (el.id === 'vmux-add-ext' || el.offsetParent === null) continue;
      var t = (el.textContent||'').trim();
      if (t === 'Add to Chrome' || t === 'Remove from Chrome') return el;
    }
    return null;
  }
  function place(){
    var id = extId();
    var existing = document.getElementById('vmux-add-ext');
    if (!id){ if (existing) existing.remove(); return; }
    if (existing && existing.dataset.extId === id) return;
    var anchor = findAddBtn();
    if (!anchor || !anchor.parentNode) return;
    if (existing) existing.remove();
    var b = anchor.cloneNode(true);
    b.id = 'vmux-add-ext';
    b.dataset.extId = id;
    b.removeAttribute('disabled');
    b.removeAttribute('href');
    b.textContent = 'Add to Vmux';
    b.addEventListener('click', function(ev){
      ev.preventDefault(); ev.stopPropagation();
      try { cef.emit({ channel: 'vmux-add-extension', id: id }); } catch(e){}
      b.textContent = 'Added — relaunch';
    });
    anchor.style.display = 'none';
    anchor.parentNode.insertBefore(b, anchor);
  }
  new MutationObserver(place).observe(document.documentElement, {childList:true, subtree:true});
  place();
})();"#;

fn inject_on_cws_nav(
    mut events: MessageReader<WebviewCommittedNavigationEvent>,
    browsers: NonSend<Browsers>,
) {
    for ev in events.read() {
        if ev.is_main_frame && is_webstore_url(&ev.url) {
            browsers.execute_js(&ev.webview, INJECTOR_JS);
        }
    }
}

fn on_add_extension(
    trigger: On<Receive<AddExtensionRequest>>,
    mut writer: MessageWriter<vmux_layout::ExtensionInstallRequest>,
) {
    let req = &trigger.payload;
    if req.channel != ADD_CHANNEL || req.id.trim().is_empty() {
        return;
    }
    writer.write(vmux_layout::ExtensionInstallRequest {
        source: req.id.clone(),
    });
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
        }
    }
}
