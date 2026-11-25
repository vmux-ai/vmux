use bevy::prelude::*;
use bevy_cef::prelude::UiInput;
use vmux_ecs::{PageMetadata, UiState, UiStateWrite};

use crate::catalog::{Catalog, Inventory, ProjectionSet};
use crate::route::ToolRoute;
use crate::state::{
    ToolOperationKey, ToolOperationNotice, ToolsFilterRequest, ToolsNavigateRequest,
    ToolsRefreshRequest, ToolsUiState,
};
use crate::tool_inventory::{Registry, Sequence};

pub(crate) struct ToolPagePlugin;

impl Plugin for ToolPagePlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(ready)
            .add_observer(filter)
            .add_observer(navigate)
            .add_observer(refresh)
            .add_systems(Update, retire.before(ProjectionSet))
            .add_systems(Update, publish.in_set(ProjectionSet));
    }
}

#[derive(Component)]
#[require(UiState<ToolsUiState>)]
pub(crate) struct State {
    pub(crate) generation: u64,
    source: Option<(Entity, crate::ToolStore)>,
    route: String,
    query: String,
    notice: Option<ToolOperationNotice>,
    revision: u64,
    emitted: u64,
    catalog: Option<u64>,
}

impl State {
    pub(crate) fn new(generation: u64) -> Self {
        Self {
            generation,
            source: None,
            route: String::new(),
            query: String::new(),
            notice: None,
            revision: 1,
            emitted: 0,
            catalog: None,
        }
    }

    pub(crate) fn begin(&mut self) {
        self.notice = None;
        self.touch();
    }

    pub(crate) fn complete(&mut self, context: &Context, notice: ToolOperationNotice) {
        if self.generation != context.generation || !self.bound(context.registry, &context.store) {
            return;
        }
        self.notice = Some(notice);
        self.touch();
    }

    pub(crate) fn canceled(&mut self, context: &Context) {
        if self.generation == context.generation && self.bound(context.registry, &context.store) {
            self.touch();
        }
    }

    pub(crate) fn bound(&self, registry: Entity, store: &crate::ToolStore) -> bool {
        self.source
            .as_ref()
            .is_some_and(|(source, captured)| *source == registry && captured == store)
    }

    pub(crate) fn bind(&mut self, registry: Entity, store: &crate::ToolStore, generation: u64) {
        if self.bound(registry, store) {
            return;
        }
        self.source = Some((registry, store.clone()));
        self.generation = generation;
        self.notice = None;
        self.touch();
    }

    pub(crate) fn navigate(&mut self, route: &ToolRoute) {
        if self.route == route.id() {
            return;
        }
        self.route = route.id().to_string();
        self.touch();
    }

    fn filter(&mut self, query: &str) {
        if self.query == query {
            return;
        }
        self.query = query.to_string();
        self.touch();
    }

    fn touch(&mut self) {
        self.revision = self.revision.wrapping_add(1).max(1);
    }
}

#[derive(Component)]
pub(crate) struct Context {
    pub(crate) operation_id: u64,
    pub(crate) target: Entity,
    pub(crate) generation: u64,
    pub(crate) operation: ToolOperationKey,
    pub(crate) registry: Entity,
    pub(crate) store: crate::ToolStore,
}

fn ready(
    trigger: On<UiInput<vmux_ecs::page::PageReady>>,
    pages: Query<&PageMetadata>,
    mut states: Query<&mut State>,
    mut registries: Query<(Entity, &crate::ToolStore, &mut Sequence), With<Registry>>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let Ok(page) = pages.get(entity) else {
        return;
    };
    let Some(route) = ToolRoute::parse(&page.url) else {
        return;
    };
    let mut refresh = None;
    if let Ok(mut state) = states.get_mut(entity) {
        if let Ok((registry, store, mut sequence)) = registries.single_mut() {
            if !state.bound(registry, store) {
                state.bind(registry, store, sequence.next());
            }
            refresh = Some(registry);
        }
        state.navigate(&route);
    } else {
        let mut state = State::new(0);
        if let Ok((registry, store, mut sequence)) = registries.single_mut() {
            state.bind(registry, store, sequence.next());
            refresh = Some(registry);
        }
        state.navigate(&route);
        commands.entity(entity).insert(state);
    }
    if let Some(registry) = refresh {
        commands.trigger(crate::tool_inventory::RefreshRequest {
            entity: registry,
            refresh_catalogs: false,
        });
    }
}

fn filter(trigger: On<UiInput<ToolsFilterRequest>>, mut states: Query<&mut State>) {
    if let Ok(mut state) = states.get_mut(trigger.event().webview) {
        state.filter(&trigger.event().payload.query);
    }
}

