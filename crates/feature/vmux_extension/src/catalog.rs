use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use bevy::winit::EventLoopProxyWrapper;
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use vmux_api::extension::{
    ExtFilterRequest, ExtInstallPhase, ExtInstallProgress, ExtOpenManagerRequest, ExtPinRequest,
    ExtToggleRequest, ExtUninstallRequest, ExtensionsUiState,
};
use vmux_ecs::PageMetadata;
use vmux_ecs::UiStateWrite;
use vmux_ecs::page::HostedPage;
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
                    ExtFilterRequest,
                )>::default(),
            ))
            .add_observer(page_ready)
            .add_observer(toggle)
            .add_observer(uninstall)
            .add_observer(pin)
            .add_observer(open_manager)
            .add_observer(filter)
            .add_systems(Startup, spawn)
            .add_systems(Update, (finish, publish).chain());

        #[cfg(ui)]
        app.add_plugins(
            crate::ui::ExtensionPage::MANIFEST
                .plugin()
                .hosted(HostedPage::page(
                    crate::ui::ExtensionPage::URL,
                    crate::ui::ExtensionPage::PAGE.title,
                )),
        )
        .add_systems(Update, open_page);
    }
}

#[derive(Message, Clone, Copy, Debug, Default)]
pub struct OpenManagerRequest;

#[derive(Component, Default)]
pub(crate) struct ExtensionCatalog {
    snapshot: ExtensionsUiState,
    revision: u64,
}

impl ExtensionCatalog {
    pub(crate) fn replace(&mut self, mut snapshot: ExtensionsUiState) {
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
    catalog_revision: u64,
    revision: u64,
    emitted_revision: u64,
    state: ExtensionsUiState,
}

impl ExtensionSubscriber {
    fn filter(&mut self, query: &str) {
        if self.state.query == query {
            return;
        }
        self.state.query = query.to_string();
        self.project();
        self.touch();
    }

    fn synchronize(&mut self, catalog: &ExtensionCatalog) {
        if self.catalog_revision == catalog.revision {
            return;
        }
        self.catalog_revision = catalog.revision;
        let query = std::mem::take(&mut self.state.query);
        self.state = catalog.snapshot.clone();
        self.state.query = query;
        self.project();
        self.touch();
    }

    fn project(&mut self) {
        let query = self.state.query.trim().to_ascii_lowercase();
        self.state.visible = self
            .state
            .extensions
            .iter()
            .filter(|extension| {
                query.is_empty()
                    || extension.name.to_ascii_lowercase().contains(&query)
                    || extension.id.to_ascii_lowercase().contains(&query)
                    || extension.version.to_ascii_lowercase().contains(&query)
            })
            .cloned()
            .collect();
    }

    fn touch(&mut self) {
        self.revision = self.revision.wrapping_add(1).max(1);
    }
}

#[derive(Component)]
struct ExtensionCatalogTask(Task<Result<Option<ExtensionsUiState>, String>>);

#[derive(Component)]
struct ExtensionCatalogKey(String);

#[derive(Component)]
struct InitialCatalogLoad;

fn spawn(
    profile: vmux_ecs::profile::CurrentProfile,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let Some((profile, paths)) = profile.profile().zip(profile.paths()) else {
        return;
    };
    let profile = profile.clone().into_id();
    let store = store::ExtensionStore::at(paths.extensions());
    let wake = vmux_ecs::wake::Wake::beside(proxy.as_deref());
    let task = IoTaskPool::get().spawn(async move {
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
    extension_pages: Query<(&HostedPage, &vmux_ecs::page::PageManifest)>,
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

fn toggle(
    trigger: On<UiInput<ExtToggleRequest>>,
    profile: vmux_ecs::profile::CurrentProfile,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let Some((profile, paths)) = profile.profile().zip(profile.paths()) else {
        return;
    };
    let profile = profile.clone().into_id();
    let store = store::ExtensionStore::at(paths.extensions());
    let request = trigger.event().payload.clone();
    let wake = vmux_ecs::wake::Wake::beside(proxy.as_deref());
    let task = IoTaskPool::get().spawn(async move {
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

fn uninstall(
    trigger: On<UiInput<ExtUninstallRequest>>,
    profile: vmux_ecs::profile::CurrentProfile,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let Some((profile, paths)) = profile.profile().zip(profile.paths()) else {
        return;
    };
    let profile = profile.clone().into_id();
    let store = store::ExtensionStore::at(paths.extensions());
    let request = trigger.event().payload.clone();
    let wake = vmux_ecs::wake::Wake::beside(proxy.as_deref());
    let task = IoTaskPool::get().spawn(async move {
        let result = store
            .uninstall_for_profile(&profile, &request.id)
            .and_then(|()| store.snapshot(&profile))
            .map(Some);
        drop(wake);
        result
    });
    commands.spawn((Name::new("Uninstall extension"), ExtensionCatalogTask(task)));
}

fn pin(
    trigger: On<UiInput<ExtPinRequest>>,
    profile: vmux_ecs::profile::CurrentProfile,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let Some((profile, paths)) = profile.profile().zip(profile.paths()) else {
        return;
    };
    let profile = profile.clone().into_id();
    let store = store::ExtensionStore::at(paths.extensions());
    let request = trigger.event().payload.clone();
    let key = request.id.clone();
    let wake = vmux_ecs::wake::Wake::beside(proxy.as_deref());
    let task = IoTaskPool::get().spawn(async move {
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

fn finish(
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
                    catalog.replace(ExtensionsUiState {
                        loaded: true,
                        ..default()
                    });
                }
            }
        }
    }
}

fn open_manager(
    _trigger: On<UiInput<ExtOpenManagerRequest>>,
    mut requests: MessageWriter<OpenManagerRequest>,
) {
    requests.write(OpenManagerRequest);
}

fn filter(
    trigger: On<UiInput<ExtFilterRequest>>,
    mut subscribers: Query<&mut ExtensionSubscriber>,
) {
    let Ok(mut subscriber) = subscribers.get_mut(trigger.event().webview) else {
        return;
    };
    subscriber.filter(&trigger.event().payload.query);
}

#[cfg(ui)]
fn open_page(
    mut requests: MessageReader<OpenManagerRequest>,
    mut pages: MessageWriter<vmux_layout::stack::OpenRequest>,
) {
    for _ in requests.read() {
        pages.write(vmux_layout::stack::OpenRequest {
            url: Some(crate::ui::ExtensionPage::URL.to_string()),
        });
    }
}

fn publish(
    catalog: Query<&ExtensionCatalog>,
    mut subscribers: Query<(Entity, &mut ExtensionSubscriber)>,
    layout_ui: Query<(), With<LayoutUiStateUpdates>>,
    mut commands: Commands,
) {
    let Ok(catalog) = catalog.single() else {
        return;
    };
    for (entity, mut subscriber) in &mut subscribers {
        subscriber.synchronize(catalog);
        if subscriber.emitted_revision == subscriber.revision {
            continue;
        }
        commands.trigger(UiStateWrite::<ExtensionsUiState>::from_event(
            entity,
            &subscriber.state,
        ));
        if layout_ui.contains(entity) {
            commands.trigger(
                UiStateWrite::<vmux_layout::state::LayoutUiState>::from_event(
                    entity,
                    &catalog.snapshot,
                ),
            );
        }
        subscriber.emitted_revision = subscriber.revision;
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
            HostedPage::page("vmux://extensions/", "Extensions"),
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
