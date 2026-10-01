use bevy::prelude::*;
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use crossbeam_channel::{Receiver, Sender};
use vmux_api::extension::{
    ExtInstallPhase, ExtInstallProgress, ExtOpenManagerRequest, ExtPinRequest, ExtToggleRequest,
    ExtUninstallRequest, ExtensionsEvent,
};
use vmux_ecs::PageMetadata;
use vmux_ecs::host::UiStateWrite;
use vmux_ecs::host::page::NativelyHosted;
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
                ExtPinRequest,
                ExtOpenManagerRequest,
            )>::default())
            .add_observer(page_ready)
            .add_observer(toggle_request)
            .add_observer(uninstall_request)
            .add_observer(pin_request)
            .add_observer(open_manager_request)
            .add_systems(Startup, spawn)
            .add_systems(
                Update,
                (start_installs, drain_outbox, emit_extensions_snapshot).chain(),
            );

        #[cfg(ui)]
        app.add_plugins(
            crate::ui::ExtensionPage::MANIFEST
                .plugin()
                .hosted(NativelyHosted::page(
                    crate::ui::ExtensionPage::URL,
                    crate::ui::ExtensionPage::NATIVE.title,
                )),
        )
        .add_systems(Update, open_manager);
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

#[derive(Component, Clone)]
struct ExtensionOutbox {
    sender: Sender<OutMsg>,
    receiver: Receiver<OutMsg>,
}

impl Default for ExtensionOutbox {
    fn default() -> Self {
        let (sender, receiver) = crossbeam_channel::unbounded();
        Self { sender, receiver }
    }
}

impl ExtensionOutbox {
    fn push(&self, message: OutMsg) {
        let _ = self.sender.send(message);
    }

    fn queue_snapshot(&self) {
        self.push(OutMsg::List(ExtensionCatalog::load()));
    }

    fn install(&self, request: ExtensionInstallRequest) {
        let sink = self.clone();
        std::thread::spawn(move || {
            let key = request.source.clone();
            let progress_sink = sink.clone();
            let result = install::install(
                &request.source,
                install::DEFAULT_PRODVERSION,
                |phase, pct, message| {
                    progress_sink.push(OutMsg::Progress(ExtInstallProgress {
                        key: key.clone(),
                        phase,
                        pct,
                        message: message.to_string(),
                    }));
                },
            );
            match result {
                Ok(entry) => {
                    if let Some(entity) = request.requester {
                        sink.push(OutMsg::InstallCompleted {
                            entity,
                            id: entry.id,
                            success: true,
                        });
                    }
                }
                Err(error) => {
                    sink.push(OutMsg::Progress(ExtInstallProgress {
                        key: key.clone(),
                        phase: ExtInstallPhase::Failed,
                        pct: None,
                        message: error,
                    }));
                    if let Some(entity) = request.requester {
                        sink.push(OutMsg::InstallCompleted {
                            entity,
                            id: key,
                            success: false,
                        });
                    }
                }
            }
            sink.queue_snapshot();
        });
    }
}

#[derive(Component, Default)]
struct ExtensionCatalog {
    snapshot: ExtensionsEvent,
    revision: u64,
}

