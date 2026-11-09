use bevy_app::{App, First, Plugin, PreStartup};
use bevy_ecs::prelude::*;
use bevy_ecs::world::EntityRef;

use crate::{Instance, Page, PageScope};

type ReadInstance = for<'w> fn(EntityRef<'w>) -> Instance;
type ClaimsPage = for<'w> fn(EntityRef<'w>) -> bool;
type InstanceChanged = for<'w> fn(EntityRef<'w>) -> bool;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PagePlacement {
    Layout,
    Pane,
    Modal,
}

#[derive(Component, Clone, Copy)]
pub struct PageRegistration {
    page: &'static Page,
    placement: PagePlacement,
    instance: Option<ReadInstance>,
    instance_changed: Option<InstanceChanged>,
    instance_present: Option<ClaimsPage>,
    claim: Option<ClaimsPage>,
}

impl PageRegistration {
    pub fn page(self) -> &'static Page {
        self.page
    }

    pub fn placement(self) -> PagePlacement {
        self.placement
    }

    pub fn same_as(self, other: Self) -> bool {
        std::ptr::eq(self.page, other.page) && self.placement == other.placement
    }

    pub fn archetype_sensitive(self) -> bool {
        self.instance.is_some() || self.claim.is_some()
    }

    pub fn answers_for(self, entity: EntityRef<'_>, url: &str) -> bool {
        match self.claim {
            Some(claim) => claim(entity),
            None => self.page.answers_for(url),
        }
    }

    pub fn instance(self, entity: EntityRef<'_>) -> Instance {
        match self.instance {
            Some(read) => read(entity),
            None => Instance::default(),
        }
    }

    pub fn instance_changed(self, entity: EntityRef<'_>) -> bool {
        self.instance_changed.is_some_and(|changed| changed(entity))
    }

    pub fn instance_present(self, entity: EntityRef<'_>) -> bool {
        self.instance_present.is_some_and(|present| present(entity))
    }
}

pub struct PagePlugin {
    registration: PageRegistration,
    dependencies: Vec<fn(&mut App)>,
}

#[derive(Component)]
pub struct PageClaimDirty;

struct PageDependencyPlugin<C>(std::marker::PhantomData<fn() -> C>);

#[derive(Resource)]
struct ReactivePageDependency<C>(std::marker::PhantomData<fn() -> C>);

impl Plugin for PagePlugin {
    fn build(&self, app: &mut App) {
        for dependency in &self.dependencies {
            dependency(app);
        }
        let registration = self.registration;
        app.add_systems(PreStartup, move |mut commands: Commands| {
            commands.spawn((registration, registration.page.page_permissions()));
            if registration.page.document_url() != registration.page.url {
                commands.spawn(vmux_api::UiEventPermissions {
                    url: registration.page.document_url(),
                    owns_subtree: registration.page.owns_subtree,
                    permissions: registration.page.permissions,
                });
            }
        });
    }

    fn is_unique(&self) -> bool {
        false
    }
}

impl PagePlugin {
    pub fn in_pane(page: &'static Page) -> Self {
        Self::new(page, PagePlacement::Pane)
    }

    pub fn as_layout(page: &'static Page) -> Self {
        Self::new(page, PagePlacement::Layout)
    }

    pub fn as_modal(page: &'static Page) -> Self {
        Self::new(page, PagePlacement::Modal)
    }

    pub fn takes<C: Component + Clone>(mut self) -> Self {
        self.registration.instance = Some(Self::read::<C>);
        self.registration.instance_present = Some(Self::has::<C>);
        self.dependencies.push(Self::track_dependency::<C>);
        self
    }

    pub fn takes_reactive<C: Component + Clone>(mut self) -> Self {
        self.registration.instance = Some(Self::read::<C>);
        self.registration.instance_changed = Some(Self::changed::<C>);
        self.registration.instance_present = Some(Self::has::<C>);
        self.dependencies.push(Self::track_reactive_dependency::<C>);
        self
    }

    pub fn claims<C: Component>(mut self) -> Self {
        self.registration.claim = Some(Self::has::<C>);
        self.dependencies.push(Self::track_dependency::<C>);
        self
    }

    fn new(page: &'static Page, placement: PagePlacement) -> Self {
        Self {
            registration: PageRegistration {
                page,
                placement,
                instance: None,
                instance_changed: None,
                instance_present: None,
                claim: None,
            },
            dependencies: Vec::new(),
        }
    }

    fn track_dependency<C: Component>(app: &mut App) {
        if !app.is_plugin_added::<PageDependencyPlugin<C>>() {
            app.add_plugins(PageDependencyPlugin::<C>(std::marker::PhantomData));
        }
    }

