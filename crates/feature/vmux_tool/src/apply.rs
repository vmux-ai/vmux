use bevy::prelude::*;
use vmux_ecs::{Operation, OperationRetained, OperationState, OperationTarget, Order};

use crate::catalog::Inventory;
use crate::dotfiles::{AppliedEnabledDotfiles, ApplyEnabledDotfiles};
use crate::observation::PackageProvider;
use crate::state::{ToolApplyRequest, ToolInstallRequest, ToolStatus};
use crate::tool_inventory::Registry;
use crate::{
    ExternalToolOperation, ToolOperationFailed, ToolOperationFinished, ToolOperationRequest,
    ToolOperationRouteSet, ToolOperationSucceeded, ToolProviderId, ToolProviderTarget, ToolStore,
    ToolStoreOperation, ToolStoreTarget, ToolTaskSet,
};

pub(crate) struct ApplyPlugin;

impl Plugin for ApplyPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, route.in_set(ToolOperationRouteSet))
            .add_systems(
                Update,
                (cancel, next, collect, link, finish)
                    .chain()
                    .after(ToolTaskSet::Apply),
            )
            .add_systems(PostUpdate, release.after(vmux_job::JobSet::Cleanup));
    }
}

#[derive(Component)]
struct Apply {
    store: Entity,
    captured: ToolStore,
    release: bool,
    installed: usize,
    linked: usize,
}

#[derive(Component)]
struct Install;

#[derive(Component)]
struct Link;

#[derive(Component)]
struct Started;

#[derive(Component)]
struct Counted;

#[derive(Component)]
struct Linking;

#[derive(Component)]
struct Finished;

fn route(
    requests: Query<
        (
            Entity,
            &ToolStoreTarget,
            &OperationState,
            Has<OperationRetained>,
        ),
        (
            With<ToolOperationRequest<ToolApplyRequest>>,
            Without<Apply>,
            Without<ToolOperationFinished>,
        ),
    >,
    registries: Query<Entity, With<Registry>>,
    stores: Query<&ToolStore>,
    providers: Query<(Entity, &ToolProviderId), With<PackageProvider>>,
    inventory: Inventory,
    wake: Option<Res<vmux_ecs::JobWake>>,
    mut commands: Commands,
) {
    let Ok(registry) = registries.single() else {
        return;
    };
    let Some(snapshot) = inventory.snapshot(registry) else {
        return;
    };
    if !snapshot.loaded {
        return;
    }
    for (entity, target, state, retained) in &requests {
        if !matches!(state, OperationState::Pending | OperationState::Running) {
            continue;
        }
        let Ok(store) = stores.get(target.entity()) else {
            continue;
        };
        let mut order = 0;
        for category in &snapshot.categories {
            for item in &category.items {
                if !item.managed || item.status != ToolStatus::Missing {
                    continue;
                }
                if !providers
                    .iter()
                    .any(|(_, provider)| provider.0 == item.provider)
                {
                    continue;
                }
                commands.spawn((
                    Name::new(format!("Install {}", item.id)),
                    Operation::bundle("tools.package.install").unwrap(),
                    Install,
                    Order(order),
                    ChildOf(entity),
                    OperationRetained,
                    ToolOperationRequest(ToolInstallRequest {
                        provider: item.provider.clone(),
                        id: item.id.clone(),
                    }),
                ));
                order += 1;
            }
        }
        commands
            .entity(entity)
            .remove::<ExternalToolOperation>()
            .insert((
                ToolStoreOperation,
                OperationRetained,
                OperationState::Running,
                Apply {
                    store: target.entity(),
                    captured: store.clone(),
                    release: !retained,
                    installed: 0,
                    linked: 0,
                },
            ));
        if let Some(wake) = &wake {
            wake.0.wake_by_ref();
        }
    }
}

fn cancel(
    parents: Query<(&Apply, &OperationState)>,
    stores: Query<&ToolStore>,
    mut children: Query<
        (&ChildOf, &mut OperationState),
        (
            Or<(With<Install>, With<Link>)>,
            Without<Counted>,
            Without<Apply>,
        ),
    >,
) {
    for (parent, mut state) in &mut children {
        let valid = parents.get(parent.parent()).is_ok_and(|(apply, state)| {
            matches!(state, OperationState::Pending | OperationState::Running)
                && stores
                    .get(apply.store)
                    .is_ok_and(|current| *current == apply.captured)
        });
        if !valid && matches!(*state, OperationState::Pending | OperationState::Running) {
            *state = OperationState::Canceled;
        }
    }
}