impl ExtensionCatalog {
    fn load() -> ExtensionsEvent {
        let store = store::ExtensionStore::current();
        let profile = vmux_ecs::profile::Profile::current().into_id();
        let index = store.load_index().unwrap_or_default();
        let loaded = store.loaded_ids(&profile);
        index.snapshot(&profile, &loaded)
    }

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

fn spawn(mut commands: Commands) {
    let outbox = ExtensionOutbox::default();
    let loader = outbox.clone();
    std::thread::spawn(move || loader.queue_snapshot());
    commands.spawn((ExtensionCatalog::default(), outbox));
}

fn page_ready(
    trigger: On<UiInput<vmux_api::PageReady>>,
    pages: Query<(Has<vmux_layout::LayoutCef>, Option<&PageMetadata>)>,
    extension_pages: Query<(&NativelyHosted, &vmux_ecs::page::PageManifest)>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let Ok((layout, metadata)) = pages.get(webview) else {
        return;
    };
    let extension = metadata.is_some_and(|metadata| {
        extension_pages.iter().any(|(page, manifest)| {
            manifest.url == crate::ExtensionPlugin::MANIFEST.url && page.answers_for(&metadata.url)
        })
    });
    if layout || extension {
        commands
            .entity(webview)
            .insert(ExtensionSubscriber::default());
    }
}

fn toggle_request(trigger: On<UiInput<ExtToggleRequest>>, runtime: Single<&ExtensionOutbox>) {
    let request = trigger.event().payload.clone();
    let profile = vmux_ecs::profile::Profile::current().into_id();
    let _ = store::ExtensionStore::current().update_index(|index| {
        index.set_enabled_for(
            &profile,
            &request.id,
            request.enabled,
            request.approve_permissions,
        );
    });
    runtime.queue_snapshot();
}

fn uninstall_request(trigger: On<UiInput<ExtUninstallRequest>>, runtime: Single<&ExtensionOutbox>) {
    let profile = vmux_ecs::profile::Profile::current().into_id();
    let _ = store::ExtensionStore::current()
        .uninstall_for_profile(&profile, &trigger.event().payload.id);
    runtime.queue_snapshot();
}

fn pin_request(trigger: On<UiInput<ExtPinRequest>>, runtime: Single<&ExtensionOutbox>) {
    let request = trigger.event().payload.clone();
    let outbox = runtime.clone();
    std::thread::spawn(move || {
        let store = store::ExtensionStore::current();
        let profile = vmux_ecs::profile::Profile::current().into_id();
        let loaded = store.loaded_ids(&profile);
        let result = store.update_index_if_changed(|index| {
            index
                .set_pinned_for(&profile, &request.id, request.pinned)
                .then(|| index.snapshot(&profile, &loaded))
        });
        match result {
            Ok(Some(snapshot)) => outbox.push(OutMsg::List(snapshot)),
            Ok(None) => {}
            Err(error) => {
                bevy::log::warn!(
                    extension = request.id,
                    "extension pin update failed: {error}"
                );
                outbox.push(OutMsg::Progress(ExtInstallProgress {
                    key: request.id,
                    phase: ExtInstallPhase::Failed,
                    pct: None,
                    message: error,
                }));
            }
        }
    });
}

fn open_manager_request(
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
        runtime.install(ExtensionInstallRequest {
            source: request.source.clone(),
            requester: request.requester,
        });
    }
}

fn drain_outbox(
    runtime: Single<&ExtensionOutbox>,
    mut catalog: Query<&mut ExtensionCatalog>,
    mut completed: MessageWriter<ExtensionInstallCompleted>,
) {
    let Ok(mut catalog) = catalog.single_mut() else {
        return;
    };
    for message in runtime.receiver.try_iter() {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_ready_subscribes_only_extension_and_layout_pages() {
        let mut app = App::new();
        app.add_observer(page_ready);
        app.world_mut().spawn((
            crate::ExtensionPlugin::MANIFEST,
            NativelyHosted::page("vmux://extensions/", "Extensions"),
        ));
        let extension = app
            .world_mut()
            .spawn(PageMetadata {
                url: "vmux://extensions/".to_string(),
                ..default()
            })
            .id();
        let layout = app.world_mut().spawn(vmux_layout::LayoutCef).id();
        let unrelated = app
            .world_mut()
            .spawn(PageMetadata {
                url: "vmux://settings/".to_string(),
                ..default()
            })
            .id();

        for webview in [extension, layout, unrelated] {
            app.world_mut().trigger(UiInput {
                webview,
                payload: vmux_api::PageReady {},
            });
        }
        app.update();

        assert!(app.world().get::<ExtensionSubscriber>(extension).is_some());
        assert!(app.world().get::<ExtensionSubscriber>(layout).is_some());
        assert!(app.world().get::<ExtensionSubscriber>(unrelated).is_none());
    }
}
