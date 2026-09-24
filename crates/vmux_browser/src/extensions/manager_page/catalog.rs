use std::sync::{Arc, Mutex};

use bevy::prelude::*;
use bevy_cef::prelude::{BinReceive, Browsers, UiEventPlugin};
use vmux_core::event::{
    ExtInstallPhase, ExtInstallProgress, ExtListRequest, ExtOpenManagerRequest, ExtPinRequest,
    ExtRow, ExtStatus, ExtToggleRequest, ExtUninstallRequest, ExtensionsEvent,
};
use vmux_core::extension::store;
use vmux_layout::LayoutUiStateUpdates;

pub(super) struct CatalogPlugin;

impl Plugin for CatalogPlugin {
    fn build(&self, app: &mut App) {
        app.world_mut().spawn(ExtensionCatalog::default());
        app.init_resource::<ExtOutbox>()
            .add_message::<InstallRequest>()
            .add_plugins(UiEventPlugin::<(
                ExtToggleRequest,
                ExtUninstallRequest,
                ExtListRequest,
                ExtPinRequest,
                ExtOpenManagerRequest,
            )>::default())
            .add_observer(on_list_request)
            .add_observer(on_toggle_request)
            .add_observer(on_uninstall_request)
            .add_observer(on_pin_request)
            .add_observer(on_open_manager_request)
            .add_systems(
                Update,
                (
                    queue_agent_installs,
                    start_installs,
                    drain_outbox,
                    emit_extensions_snapshot,
                )
                    .chain(),
            );
    }
}

#[derive(Message)]
pub(super) struct InstallRequest {
    source: String,
    requester: Option<Entity>,
}

impl InstallRequest {
    pub(super) fn web_store(source: String, requester: Entity) -> Self {
        Self {
            source,
            requester: Some(requester),
        }
    }

    fn agent(source: String) -> Self {
        Self {
            source,
            requester: None,
        }
    }
}

enum OutMsg {
    Progress(ExtInstallProgress),
    List(ExtensionsEvent),
    WebStoreInstallResult {
        entity: Entity,
        id: String,
        success: bool,
    },
}

#[derive(Resource, Clone, Default)]
struct ExtOutbox(Arc<Mutex<Vec<OutMsg>>>);

#[derive(Component, Default)]
struct ExtensionCatalog {
    snapshot: ExtensionsEvent,
    revision: u64,
}

impl ExtensionCatalog {
    fn replace(&mut self, mut snapshot: ExtensionsEvent) {
        snapshot.loaded = true;
        snapshot.installing = std::mem::take(&mut self.snapshot.installing);
        self.snapshot = snapshot;
        self.revision = self.revision.wrapping_add(1);
    }

    fn update_progress(&mut self, progress: ExtInstallProgress) {
        let current = self
            .snapshot
            .installing
            .iter()
            .position(|item| item.key == progress.key);
        if matches!(
            progress.phase,
            ExtInstallPhase::Done | ExtInstallPhase::Failed
        ) {
            if let Some(index) = current {
                self.snapshot.installing.remove(index);
            }
        } else if let Some(index) = current {
            self.snapshot.installing[index] = progress;
        } else {
            self.snapshot.installing.push(progress);
        }
        self.revision = self.revision.wrapping_add(1);
    }
}

#[derive(Component, Default)]
struct ExtensionSubscriber {
    revision: u64,
}

fn push(outbox: &ExtOutbox, msg: OutMsg) {
    outbox.0.lock().unwrap_or_else(|e| e.into_inner()).push(msg);
}

fn snapshot() -> ExtensionsEvent {
    let root = store::root();
    let profile = vmux_core::profile::active_profile_name();
    let index = store::Index::load(&root).unwrap_or_default();
    let loaded = super::super::load::loaded_ids();
    index.snapshot(&profile, &loaded)
}

trait ExtensionIndexSnapshot {
    fn snapshot(&self, profile: &str, loaded: &[String]) -> ExtensionsEvent;
}

impl ExtensionIndexSnapshot for store::Index {
    fn snapshot(&self, profile: &str, loaded: &[String]) -> ExtensionsEvent {
        let mut extensions = Vec::new();
        for entry in &self.entries {
            if !entry.installed_for(profile) {
                continue;
            }
            let enabled = entry.enabled_for(profile);
            extensions.push(ExtRow {
                id: entry.id.clone(),
                name: entry.name.clone(),
                version: entry.version.clone(),
                icon: entry.icon.clone(),
                popup: entry.popup.clone(),
                enabled,
                pinned: entry.pinned_for(profile),
                needs_approval: !entry
                    .grants_for(profile)
                    .covers(&entry.permissions, &entry.host_permissions),
                required_permissions: entry.permissions.clone(),
                required_host_permissions: entry.host_permissions.clone(),
                status: if enabled {
                    ExtStatus::Installed
                } else {
                    ExtStatus::Disabled
                },
            });
        }
        ExtensionsEvent {
            loaded: true,
            extensions,
            installing: Vec::new(),
            pending: self.is_dirty_for(profile, loaded),
        }
    }
}

fn queue_snapshot(outbox: &ExtOutbox) {
    push(outbox, OutMsg::List(snapshot()));
}

