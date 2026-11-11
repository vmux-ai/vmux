use std::collections::{BTreeMap, HashMap, HashSet};

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use vmux_ecs::RegistrationOrder;

use crate::ToolProviderId;
use crate::state::{
    ToolCategory, ToolItem, ToolOperationKind, ToolProvider, ToolProviderMetadata, ToolStatus,
    ToolsSnapshot,
};

pub(crate) struct Plugin;

impl bevy::prelude::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        app.add_observer(replace)
            .add_observer(remove)
            .add_systems(Update, track.before(crate::host::ProjectionSet));
    }
}

#[derive(Component, Default)]
pub(crate) struct Catalog {
    pub(crate) revision: u64,
    pub(crate) loaded: bool,
    pub(crate) root: String,
    pub(crate) providers: Vec<ToolProviderMetadata>,
    categories: Vec<ToolProvider>,
    error: String,
}

#[derive(EntityEvent)]
pub(crate) struct Received {
    #[event_target]
    pub(crate) entity: Entity,
    pub(crate) snapshot: ToolsSnapshot,
}

#[derive(Component)]
pub(crate) struct Item;

#[derive(Component)]
pub(crate) struct Id(String);

#[derive(Component)]
struct Version(Option<String>);

#[derive(Component)]
struct Icon(Option<String>);

#[derive(Component)]
struct Detail(String);

#[derive(Component)]
struct Status(ToolStatus);

#[derive(Component)]
struct Managed;

#[derive(Component)]
struct Operations(Vec<ToolOperationKind>);

type Items<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static ToolProviderId,
        &'static Id,
        &'static Name,
        &'static RegistrationOrder,
        &'static Version,
        &'static Icon,
        &'static Detail,
        &'static Status,
        &'static Operations,
        Has<Managed>,
        &'static ChildOf,
    ),
    With<Item>,
>;

#[derive(SystemParam)]
pub(crate) struct Inventory<'w, 's> {
    catalogs: Query<'w, 's, &'static Catalog>,
    items: Items<'w, 's>,
}

impl Inventory<'_, '_> {
    pub(crate) fn snapshot(&self, entity: Entity) -> Option<ToolsSnapshot> {
        let catalog = self.catalogs.get(entity).ok()?;
        let mut snapshot = ToolsSnapshot {
            loaded: catalog.loaded,
            root: catalog.root.clone(),
            providers: catalog.providers.clone(),
            error: catalog.error.clone(),
            ..Default::default()
        };
        let mut categories = BTreeMap::<ToolProvider, Vec<(u32, ToolItem)>>::new();
        for (
            _,
            provider,
            id,
            name,
            order,
            version,
            icon,
            detail,
            status,
            operations,
            managed,
            parent,
        ) in &self.items
        {
            if parent.parent() != entity {
                continue;
            }
            match status.0 {
                ToolStatus::Installed => snapshot.installed += 1,
                ToolStatus::Outdated => {
                    snapshot.installed += 1;
                    snapshot.updates += 1;
                }
                ToolStatus::Conflict => snapshot.conflicts += 1,
                _ => {}
            }
            categories.entry(provider.0.clone()).or_default().push((
                order.0,
                ToolItem {
                    provider: provider.0.clone(),
                    id: id.0.clone(),
                    name: name.as_str().to_string(),
                    icon: icon.0.clone(),
                    version: version.0.clone(),
                    detail: detail.0.clone(),
                    status: status.0,
                    managed,
                    operations: operations.0.clone(),
                },
            ));
        }
        for provider in &catalog.categories {
            let mut items = categories.remove(provider).unwrap_or_default();
            items.sort_by_key(|(order, _)| *order);
            let mut category = ToolCategory {
                provider: provider.clone(),
                items: Vec::new(),
            };
            for (_, item) in items {
                category.items.push(item);
            }
            snapshot.categories.push(category);
        }
        Some(snapshot)
    }
}

