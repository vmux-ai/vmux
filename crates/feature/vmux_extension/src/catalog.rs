use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use bevy::winit::EventLoopProxyWrapper;
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use vmux_api::extension::{
    ExtInstallPhase, ExtInstallProgress, ExtOpenManagerRequest, ExtPinRequest, ExtToggleRequest,
    ExtUninstallRequest, ExtensionsEvent,
};
use vmux_ecs::PageMetadata;
use vmux_ecs::host::UiStateWrite;
use vmux_ecs::host::page::NativelyHosted;
use vmux_layout::LayoutUiStateUpdates;

use crate::store;

pub(crate) struct ExtensionCatalogPlugin;

impl Plugin for ExtensionCatalogPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<OpenManagerRequest>()
            .add_plugins((
                crate::install::InstallPlugin,
                UiEventPlugin::<(
                    ExtToggleRequest,
                    ExtUninstallRequest,
                    ExtPinRequest,
                    ExtOpenManagerRequest,
                )>::default(),
            ))
            .add_observer(page_ready)
            .add_observer(toggle_request)
            .add_observer(uninstall_request)
            .add_observer(pin_request)
            .add_observer(open_manager_request)
            .add_systems(Startup, spawn)
            .add_systems(Update, (finish_tasks, emit_extensions_snapshot).chain());

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

#[derive(Message, Clone, Copy, Debug, Default)]
pub struct OpenManagerRequest;

#[derive(Component, Default)]
pub(crate) struct ExtensionCatalog {
    snapshot: ExtensionsEvent,
    revision: u64,
}

impl ExtensionCatalog {
    pub(crate) fn replace(&mut self, mut snapshot: ExtensionsEvent) {
        snapshot.loaded = true;
        snapshot.installing = std::mem::take(&mut self.snapshot.installing);
        self.snapshot = snapshot;
        self.revision = self.revision.wrapping_add(1);
    }

    pub(crate) fn update_progress(&mut self, progress: ExtInstallProgress) {
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

#[derive(Component)]
struct ExtensionCatalogTask(Task<Result<Option<ExtensionsEvent>, String>>);

#[derive(Component)]
struct ExtensionCatalogKey(String);

#[derive(Component)]
struct InitialCatalogLoad;

fn spawn(proxy: Option<Res<EventLoopProxyWrapper>>, mut commands: Commands) {
    let wake = vmux_ecs::host::wake::Wake::beside(proxy.as_deref());
    let task = IoTaskPool::get().spawn(async move {
        let store = store::ExtensionStore::current();
        let profile = vmux_ecs::profile::Profile::current().into_id();
        let result = store.snapshot(&profile).map(Some);
        drop(wake);
        result
    });
    commands.spawn((Name::new("Extension catalog"), ExtensionCatalog::default()));
    commands.spawn((
        Name::new("Load extensions"),
        ExtensionCatalogTask(task),
        InitialCatalogLoad,
    ));
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

fn toggle_request(
    trigger: On<UiInput<ExtToggleRequest>>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let request = trigger.event().payload.clone();
    let wake = vmux_ecs::host::wake::Wake::beside(proxy.as_deref());
    let task = IoTaskPool::get().spawn(async move {
        let store = store::ExtensionStore::current();
        let profile = vmux_ecs::profile::Profile::current().into_id();
        let result = store.update_index(|index| {
            index.set_enabled_for(
                &profile,
                &request.id,
                request.enabled,
                request.approve_permissions,
            );
        });
        let result = result.and_then(|()| store.snapshot(&profile)).map(Some);
        drop(wake);
        result
    });
    commands.spawn((Name::new("Toggle extension"), ExtensionCatalogTask(task)));
}

fn uninstall_request(
    trigger: On<UiInput<ExtUninstallRequest>>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let request = trigger.event().payload.clone();
    let wake = vmux_ecs::host::wake::Wake::beside(proxy.as_deref());
    let task = IoTaskPool::get().spawn(async move {
        let store = store::ExtensionStore::current();
        let profile = vmux_ecs::profile::Profile::current().into_id();
        let result = store
            .uninstall_for_profile(&profile, &request.id)
            .and_then(|()| store.snapshot(&profile))
            .map(Some);
        drop(wake);
        result
    });
    commands.spawn((Name::new("Uninstall extension"), ExtensionCatalogTask(task)));
}

fn pin_request(
    trigger: On<UiInput<ExtPinRequest>>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let request = trigger.event().payload.clone();
    let key = request.id.clone();
    let wake = vmux_ecs::host::wake::Wake::beside(proxy.as_deref());
    let task = IoTaskPool::get().spawn(async move {
        let store = store::ExtensionStore::current();
        let profile = vmux_ecs::profile::Profile::current().into_id();
        let loaded = store.loaded_ids(&profile);
        let result = store.update_index_if_changed(|index| {
            index
                .set_pinned_for(&profile, &request.id, request.pinned)
                .then(|| index.snapshot(&profile, &loaded))
        });
        drop(wake);
        result
    });
    commands.spawn((
        Name::new("Pin extension"),
        ExtensionCatalogTask(task),
        ExtensionCatalogKey(key),
    ));
}

fn finish_tasks(
    mut tasks: Query<(
        Entity,
        &Name,
        &mut ExtensionCatalogTask,
        Option<&ExtensionCatalogKey>,
        Has<InitialCatalogLoad>,
    )>,
    mut catalog: Single<&mut ExtensionCatalog>,
    mut commands: Commands,
) {
    for (entity, name, mut task, key, initial) in &mut tasks {
        let Some(result) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        commands.entity(entity).despawn();
        match result {
            Ok(Some(snapshot)) => catalog.replace(snapshot),
            Ok(None) => {}
            Err(error) => {
                warn!(operation = name.as_str(), "{error}");
                if let Some(key) = key {
                    catalog.update_progress(ExtInstallProgress {
                        key: key.0.clone(),
                        phase: ExtInstallPhase::Failed,
                        pct: None,
                        message: error,
                    });
                } else if initial {
                    catalog.replace(ExtensionsEvent {
                        loaded: true,
                        ..default()
                    });
                }
            }
        }
    }
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