fn next(
    parents: Query<(Entity, &Apply, &OperationState), Without<Finished>>,
    children: Query<
        (
            Entity,
            &ChildOf,
            &Order,
            &ToolOperationRequest<ToolInstallRequest>,
            Has<Started>,
            Has<Counted>,
        ),
        With<Install>,
    >,
    providers: Query<(Entity, &ToolProviderId)>,
    wake: Option<Res<vmux_ecs::JobWake>>,
    mut commands: Commands,
) {
    for (parent, apply, state) in &parents {
        if *state != OperationState::Running {
            continue;
        }
        let mut candidate = None;
        let mut running = false;
        for (entity, owner, order, request, started, counted) in &children {
            if owner.parent() != parent || counted {
                continue;
            }
            if started {
                running = true;
                break;
            }
            if candidate
                .as_ref()
                .is_none_or(|(_, current, _)| order.0 < *current)
            {
                candidate = Some((entity, order.0, request));
            }
        }
        if running {
            continue;
        }
        let Some((entity, _, request)) = candidate else {
            continue;
        };
        let provider = providers
            .iter()
            .find(|(_, provider)| provider.0 == request.0.provider)
            .map(|(entity, _)| entity);
        let Some(provider) = provider else {
            commands.entity(entity).insert((
                Started,
                ToolOperationFailed("tool provider is unavailable".into()),
                ToolOperationFinished,
            ));
            continue;
        };
        commands.entity(entity).insert((
            Started,
            ToolStoreTarget::new(apply.store),
            ToolProviderTarget::new(provider),
        ));
        if let Some(wake) = &wake {
            wake.0.wake_by_ref();
        }
    }
}

fn collect(
    children: Query<
        (
            Entity,
            &ChildOf,
            &OperationState,
            Option<&ToolOperationSucceeded>,
            Option<&ToolOperationFailed>,
            Option<&AppliedEnabledDotfiles>,
            Has<Install>,
            Has<ToolOperationFinished>,
        ),
        (Or<(With<Install>, With<Link>)>, Without<Counted>),
    >,
    mut parents: Query<(Entity, &mut Apply, &OperationState)>,
    mut commands: Commands,
) {
    for (entity, parent, state, success, failure, linked, install, finished) in &children {
        let Ok((owner, mut apply, parent_state)) = parents.get_mut(parent.parent()) else {
            continue;
        };
        if !finished && linked.is_none() && *state != OperationState::Canceled {
            continue;
        }
        commands.entity(entity).insert(Counted);
        if let Some(linked) = linked {
            apply.linked += linked.files;
            commands
                .entity(entity)
                .insert((ToolOperationFinished, OperationState::Succeeded));
        } else if success.is_some() && install {
            apply.installed += 1;
        }
        if *parent_state != OperationState::Running {
            continue;
        }
        let error = failure.map(|failure| failure.0.clone()).or_else(|| {
            (*state == OperationState::Canceled).then(|| "tool apply operation canceled".into())
        });
        if let Some(error) = error {
            commands.entity(owner).insert((
                OperationState::Failed,
                Finished,
                ToolOperationFailed(error),
                ToolOperationFinished,
            ));
        }
    }
}

fn link(
    parents: Query<(Entity, &Apply, &OperationState), (Without<Linking>, Without<Finished>)>,
    installs: Query<(&ChildOf, Has<Counted>), With<Install>>,
    wake: Option<Res<vmux_ecs::JobWake>>,
    mut commands: Commands,
) {
    for (entity, apply, state) in &parents {
        if *state != OperationState::Running
            || installs
                .iter()
                .any(|(parent, counted)| parent.parent() == entity && !counted)
        {
            continue;
        }
        commands.entity(entity).insert(Linking);
        commands.spawn((
            Name::new("Apply enabled dotfiles"),
            Link,
            ChildOf(entity),
            OperationRetained,
            Operation::bundle("tools.dotfiles.apply").unwrap(),
            ToolStoreTarget::new(apply.store),
            OperationTarget::new(apply.store),
            ApplyEnabledDotfiles,
        ));
        if let Some(wake) = &wake {
            wake.0.wake_by_ref();
        }
    }
}

