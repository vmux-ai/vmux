use bevy::prelude::*;
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use vmux_api::extension::{
    ExtFilterRequest, ExtInstallPhase, ExtInstallProgress, ExtOpenManagerRequest, ExtPinRequest,
    ExtToggleRequest, ExtUninstallRequest, ExtensionsUiState,
};
use vmux_ecs::PageMetadata;
use vmux_ecs::UiStateWrite;
use vmux_layout::LayoutUiStateUpdates;

use crate::host::{Binding, RuntimeIds};
use crate::{Catalog, ExtensionSet, Mutation, Request};
use vmux_ecs::{OperationError, OperationState, Revision};

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
            .add_systems(
                Update,
                (finish, project, publish).chain().after(ExtensionSet::Step),
            );

        #[cfg(ui)]
        app.add_systems(Update, open_page);
    }
}

#[derive(Message, Clone, Copy, Debug, Default)]
pub struct OpenManagerRequest;

#[derive(Component, Default)]
pub(crate) struct ExtensionCatalog {
    snapshot: ExtensionsUiState,
    revision: u64,
    root_revision: Option<Revision>,
    root_generation: Option<u64>,
    root_loaded: bool,
    profile: String,
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
struct PendingCatalogOperation;

#[derive(Component)]
struct ExtensionCatalogKey(String);

fn spawn(mut commands: Commands) {
    commands.spawn((Name::new("Extension catalog"), ExtensionCatalog::default()));
}

fn page_ready(
    trigger: On<UiInput<vmux_api::PageReady>>,
    pages: Query<(Has<vmux_layout::LayoutCef>, Option<&PageMetadata>)>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let Ok((layout, metadata)) = pages.get(webview) else {
        return;
    };
    let extension = metadata.is_some_and(|metadata| {
        let Some(route) = vmux_api::VmuxRoute::parse(&metadata.url) else {
            return false;
        };
        let Some(expected) = vmux_api::VmuxRoute::parse(vmux_tool::EXTENSIONS_URL) else {
            return false;
        };
        route.same_page(&expected)
    });
    if layout || extension {
        commands
            .entity(webview)
            .insert(ExtensionSubscriber::default());
    } else {
        commands.entity(webview).remove::<ExtensionSubscriber>();
    }
}

fn toggle(
    trigger: On<UiInput<ExtToggleRequest>>,
    profile: vmux_ecs::profile::CurrentProfile,
    root: Single<Entity, With<Binding>>,
    catalog: Catalog,
    mut commands: Commands,
) {
    let Some(profile) = profile.profile() else {
        return;
    };
    let Some(ticket) = catalog.ticket(*root) else {
        return;
    };
    commands.spawn((
        Request {
            ticket: ticket.clone(),
            profile: profile.clone().into_id(),
            mutation: Mutation::Toggle(trigger.event().payload.clone()),
        }
        .bundle(),
        PendingCatalogOperation,
    ));
}

fn uninstall(
    trigger: On<UiInput<ExtUninstallRequest>>,
    profile: vmux_ecs::profile::CurrentProfile,
    root: Single<Entity, With<Binding>>,
    catalog: Catalog,
    mut commands: Commands,
) {
    let Some(profile) = profile.profile() else {
        return;
    };
    let Some(ticket) = catalog.ticket(*root) else {
        return;
    };
    commands.spawn((
        Request {
            ticket: ticket.clone(),
            profile: profile.clone().into_id(),
            mutation: Mutation::Remove(trigger.event().payload.clone()),
        }
        .bundle(),
        PendingCatalogOperation,
    ));
}

fn pin(
    trigger: On<UiInput<ExtPinRequest>>,
    profile: vmux_ecs::profile::CurrentProfile,
    root: Single<Entity, With<Binding>>,
    catalog: Catalog,
    mut commands: Commands,
) {
    let Some(profile) = profile.profile() else {
        return;
    };
    let Some(ticket) = catalog.ticket(*root) else {
        return;
    };
    let request = trigger.event().payload.clone();
    commands.spawn((
        Request {
            ticket: ticket.clone(),
            profile: profile.clone().into_id(),
            mutation: Mutation::Pin(request.clone()),
        }
        .bundle(),
        PendingCatalogOperation,
        ExtensionCatalogKey(request.id),
    ));
}

fn finish(
    operations: Query<
        (
            Entity,
            &OperationState,
            Option<&OperationError>,
            Option<&ExtensionCatalogKey>,
        ),
        With<PendingCatalogOperation>,
    >,
    mut catalog: Single<&mut ExtensionCatalog>,
    mut commands: Commands,
) {
    for (entity, state, error, key) in &operations {
        if matches!(state, OperationState::Pending | OperationState::Running) {
            continue;
        }
        if *state == OperationState::Failed
            && let Some(error) = error
        {
            warn!("extension operation failed: {}", error.message());
            if let Some(key) = key {
                catalog.update_progress(ExtInstallProgress {
                    key: key.0.clone(),
                    phase: ExtInstallPhase::Failed,
                    pct: None,
                    message: error.message().into(),
                });
            }
        }
        commands
            .entity(entity)
            .remove::<(PendingCatalogOperation, ExtensionCatalogKey)>();
    }
}

fn project(
    roots: Query<(Entity, &Revision, &RuntimeIds, Has<crate::Loaded>), With<Binding>>,
    source: Catalog,
    profile: vmux_ecs::profile::CurrentProfile,
    mut catalog: Single<&mut ExtensionCatalog>,
) {
    let Some(profile) = profile.profile() else {
        return;
    };
    let profile = profile.clone().into_id();
    let Ok((root, revision, runtime, loaded)) = roots.single() else {
        return;
    };
    let generation = source.ticket(root).map(|ticket| ticket.generation);
    if catalog.root_revision == Some(*revision)
        && catalog.root_generation == generation
        && catalog.root_loaded == loaded
        && catalog.profile == profile
    {
        return;
    }
    let snapshot = match source.snapshot(root) {
        Ok(snapshot) => snapshot.snapshot(&profile, &runtime.0),
        Err(error) => {
            warn!("extension catalog unavailable: {error}");
            ExtensionsUiState {
                loaded: true,
                ..Default::default()
            }
        }
    };
    catalog.replace(snapshot);
    catalog.root_revision = Some(*revision);
    catalog.root_generation = generation;
    catalog.root_loaded = loaded;
    catalog.profile = profile;
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
            url: Some(vmux_tool::EXTENSIONS_URL.to_string()),
            policy: vmux_layout::stack::OpenPolicy::FocusReusable,
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
        let extension = app
            .world_mut()
            .spawn(PageMetadata {
                url: vmux_tool::EXTENSIONS_URL.to_string(),
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