fn replace(
    trigger: On<Received>,
    mut catalogs: Query<&mut Catalog>,
    items: Query<(Entity, &ToolProviderId, &Id, &ChildOf), With<Item>>,
    mut commands: Commands,
) {
    let event = trigger.event();
    let Ok(mut catalog) = catalogs.get_mut(event.entity) else {
        return;
    };
    let mut existing = HashMap::new();
    for (entity, provider, id, parent) in &items {
        if parent.parent() == event.entity {
            existing.insert((provider.0.clone(), id.0.clone()), entity);
        }
    }
    let mut seen = HashSet::new();
    let mut categories = Vec::new();
    for category in &event.snapshot.categories {
        categories.push(category.provider.clone());
        for (order, item) in category.items.iter().enumerate() {
            let key = (item.provider.clone(), item.id.clone());
            if !seen.insert(key.clone()) {
                continue;
            }
            let fields = (
                Name::new(item.name.clone()),
                ToolProviderId(item.provider.clone()),
                Id(item.id.clone()),
                RegistrationOrder(order as u32),
                Version(item.version.clone()),
                Icon(item.icon.clone()),
                Detail(item.detail.clone()),
                Status(item.status),
                Operations(item.operations.clone()),
            );
            match existing.remove(&key) {
                Some(entity) => {
                    let mut item_entity = commands.entity(entity);
                    item_entity.insert(fields);
                    if item.managed {
                        item_entity.insert(Managed);
                    } else {
                        item_entity.remove::<Managed>();
                    }
                }
                None => {
                    if item.managed {
                        commands.spawn((fields, Managed, ChildOf(event.entity), Item));
                    } else {
                        commands.spawn((fields, ChildOf(event.entity), Item));
                    }
                }
            }
        }
    }
    for entity in existing.into_values() {
        commands.entity(entity).despawn();
    }
    catalog.loaded = event.snapshot.loaded;
    catalog.root.clone_from(&event.snapshot.root);
    catalog.providers.clone_from(&event.snapshot.providers);
    catalog.categories = categories;
    catalog.error.clone_from(&event.snapshot.error);
    catalog.revision = catalog.revision.wrapping_add(1);
}

type Changes<'w, 's> = Query<
    'w,
    's,
    &'static ChildOf,
    (
        With<Item>,
        Or<(
            Added<Item>,
            Changed<ToolProviderId>,
            Changed<Id>,
            Changed<Name>,
            Changed<RegistrationOrder>,
            Changed<Version>,
            Changed<Icon>,
            Changed<Detail>,
            Changed<Status>,
            Changed<Operations>,
            Changed<ChildOf>,
            Added<Managed>,
        )>,
    ),
>;

fn track(changes: Changes, mut catalogs: Query<&mut Catalog>) {
    let mut dirty = HashSet::new();
    for parent in &changes {
        dirty.insert(parent.parent());
    }
    for entity in dirty {
        if let Ok(mut catalog) = catalogs.get_mut(entity) {
            catalog.revision = catalog.revision.wrapping_add(1);
        }
    }
}