    fn track_reactive_dependency<C: Component>(app: &mut App) {
        Self::track_dependency::<C>(app);
        if !app.world().contains_resource::<ReactivePageDependency<C>>() {
            app.insert_resource(ReactivePageDependency::<C>(std::marker::PhantomData))
                .add_systems(First, mark_dependency_changed::<C>);
        }
    }

    fn read<C: Component + Clone>(entity: EntityRef<'_>) -> Instance {
        let Some(value) = entity.get::<C>().cloned() else {
            return Instance::default();
        };
        Instance::from(move |scope: PageScope<'_>| scope.provide(value))
    }

    fn has<C: Component>(entity: EntityRef<'_>) -> bool {
        entity.contains::<C>()
    }

    fn changed<C: Component>(entity: EntityRef<'_>) -> bool {
        entity
            .get_ref::<C>()
            .is_some_and(|value| value.is_changed())
    }
}

impl<C: Component> Plugin for PageDependencyPlugin<C> {
    fn build(&self, app: &mut App) {
        app.add_observer(mark_dependency_added::<C>)
            .add_observer(mark_dependency_removed::<C>);
    }
}

fn mark_dependency_added<C: Component>(event: On<Add, C>, mut commands: Commands) {
    commands.entity(event.entity).try_insert(PageClaimDirty);
}

fn mark_dependency_removed<C: Component>(event: On<Remove, C>, mut commands: Commands) {
    commands.entity(event.entity).try_insert(PageClaimDirty);
}

fn mark_dependency_changed<C: Component>(
    changed: Query<Entity, Changed<C>>,
    mut commands: Commands,
) {
    for entity in &changed {
        commands.entity(entity).try_insert(PageClaimDirty);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Component, Clone)]
    struct Context(u8);

    #[derive(Component)]
    struct Claimed;

    #[derive(Component)]
    struct Unrelated;

    fn page() -> dioxus_core::Element {
        dioxus_core::VNode::empty()
    }

    #[test]
    fn reactive_takes_tracks_context_changes_without_a_manual_revision() {
        static PAGE: Page = Page::pane("vmux://test/", page);
        let registration = PagePlugin::in_pane(&PAGE)
            .takes_reactive::<Context>()
            .registration;
        let mut world = World::new();
        let entity = world.spawn(Context(1)).id();

        assert!(registration.instance_changed(world.entity(entity)));
        assert!(registration.instance_present(world.entity(entity)));
        world.clear_trackers();
        assert!(!registration.instance_changed(world.entity(entity)));

        world.entity_mut(entity).insert(Context(2));
        assert!(registration.instance_changed(world.entity(entity)));
        assert!(registration.instance_present(world.entity(entity)));
        assert_eq!(world.get::<Context>(entity).unwrap().0, 2);
        world.clear_trackers();
        world.entity_mut(entity).insert(Unrelated);
        assert!(!registration.instance_changed(world.entity(entity)));
        assert!(registration.instance_present(world.entity(entity)));
        world.entity_mut(entity).remove::<Context>();
        assert!(!registration.instance_present(world.entity(entity)));
    }

    #[test]
    fn only_contextual_registrations_depend_on_archetype_changes() {
        static PAGE: Page = Page::pane("vmux://test/", page);

        assert!(
            !PagePlugin::in_pane(&PAGE)
                .registration
                .archetype_sensitive()
        );
        assert!(
            PagePlugin::in_pane(&PAGE)
                .takes::<Context>()
                .registration
                .archetype_sensitive()
        );
        assert!(
            PagePlugin::in_pane(&PAGE)
                .claims::<Claimed>()
                .registration
                .archetype_sensitive()
        );
    }

    #[test]
    fn reactive_dependency_marks_only_changed_entities_for_rediscovery() {
        static PAGE: Page = Page::pane("vmux://test/", page);
        let mut app = App::new();
        app.add_plugins(PagePlugin::in_pane(&PAGE).takes_reactive::<Context>());
        let entity = app.world_mut().spawn(Context(1)).id();

        app.update();
        assert!(app.world().get::<PageClaimDirty>(entity).is_some());
        app.world_mut()
            .entity_mut(entity)
            .remove::<PageClaimDirty>();
        app.world_mut().get_mut::<Context>(entity).unwrap().0 = 2;

        app.update();

        assert!(app.world().get::<PageClaimDirty>(entity).is_some());
    }

    #[test]
    fn instance_and_claim_dependencies_are_both_installed() {
        static PAGE: Page = Page::pane("vmux://test/", page);
        let mut app = App::new();

        app.add_plugins(
            PagePlugin::in_pane(&PAGE)
                .takes_reactive::<Context>()
                .claims::<Claimed>(),
        );

        assert!(app.is_plugin_added::<PageDependencyPlugin<Context>>());
        assert!(
            app.world()
                .contains_resource::<ReactivePageDependency<Context>>()
        );
        assert!(app.is_plugin_added::<PageDependencyPlugin<Claimed>>());
    }
}