fn finish(
    parents: Query<(Entity, &Apply, &OperationState), (With<Linking>, Without<Finished>)>,
    children: Query<(&ChildOf, Has<Counted>), Or<(With<Install>, With<Link>)>>,
    mut commands: Commands,
) {
    for (entity, apply, state) in &parents {
        if *state != OperationState::Running
            || children
                .iter()
                .any(|(parent, counted)| parent.parent() == entity && !counted)
        {
            continue;
        }
        commands.entity(entity).insert((
            OperationState::Succeeded,
            Finished,
            ToolOperationSucceeded(format!(
                "installed {} package(s), linked {} file(s)",
                apply.installed, apply.linked
            )),
            ToolOperationFinished,
        ));
    }
}

fn release(
    parents: Query<(Entity, &Apply, &OperationState, Option<&Children>)>,
    native: Query<&OperationTarget, With<vmux_job::JobCapture>>,
    hierarchy: Query<(&ChildOf, Option<&OperationTarget>)>,
    targets: Query<&OperationTarget>,
    stores: Query<&ToolStore>,
    wake: Option<Res<vmux_ecs::JobWake>>,
    mut commands: Commands,
) {
    for (entity, apply, state, children) in &parents {
        if matches!(state, OperationState::Pending | OperationState::Running) {
            continue;
        }
        if stores.get(apply.store).is_ok_and(|store| store.is_busy()) {
            continue;
        }
        let pending = native.iter().any(|target| {
            let mut current = target.entity();
            let mut visited = std::collections::HashSet::new();
            while visited.insert(current) {
                if current == entity {
                    return true;
                }
                if let Ok((parent, _)) = hierarchy.get(current) {
                    current = parent.parent();
                } else if let Ok(target) = targets.get(current) {
                    current = target.entity();
                } else {
                    break;
                }
            }
            false
        });
        if pending {
            continue;
        }
        if let Some(children) = children {
            for child in children.iter() {
                commands.entity(child).despawn();
            }
        }
        let mut parent = commands.entity(entity);
        parent.remove::<Apply>();
        if apply.release {
            parent.remove::<OperationRetained>();
        }
        if let Some(wake) = &wake {
            wake.0.wake_by_ref();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{Catalog, CatalogPlugin, Received};
    use crate::state::{ToolCategory, ToolItem, ToolProvider, ToolsSnapshot};
    use bevy::prelude::{
        Add, App, Commands, Entity, On, Plugin, Query, ResMut, Resource, Update, With, Without,
    };

    #[derive(Resource, Default)]
    struct Effects {
        installs: Vec<String>,
        permitted: usize,
        links: usize,
    }

    struct FulfillmentPlugin;

    impl Plugin for FulfillmentPlugin {
        fn build(&self, app: &mut App) {
            app.add_observer(observe).add_systems(Update, fulfill);
        }
    }

    fn observe(
        trigger: On<Add, ToolStoreTarget>,
        requests: Query<&ToolOperationRequest<ToolInstallRequest>>,
        mut effects: ResMut<Effects>,
    ) {
        if let Ok(request) = requests.get(trigger.event_target()) {
            effects
                .installs
                .push(format!("{}:{}", request.0.provider.id(), request.0.id));
        }
    }

    fn fulfill(
        installs: Query<
            (
                Entity,
                &ToolOperationRequest<ToolInstallRequest>,
                &OperationState,
            ),
            (With<ToolStoreTarget>, Without<ToolOperationFinished>),
        >,
        links: Query<Entity, (With<ApplyEnabledDotfiles>, Without<AppliedEnabledDotfiles>)>,
        mut effects: ResMut<Effects>,
        mut commands: Commands,
    ) {
        for (entity, request, state) in &installs {
            if *state == OperationState::Canceled {
                continue;
            }
            let key = format!("{}:{}", request.0.provider.id(), request.0.id);
            let permitted = effects
                .installs
                .iter()
                .take(effects.permitted)
                .any(|item| *item == key);
            if permitted {
                commands.entity(entity).insert((
                    OperationState::Succeeded,
                    ToolOperationSucceeded("installed".into()),
                    ToolOperationFinished,
                ));
            }
        }
        for entity in &links {
            effects.links += 1;
            commands
                .entity(entity)
                .insert(AppliedEnabledDotfiles { files: 2 });
        }
    }

    struct Fixture {
        app: App,
        operation: Entity,
        _directory: tempfile::TempDir,
    }

    impl Fixture {
        fn new() -> Self {
            let mut app = App::new();
            app.init_resource::<Effects>().add_plugins((
                vmux_ecs::OperationPlugin,
                CatalogPlugin,
                ApplyPlugin,
                FulfillmentPlugin,
            ));
            let directory = tempfile::tempdir().unwrap();
            let store = app
                .world_mut()
                .spawn((
                    Registry::default(),
                    Catalog::default(),
                    ToolStore::new(
                        directory.path().join("config"),
                        directory.path().join("home"),
                    ),
                ))
                .id();
            let npm = ToolProvider::new("npm");
            let acp = ToolProvider::new("acp");
            app.world_mut()
                .spawn((ToolProviderId(npm.clone()), PackageProvider));
            app.world_mut()
                .spawn((ToolProviderId(acp.clone()), PackageProvider));
            app.world_mut().trigger(Received {
                entity: store,
                snapshot: ToolsSnapshot {
                    loaded: true,
                    categories: vec![
                        ToolCategory {
                            provider: npm.clone(),
                            items: vec![
                                ToolItem {
                                    provider: npm.clone(),
                                    id: "first".into(),
                                    name: "first".into(),
                                    icon: None,
                                    version: None,
                                    detail: String::new(),
                                    status: ToolStatus::Missing,
                                    managed: true,
                                    operations: Vec::new(),
                                },
                                ToolItem {
                                    provider: npm.clone(),
                                    id: "unmanaged".into(),
                                    name: "unmanaged".into(),
                                    icon: None,
                                    version: None,
                                    detail: String::new(),
                                    status: ToolStatus::Missing,
                                    managed: false,
                                    operations: Vec::new(),
                                },
                            ],
                        },
                        ToolCategory {
                            provider: acp.clone(),
                            items: vec![ToolItem {
                                provider: acp.clone(),
                                id: "second".into(),
                                name: "second".into(),
                                icon: None,
                                version: None,
                                detail: String::new(),
                                status: ToolStatus::Missing,
                                managed: true,
                                operations: Vec::new(),
                            }],
                        },
                    ],
                    ..Default::default()
                },
            });
            app.world_mut().flush();
            let operation = app
                .world_mut()
                .spawn((
                    Operation::bundle("test.tools.apply").unwrap(),
                    ToolStoreTarget::new(store),
                    ToolOperationRequest(ToolApplyRequest { provider: npm }),
                ))
                .id();
            Self {
                app,
                operation,
                _directory: directory,
            }
        }

        fn until(&mut self, predicate: impl Fn(&App) -> bool) {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
            while !predicate(&self.app) {
                self.app.update();
                assert!(
                    std::time::Instant::now() < deadline,
                    "apply did not progress"
                );
            }
        }
    }

    #[test]
    fn apply_routes_package_requests_without_operator_callbacks_and_waits_between_steps() {
        let mut fixture = Fixture::new();
        fixture.until(|app| app.world().resource::<Effects>().installs.len() == 1);
        for _ in 0..3 {
            fixture.app.update();
        }
        assert_eq!(
            fixture.app.world().resource::<Effects>().installs,
            vec!["npm:first"]
        );
        assert_eq!(fixture.app.world().resource::<Effects>().links, 0);
        fixture.app.world_mut().resource_mut::<Effects>().permitted = 1;
        fixture.until(|app| app.world().resource::<Effects>().installs.len() == 2);
        assert_eq!(
            fixture.app.world().resource::<Effects>().installs,
            vec!["npm:first", "acp:second"]
        );
        assert_eq!(fixture.app.world().resource::<Effects>().links, 0);
        fixture.app.world_mut().resource_mut::<Effects>().permitted = 2;
        let operation = fixture.operation;
        fixture.until(|app| {
            app.world()
                .get::<ToolOperationFinished>(operation)
                .is_some()
        });
        assert_eq!(fixture.app.world().resource::<Effects>().links, 1);
        assert!(
            fixture
                .app
                .world()
                .get::<ToolOperationSucceeded>(fixture.operation)
                .is_some()
        );
    }

    #[test]
    fn canceled_apply_does_not_start_queued_installs_or_dotfile_work() {
        let mut fixture = Fixture::new();
        fixture.until(|app| app.world().resource::<Effects>().installs.len() == 1);
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.operation)
            .insert(OperationState::Canceled);
        fixture.app.update();
        fixture.app.world_mut().resource_mut::<Effects>().permitted = 2;
        for _ in 0..5 {
            fixture.app.update();
        }
        assert_eq!(
            fixture.app.world().resource::<Effects>().installs,
            vec!["npm:first"]
        );
        assert_eq!(fixture.app.world().resource::<Effects>().links, 0);
        assert!(
            fixture
                .app
                .world()
                .get::<ToolOperationSucceeded>(fixture.operation)
                .is_none()
        );
    }
}