fn remove(trigger: On<Remove, Item>, parents: Query<&ChildOf>, mut catalogs: Query<&mut Catalog>) {
    let Ok(parent) = parents.get(trigger.event_target()) else {
        return;
    };
    if let Ok(mut catalog) = catalogs.get_mut(parent.parent()) {
        catalog.revision = catalog.revision.wrapping_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Resource, Default)]
    struct Published(Option<ToolsSnapshot>);

    struct FixturePlugin;

    impl bevy::prelude::Plugin for FixturePlugin {
        fn build(&self, app: &mut App) {
            app.add_plugins(Plugin)
                .init_resource::<Published>()
                .add_systems(PostUpdate, publish);
        }
    }

    fn publish(
        root: Single<Entity, With<Catalog>>,
        inventory: Inventory,
        mut published: ResMut<Published>,
    ) {
        published.0 = inventory.snapshot(*root);
    }

    struct Fixture {
        app: App,
        root: Entity,
    }

    impl Fixture {
        fn new() -> Self {
            let mut app = App::new();
            app.add_plugins(FixturePlugin);
            let root = app.world_mut().spawn(Catalog::default()).id();
            Self { app, root }
        }

        fn replace(&mut self, categories: Vec<ToolCategory>) -> ToolsSnapshot {
            self.app.world_mut().trigger(Received {
                entity: self.root,
                snapshot: ToolsSnapshot {
                    loaded: true,
                    categories,
                    ..Default::default()
                },
            });
            self.app.update();
            self.app.world().resource::<Published>().0.clone().unwrap()
        }

        fn entity(&mut self, provider: &ToolProvider, id: &str) -> Entity {
            for (entity, held_provider, held_id) in self
                .app
                .world_mut()
                .query_filtered::<(Entity, &ToolProviderId, &Id), With<Item>>()
                .iter(self.app.world())
            {
                if &held_provider.0 == provider && held_id.0 == id {
                    return entity;
                }
            }
            panic!("missing inventory item");
        }
    }

    #[test]
    fn refresh_preserves_identity_and_projects_order_status_and_removed_items() {
        let provider = ToolProvider::new("npm");
        let other = ToolProvider::new("brew");
        let first = ToolItem {
            provider: provider.clone(),
            id: "shared".into(),
            name: "First".into(),
            icon: None,
            version: Some("1".into()),
            detail: String::new(),
            status: ToolStatus::Installed,
            managed: true,
            operations: vec![ToolOperationKind::Update],
        };
        let second = ToolItem {
            id: "second".into(),
            name: "Second".into(),
            ..first.clone()
        };
        let external = ToolItem {
            provider: other.clone(),
            name: "Other provider".into(),
            ..first.clone()
        };
        let mut fixture = Fixture::new();
        fixture.replace(vec![
            ToolCategory {
                provider: provider.clone(),
                items: vec![first.clone(), second.clone()],
            },
            ToolCategory {
                provider: other.clone(),
                items: vec![external],
            },
        ]);
        let original = fixture.entity(&provider, "shared");
        let removed = fixture.entity(&provider, "second");
        let independent = fixture.entity(&other, "shared");
        let mut refreshed = first;
        refreshed.name = "Updated".into();
        refreshed.version = Some("2".into());
        refreshed.status = ToolStatus::Outdated;
        refreshed.managed = false;
        let reordered = fixture.replace(vec![
            ToolCategory {
                provider: provider.clone(),
                items: vec![second.clone(), refreshed.clone()],
            },
            ToolCategory {
                provider: other.clone(),
                items: Vec::new(),
            },
        ]);
        assert_eq!(
            reordered.categories[0].items,
            vec![second, refreshed.clone()]
        );
        assert_eq!(fixture.entity(&provider, "shared"), original);
        assert_eq!(fixture.entity(&provider, "second"), removed);
        let snapshot = fixture.replace(vec![
            ToolCategory {
                provider: provider.clone(),
                items: vec![refreshed.clone()],
            },
            ToolCategory {
                provider: other.clone(),
                items: Vec::new(),
            },
        ]);
        assert_eq!(fixture.entity(&provider, "shared"), original);
        assert!(fixture.app.world().get_entity(removed).is_err());
        assert!(fixture.app.world().get_entity(independent).is_err());
        assert_eq!(snapshot.categories[0].items, vec![refreshed]);
        assert!(snapshot.categories[1].items.is_empty());
        assert_eq!(snapshot.installed, 1);
        assert_eq!(snapshot.updates, 1);
        assert_eq!(snapshot.conflicts, 0);
        fixture
            .app
            .world_mut()
            .entity_mut(original)
            .insert(Status(ToolStatus::Conflict));
        fixture.app.update();
        let updated = fixture
            .app
            .world()
            .resource::<Published>()
            .0
            .as_ref()
            .unwrap();
        assert_eq!(updated.installed, 0);
        assert_eq!(updated.conflicts, 1);
        fixture.app.world_mut().entity_mut(fixture.root).despawn();
        assert!(fixture.app.world().get_entity(original).is_err());
    }
}