fn navigate(
    trigger: On<UiInput<ToolsNavigateRequest>>,
    mut states: Query<&mut State>,
    mut pages: Query<&mut PageMetadata>,
) {
    let Some(route) = ToolRoute::from_request(&trigger.event().payload) else {
        return;
    };
    let entity = trigger.event().webview;
    let Ok(mut page) = pages.get_mut(entity) else {
        return;
    };
    if ToolRoute::parse(&page.url).is_none() {
        return;
    }
    if let Ok(mut state) = states.get_mut(entity) {
        state.navigate(&route);
    }
    page.url = route.url();
}

fn refresh(
    trigger: On<UiInput<ToolsRefreshRequest>>,
    mut registry: Query<(Entity, &crate::ToolStore, &mut Sequence), With<Registry>>,
    pages: Query<&PageMetadata>,
    states: Query<(), With<State>>,
    mut commands: Commands,
) {
    let Ok(page) = pages.get(trigger.event().webview) else {
        return;
    };
    let Some(route) = ToolRoute::parse(&page.url) else {
        return;
    };
    let Ok((registry, store, mut sequence)) = registry.single_mut() else {
        return;
    };
    if !states.contains(trigger.event().webview) {
        let mut state = State::new(sequence.next());
        state.bind(registry, store, state.generation);
        state.navigate(&route);
        commands.entity(trigger.event().webview).insert(state);
    }
    commands.trigger(crate::tool_inventory::RefreshRequest {
        entity: registry,
        refresh_catalogs: trigger.event().payload.refresh,
    });
}

fn retire(pages: Query<(Entity, &PageMetadata), With<State>>, mut commands: Commands) {
    for (entity, page) in &pages {
        if ToolRoute::parse(&page.url).is_none() {
            commands
                .entity(entity)
                .remove::<(State, UiState<ToolsUiState>)>();
        }
    }
}

