use std::sync::{Arc, Mutex};

use bevy::prelude::*;
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use vmux_api::extension::{
    ExtInstallPhase, ExtInstallProgress, ExtListRequest, ExtOpenManagerRequest, ExtPinRequest,
    ExtToggleRequest, ExtUninstallRequest, ExtensionsEvent,
};
use vmux_core::host::UiStateWrite;
use vmux_layout::LayoutUiStateUpdates;

use crate::{install, store};

pub(crate) struct ExtensionCatalogPlugin;

impl Plugin for ExtensionCatalogPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ExtensionInstallRequest>()
            .add_message::<ExtensionInstallCompleted>()
            .add_message::<OpenManagerRequest>()
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
            .add_systems(Startup, spawn_extension_catalog)
            .add_systems(
                Update,
                (start_installs, drain_outbox, emit_extensions_snapshot).chain(),
            );

        #[cfg(ui)]
        app.add_systems(Update, open_manager);
    }
}

#[derive(Message, Clone, Debug)]
pub struct ExtensionInstallRequest {
    pub source: String,
    pub requester: Option<Entity>,
}

#[derive(Message, Clone, Debug)]
pub struct ExtensionInstallCompleted {
    pub requester: Entity,
    pub id: String,
    pub success: bool,
}

#[derive(Message, Clone, Copy, Debug, Default)]
pub struct OpenManagerRequest;

enum OutMsg {
    Progress(ExtInstallProgress),
    List(ExtensionsEvent),
    InstallCompleted {
        entity: Entity,
        id: String,
        success: bool,
    },
}

#[derive(Component, Clone, Default)]
struct ExtensionOutbox(Arc<Mutex<Vec<OutMsg>>>);

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

fn spawn_extension_catalog(mut commands: Commands) {
    commands.spawn((ExtensionCatalog::default(), ExtensionOutbox::default()));
}

fn push(outbox: &ExtensionOutbox, msg: OutMsg) {
    outbox.0.lock().unwrap_or_else(|e| e.into_inner()).push(msg);
}

fn snapshot() -> ExtensionsEvent {
    let root = store::root();
    let profile = vmux_core::profile::active_profile_name();
    let index = store::Index::load(&root).unwrap_or_default();
    let loaded = store::loaded_ids();
    index.snapshot(&profile, &loaded)
}

fn queue_snapshot(outbox: &ExtensionOutbox) {
    push(outbox, OutMsg::List(snapshot()));
}

fn spawn_install(outbox: &ExtensionOutbox, request: ExtensionInstallRequest) {
    let sink = outbox.clone();
    std::thread::spawn(move || {
        let key = request.source.clone();
        let progress_sink = sink.clone();
        let result = install::install(
            &request.source,
            install::DEFAULT_PRODVERSION,
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
                        OutMsg::InstallCompleted {
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
                        OutMsg::InstallCompleted {
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
    trigger: On<UiInput<ExtListRequest>>,
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

fn on_toggle_request(trigger: On<UiInput<ExtToggleRequest>>, runtime: Single<&ExtensionOutbox>) {
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
    queue_snapshot(&runtime);
}

fn on_uninstall_request(
    trigger: On<UiInput<ExtUninstallRequest>>,
    runtime: Single<&ExtensionOutbox>,
) {
    let profile = vmux_core::profile::active_profile_name();
    let _ = store::uninstall_for_profile(&store::root(), &profile, &trigger.event().payload.id);
    queue_snapshot(&runtime);
}

fn on_pin_request(trigger: On<UiInput<ExtPinRequest>>, runtime: Single<&ExtensionOutbox>) {
    let request = trigger.event().payload.clone();
    let outbox = runtime.clone();
    std::thread::spawn(move || {
        let profile = vmux_core::profile::active_profile_name();
        let loaded = store::loaded_ids();
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
    _trigger: On<UiInput<ExtOpenManagerRequest>>,
    mut requests: MessageWriter<OpenManagerRequest>,
) {
    requests.write(OpenManagerRequest);
}

#[cfg(ui)]
fn open_manager(
    mut requests: MessageReader<OpenManagerRequest>,
    mut pages: MessageWriter<vmux_layout::stack::OpenRequest>,
) {
    for _ in requests.read() {
        pages.write(vmux_layout::stack::OpenRequest {
            url: Some(crate::ui::ExtensionPage::URL.to_string()),
        });
    }
}

fn start_installs(
    mut requests: MessageReader<ExtensionInstallRequest>,
    runtime: Single<&ExtensionOutbox>,
) {
    for request in requests.read() {
        spawn_install(
            &runtime,
            ExtensionInstallRequest {
                source: request.source.clone(),
                requester: request.requester,
            },
        );
    }
}

fn drain_outbox(
    runtime: Single<&ExtensionOutbox>,
    mut catalog: Query<&mut ExtensionCatalog>,
    mut completed: MessageWriter<ExtensionInstallCompleted>,
) {
    let drained: Vec<OutMsg> = {
        let mut queue = runtime.0.lock().unwrap_or_else(|error| error.into_inner());
        queue.drain(..).collect()
    };
    let Ok(mut catalog) = catalog.single_mut() else {
        return;
    };
    for message in drained {
        match message {
            OutMsg::List(snapshot) => catalog.replace(snapshot),
            OutMsg::Progress(progress) => catalog.update_progress(progress),
            OutMsg::InstallCompleted {
                entity,
                id,
                success,
            } => {
                completed.write(ExtensionInstallCompleted {
                    requester: entity,
                    id,
                    success,
                });
            }
        }
    }
}

fn emit_extensions_snapshot(
    catalog: Query<&ExtensionCatalog>,
    mut subscribers: Query<(Entity, &mut ExtensionSubscriber)>,
    layout_ui: Query<(), With<LayoutUiStateUpdates>>,
    mut commands: Commands,
) {
    let Ok(catalog) = catalog.single() else {
        return;
    };
    for (entity, mut subscriber) in &mut subscribers {
        if subscriber.revision == catalog.revision {
            continue;
        }
        commands.trigger(UiStateWrite::<ExtensionsEvent>::from_event(
            entity,
            &catalog.snapshot,
        ));
        if layout_ui.contains(entity) {
            commands.trigger(
                UiStateWrite::<vmux_layout::state::LayoutUiState>::from_event(
                    entity,
                    &catalog.snapshot,
                ),
            );
        }
        subscriber.revision = catalog.revision;
    }
}