fn spawn_install(outbox: &ExtOutbox, request: InstallRequest) {
    let sink = outbox.clone();
    std::thread::spawn(move || {
        let key = request.source.clone();
        let progress_sink = sink.clone();
        let result = super::super::install::install(
            &request.source,
            super::super::install::DEFAULT_PRODVERSION,
            |phase, pct, message| {
                push(
                    &progress_sink,
                    OutMsg::Progress(ExtInstallProgress {
                        key: key.clone(),
                        phase,
                        pct,
                        message: message.to_string(),
                    }),
                );
            },
        );
        match result {
            Ok(entry) => {
                if let Some(entity) = request.requester {
                    push(
                        &sink,
                        OutMsg::WebStoreInstallResult {
                            entity,
                            id: entry.id,
                            success: true,
                        },
                    );
                }
            }
            Err(error) => {
                push(
                    &sink,
                    OutMsg::Progress(ExtInstallProgress {
                        key: key.clone(),
                        phase: ExtInstallPhase::Failed,
                        pct: None,
                        message: error,
                    }),
                );
                if let Some(entity) = request.requester {
                    push(
                        &sink,
                        OutMsg::WebStoreInstallResult {
                            entity,
                            id: key,
                            success: false,
                        },
                    );
                }
            }
        }
        push(&sink, OutMsg::List(snapshot()));
    });
}

fn on_list_request(
    trigger: On<BinReceive<ExtListRequest>>,
    mut catalog: Query<&mut ExtensionCatalog>,
    mut commands: Commands,
) {
    commands
        .entity(trigger.event().webview)
        .insert(ExtensionSubscriber::default());
    let Ok(mut catalog) = catalog.single_mut() else {
        return;
    };
    catalog.replace(snapshot());
}

fn on_toggle_request(trigger: On<BinReceive<ExtToggleRequest>>, outbox: Res<ExtOutbox>) {
    let request = trigger.event().payload.clone();
    let profile = vmux_core::profile::active_profile_name();
    let _ = store::update_index(&store::root(), |index| {
        index.set_enabled_for(
            &profile,
            &request.id,
            request.enabled,
            request.approve_permissions,
        );
    });
    queue_snapshot(&outbox);
}

fn on_uninstall_request(trigger: On<BinReceive<ExtUninstallRequest>>, outbox: Res<ExtOutbox>) {
    let profile = vmux_core::profile::active_profile_name();
    let _ = store::uninstall_for_profile(&store::root(), &profile, &trigger.event().payload.id);
    queue_snapshot(&outbox);
}

fn on_pin_request(trigger: On<BinReceive<ExtPinRequest>>, outbox: Res<ExtOutbox>) {
    let request = trigger.event().payload.clone();
    let outbox = outbox.clone();
    std::thread::spawn(move || {
        let profile = vmux_core::profile::active_profile_name();
        let loaded = super::super::load::loaded_ids();
        let result = store::update_index_if_changed(&store::root(), |index| {
            index
                .set_pinned_for(&profile, &request.id, request.pinned)
                .then(|| index.snapshot(&profile, &loaded))
        });
        match result {
            Ok(Some(snapshot)) => push(&outbox, OutMsg::List(snapshot)),
            Ok(None) => {}
            Err(error) => {
                bevy::log::warn!(
                    extension = request.id,
                    "extension pin update failed: {error}"
                );
                push(
                    &outbox,
                    OutMsg::Progress(ExtInstallProgress {
                        key: request.id,
                        phase: ExtInstallPhase::Failed,
                        pct: None,
                        message: error,
                    }),
                );
            }
        }
    });
}

fn on_open_manager_request(
    _trigger: On<BinReceive<ExtOpenManagerRequest>>,
    mut requests: MessageWriter<vmux_layout::stack::StackRequest>,
) {
    requests.write(vmux_layout::stack::StackRequest::Open {
        url: Some("vmux://tools/extensions".to_string()),
    });
}

fn queue_agent_installs(
    mut incoming: MessageReader<vmux_layout::ExtensionInstallRequest>,
    mut outgoing: MessageWriter<InstallRequest>,
) {
    for request in incoming.read() {
        outgoing.write(InstallRequest::agent(request.source.clone()));
    }
}

fn start_installs(mut requests: MessageReader<InstallRequest>, outbox: Res<ExtOutbox>) {
    for request in requests.read() {
        spawn_install(
            &outbox,
            InstallRequest {
                source: request.source.clone(),
                requester: request.requester,
            },
        );
    }
}

fn drain_outbox(
    outbox: Res<ExtOutbox>,
    browsers: NonSend<Browsers>,
    mut catalog: Query<&mut ExtensionCatalog>,
) {
    let drained: Vec<OutMsg> = {
        let mut queue = outbox.0.lock().unwrap_or_else(|error| error.into_inner());
        queue.drain(..).collect()
    };
    let Ok(mut catalog) = catalog.single_mut() else {
        return;
    };
    for message in drained {
        match message {
            OutMsg::List(snapshot) => catalog.replace(snapshot),
            OutMsg::Progress(progress) => catalog.update_progress(progress),
            OutMsg::WebStoreInstallResult {
                entity,
                id,
                success,
            } => {
                if !browsers.can_emit_to(&entity) {
                    continue;
                }
                let detail = serde_json::json!({ "id": id, "success": success });
                let script = format!(
                    "globalThis.dispatchEvent(new CustomEvent('__vmuxWebStoreInstallResult',{{detail:{detail}}}));"
                );
                browsers.execute_js(&entity, &script);
            }
        }
    }
}

fn emit_extensions_snapshot(
    catalog: Query<&ExtensionCatalog>,
    mut subscribers: Query<(Entity, &mut ExtensionSubscriber)>,
    browsers: NonSend<Browsers>,
    layout_ui: Query<(), With<LayoutUiStateUpdates>>,
    mut commands: Commands,
) {
    let Ok(catalog) = catalog.single() else {
        return;
    };
    for (entity, mut subscriber) in &mut subscribers {
        if subscriber.revision == catalog.revision || !browsers.can_emit_to(&entity) {
            continue;
        }
        LayoutUiStateUpdates::deliver(&layout_ui, &mut commands, entity, &catalog.snapshot);
        subscriber.revision = catalog.revision;
    }
}