fn publish(
    mut registry: Query<(Entity, &Catalog, &crate::ToolStore, &mut Sequence), With<Registry>>,
    inventory: Inventory,
    mut states: Query<(Entity, &mut State)>,
    contexts: Query<&Context>,
    mut commands: Commands,
) {
    let Ok((registry, catalog, store, mut sequence)) = registry.single_mut() else {
        return;
    };
    let mut snapshot = None;
    for (entity, mut state) in &mut states {
        if !state.bound(registry, store) {
            state.bind(registry, store, sequence.next());
        }
        if state.emitted == state.revision && state.catalog == Some(catalog.revision) {
            continue;
        }
        if snapshot.is_none() {
            snapshot = inventory.snapshot(registry);
        }
        let Some(snapshot) = &snapshot else {
            continue;
        };
        let Some(view) = inventory.view(registry, &state.route, &state.query) else {
            continue;
        };
        let mut pending = Vec::new();
        for context in &contexts {
            if context.target == entity
                && context.generation == state.generation
                && context.registry == registry
                && &context.store == store
            {
                pending.push((context.operation_id, context.operation.clone()));
            }
        }
        pending.sort_by_key(|(order, _)| *order);
        let mut value = ToolsUiState {
            snapshot: snapshot.clone(),
            view,
            notice: state.notice.clone(),
            pending: Vec::new(),
        };
        for (_, operation) in pending {
            value.pending.push(operation);
        }
        commands.trigger(UiStateWrite::<ToolsUiState>::from_event(entity, &value));
        state.emitted = state.revision;
        state.catalog = Some(catalog.revision);
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::catalog::Received;
    use crate::state::{
        ToolCategory, ToolImportRequest, ToolItem, ToolProvider, ToolProviderMetadata, ToolStatus,
        ToolsSnapshot,
    };
    use crate::{ToolStore, ToolsManifest};
    use std::time::{Duration, Instant};
    use vmux_ecs::PageOpenRequest;
    use vmux_ecs::page::PageReady;

    #[derive(Component)]
    struct Captured(ToolsUiState);

    fn capture(
        trigger: On<UiStateWrite<ToolsUiState>>,
        values: Query<&Captured>,
        mut commands: Commands,
    ) {
        let page = trigger.event().webview();
        let previous = values.get(page).ok().map(|captured| captured.0.clone());
        let value = <ToolsUiState as vmux_api::UiState>::from_updates(
            previous,
            vec![trigger.event().update().clone()],
        );
        commands.entity(page).insert(Captured(value));
    }

    pub(crate) struct Fixture {
        pub(crate) app: App,
        pub(crate) directory: tempfile::TempDir,
        pub(crate) store: ToolStore,
        pub(crate) registry: Entity,
    }

    impl Fixture {
        pub(crate) fn new() -> Self {
            let directory = tempfile::tempdir().unwrap();
            let store = ToolStore::new(
                directory.path().join("config"),
                directory.path().join("home"),
            );
            ToolsManifest::default()
                .write_to(&store.manifest_path())
                .unwrap();
            let mut app = App::new();
            app.world_mut()
                .spawn(crate::ToolProviderId(ToolProvider::new("npm")));
            app.add_plugins((
                bevy::app::TaskPoolPlugin::default(),
                crate::ToolRuntimePlugin,
                crate::host::ToolHostPlugin,
                crate::npm::NpmToolPlugin,
            ))
            .add_observer(capture)
            .add_message::<PageOpenRequest>();
            let registry = app
                .world_mut()
                .spawn((
                    Registry::default(),
                    Catalog::default(),
                    store.clone(),
                    Sequence::default(),
                ))
                .id();
            app.world_mut().spawn((
                crate::npm::NpmProvider,
                crate::ToolProviderId(ToolProvider::new("npm")),
                crate::ToolProviderBinding::new::<crate::Feature>(2),
                crate::observation::Handler,
                crate::observation::PackageProvider,
            ));
            let mut providers = Vec::new();
            for (id, title, brewfile) in [("npm", "NPM", false), ("homebrew", "Homebrew", true)] {
                providers.push(ToolProviderMetadata {
                    provider: ToolProvider::new(id),
                    title: title.into(),
                    title_message_id: String::new(),
                    route_title: title.into(),
                    route_title_message_id: String::new(),
                    short_label: title.into(),
                    route: id.into(),
                    rank: 0,
                    thumbnails: false,
                    apply: brewfile,
                    brewfile,
                });
            }
            let mut items = Vec::new();
            for (id, name, status, managed) in [
                ("alpha", "Alpha", ToolStatus::Installed, true),
                ("beta", "beta", ToolStatus::Installed, false),
                ("zebra", "Zebra", ToolStatus::Outdated, false),
                ("gamma", "Gamma", ToolStatus::Available, true),
            ] {
                items.push(ToolItem {
                    provider: ToolProvider::new("npm"),
                    id: id.into(),
                    name: name.into(),
                    icon: None,
                    version: None,
                    detail: String::new(),
                    status,
                    managed,
                    operations: Vec::new(),
                });
            }
            app.world_mut().trigger(Received {
                entity: registry,
                snapshot: ToolsSnapshot {
                    loaded: true,
                    providers,
                    categories: vec![ToolCategory {
                        provider: ToolProvider::new("npm"),
                        items,
                    }],
                    ..Default::default()
                },
            });
            Self {
                app,
                directory,
                store,
                registry,
            }
        }

        pub(crate) fn page(&mut self, url: &str) -> Entity {
            let page = self
                .app
                .world_mut()
                .spawn(PageMetadata {
                    url: url.into(),
                    ..Default::default()
                })
                .id();
            self.app.world_mut().trigger(UiInput {
                webview: page,
                payload: PageReady {},
            });
            self.app.update();
            page
        }

        pub(crate) fn state(&self, page: Entity) -> &ToolsUiState {
            &self
                .app
                .world()
                .get::<Captured>(page)
                .expect("typed page state was not published")
                .0
        }

        pub(crate) fn import(&mut self, page: Entity, name: &str) {
            let path = self.directory.path().join(format!("{name}.json"));
            std::fs::write(&path, format!("{{\"dependencies\":{{\"{name}\":\"1\"}}}}")).unwrap();
            self.app.world_mut().trigger(UiInput {
                webview: page,
                payload: ToolImportRequest {
                    provider: ToolProvider::new("npm"),
                    value: path.to_string_lossy().into_owned(),
                },
            });
        }

        pub(crate) fn drain(&mut self) {
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                self.app.update();
                if self
                    .app
                    .world_mut()
                    .query::<&Context>()
                    .iter(self.app.world())
                    .next()
                    .is_none()
                {
                    return;
                }
                assert!(
                    Instant::now() < deadline,
                    "accepted Tool operation did not drain"
                );
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    }

    #[test]
    fn canonical_items_determine_filter_order_and_independent_page_routes() {
        let mut fixture = Fixture::new();
        let first = fixture.page("vmux://tools/npm");
        let second = fixture.page("vmux://tools/homebrew");
        let names: Vec<_> = fixture.state(first).view.categories[0]
            .items
            .iter()
            .map(|item| item.name.as_str())
            .collect();
        assert_eq!(names, ["Zebra", "Alpha", "beta", "Gamma"]);
        assert_eq!(fixture.state(first).view.visible_count, 4);
        assert!(fixture.state(second).view.show_brewfile);
        fixture.app.world_mut().trigger(UiInput {
            webview: first,
            payload: ToolsFilterRequest {
                query: "  BeTa  ".into(),
            },
        });
        fixture.app.update();
        assert_eq!(fixture.state(first).view.visible_count, 1);
        assert_eq!(fixture.state(first).view.categories[0].items[0].id, "beta");
        assert_eq!(fixture.state(second).view.query, "");
        fixture.app.world_mut().trigger(UiInput {
            webview: first,
            payload: ToolsNavigateRequest {
                url: "vmux://tools/homebrew".into(),
            },
        });
        fixture.app.update();
        assert_eq!(
            fixture.app.world().get::<PageMetadata>(first).unwrap().url,
            "vmux://tools/homebrew"
        );
        assert_eq!(fixture.state(first).view.route, "homebrew");
        assert_eq!(fixture.state(first).view.query, "  BeTa  ");
        assert_eq!(fixture.state(second).view.route, "homebrew");
    }

    #[test]
    fn catalog_items_cannot_be_selected_as_the_provider_for_a_page_import() {
        let mut fixture = Fixture::new();
        let page = fixture.page("vmux://tools/npm");
        fixture.import(page, "typescript");
        fixture.drain();
        let notice = fixture.state(page).notice.as_ref().unwrap();
        assert!(notice.success, "{}", notice.message);
        assert_eq!(
            ToolsManifest::read(&fixture.store.manifest_path())
                .unwrap()
                .packages["npm"],
            ["typescript"]
        );
        assert!(fixture.store.lease().is_some());
    }

    #[test]
    fn accepted_imports_publish_pending_and_notices_only_to_their_own_page() {
        let mut fixture = Fixture::new();
        let first = fixture.page("vmux://tools/npm");
        let second = fixture.page("vmux://tools/npm");
        let lease = fixture.store.lease().unwrap();
        fixture.import(first, "first");
        fixture.import(second, "second");
        fixture.app.update();
        assert_eq!(fixture.state(first).pending.len(), 1);
        assert_eq!(fixture.state(second).pending.len(), 1);
        assert!(fixture.state(first).notice.is_none());
        drop(lease);
        fixture.drain();
        assert!(fixture.state(first).pending.is_empty());
        assert!(fixture.state(second).pending.is_empty());
        assert!(
            fixture.state(first).notice.as_ref().unwrap().success,
            "{}",
            fixture.state(first).notice.as_ref().unwrap().message
        );
        assert!(
            fixture.state(second).notice.as_ref().unwrap().success,
            "{}",
            fixture.state(second).notice.as_ref().unwrap().message
        );
        let manifest = ToolsManifest::read(&fixture.store.manifest_path()).unwrap();
        assert_eq!(
            manifest.packages.get("npm").cloned().unwrap_or_default(),
            ["first", "second"]
        );
    }

    #[test]
    fn recreating_a_page_refuses_old_delivery_while_accepted_background_work_finishes() {
        let mut fixture = Fixture::new();
        let page = fixture.page("vmux://tools/npm");
        let lease = fixture.store.lease().unwrap();
        fixture.import(page, "retained");
        fixture.app.update();
        assert_eq!(fixture.state(page).pending.len(), 1);
        fixture
            .app
            .world_mut()
            .get_mut::<PageMetadata>(page)
            .unwrap()
            .url = "https://example.test/".into();
        fixture.app.update();
        assert!(fixture.app.world().get::<State>(page).is_none());
        fixture
            .app
            .world_mut()
            .get_mut::<PageMetadata>(page)
            .unwrap()
            .url = "vmux://tools/npm".into();
        fixture.app.world_mut().trigger(UiInput {
            webview: page,
            payload: PageReady {},
        });
        fixture.app.update();
        assert!(fixture.state(page).pending.is_empty());
        drop(lease);
        fixture.drain();
        assert_eq!(
            ToolsManifest::read(&fixture.store.manifest_path())
                .unwrap()
                .packages["npm"],
            ["retained"]
        );
        assert!(fixture.state(page).notice.is_none());
    }

    #[test]
    fn replacing_a_store_refuses_queued_io_and_clears_only_the_old_pending_projection() {
        let mut fixture = Fixture::new();
        let page = fixture.page("vmux://tools/npm");
        let before = std::fs::read(fixture.store.manifest_path()).unwrap();
        let lease = fixture.store.lease().unwrap();
        fixture.import(page, "refused");
        fixture.app.update();
        assert_eq!(fixture.state(page).pending.len(), 1);
        let replacement = ToolStore::new(
            fixture.directory.path().join("replacement"),
            fixture.store.home(),
        );
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.registry)
            .insert(replacement.clone());
        fixture.drain();
        drop(lease);
        assert_eq!(
            std::fs::read(fixture.store.manifest_path()).unwrap(),
            before
        );
        assert!(!replacement.manifest_path().exists());
        assert!(fixture.state(page).pending.is_empty());
        assert!(fixture.state(page).notice.is_none());
    }

    #[test]
    fn replacing_an_admitted_store_refuses_the_next_native_dispatch() {
        let mut fixture = Fixture::new();
        let page = fixture.page("vmux://tools/npm");
        let before = std::fs::read(fixture.store.manifest_path()).unwrap();
        fixture.import(page, "refused");
        fixture.app.update();
        assert!(
            fixture
                .app
                .world_mut()
                .query_filtered::<Entity, (With<Context>, With<crate::ToolStoreTarget>)>()
                .iter(fixture.app.world())
                .next()
                .is_some()
        );
        let replacement = ToolStore::new(
            fixture.directory.path().join("replacement"),
            fixture.store.home(),
        );
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.registry)
            .insert(replacement.clone());
        fixture.drain();
        assert_eq!(
            std::fs::read(fixture.store.manifest_path()).unwrap(),
            before
        );
        assert!(!replacement.manifest_path().exists());
        assert!(fixture.state(page).pending.is_empty());
        assert!(fixture.state(page).notice.is_none());
    }

    #[test]
    fn page_ready_starts_the_initial_scan_once_and_publishes_declared_providers() {
        let directory = tempfile::tempdir().unwrap();
        let store = ToolStore::new(
            directory.path().join("config"),
            directory.path().join("home"),
        );
        ToolsManifest::default()
            .write_to(&store.manifest_path())
            .unwrap();
        let mut app = App::new();
        app.add_plugins((
            bevy::app::TaskPoolPlugin::default(),
            vmux_feature::manifest::FeaturePlugin::<crate::Feature>::default(),
            crate::ToolRuntimePlugin,
            crate::host::ToolHostPlugin,
        ))
        .add_observer(capture)
        .add_message::<PageOpenRequest>();
        let registry = app
            .world_mut()
            .spawn((
                Registry::default(),
                Catalog::default(),
                store.clone(),
                ToolsManifest::default(),
                Sequence::default(),
            ))
            .id();
        let page = app
            .world_mut()
            .spawn(PageMetadata {
                url: "vmux://tools/acp".into(),
                ..Default::default()
            })
            .id();
        let lease = store.lease().unwrap();
        app.world_mut().trigger(UiInput {
            webview: page,
            payload: PageReady {},
        });
        app.update();
        let initial = &app.world().get::<Captured>(page).unwrap().0;
        let routes: Vec<_> = initial
            .snapshot
            .providers
            .iter()
            .map(|provider| provider.route.as_str())
            .collect();
        assert_eq!(routes, ["homebrew", "homebrew", "npm", "mcp", "dotfiles"]);
        assert!(!initial.snapshot.loaded);
        assert!(app.world().get::<Registry>(registry).unwrap().dirty);
        let generation = app.world().get::<Registry>(registry).unwrap().generation;
        drop(lease);
        app.update();
        let scan = app
            .world_mut()
            .query_filtered::<Entity, With<crate::tool_inventory::Scan>>()
            .single(app.world())
            .unwrap();
        app.world_mut().trigger(UiInput {
            webview: page,
            payload: PageReady {},
        });
        app.update();
        assert_eq!(
            app.world().get::<Registry>(registry).unwrap().generation,
            generation
        );
        assert!(app.world().get_entity(scan).is_ok());
        let deadline = Instant::now() + Duration::from_secs(5);
        while !app.world().get::<Captured>(page).unwrap().0.snapshot.loaded {
            app.update();
            assert!(
                Instant::now() < deadline,
                "initial tool scan did not finish"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
        app.world_mut().trigger(UiInput {
            webview: page,
            payload: PageReady {},
        });
        app.update();
        assert!(!app.world().get::<Registry>(registry).unwrap().dirty);
        assert!(
            app.world_mut()
                .query_filtered::<Entity, With<crate::tool_inventory::Scan>>()
                .iter(app.world())
                .next()
                .is_none()
        );
    }
}
