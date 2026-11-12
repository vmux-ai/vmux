use std::collections::{HashMap, HashSet};

use bevy::prelude::*;

use super::bridge::{BridgeAuthorization, ExtensionBridgeServer};
use super::broker::{
    BridgeResponseCache, BridgeSubscriptions, ConformanceWakeTimer, PendingBridgeEvents,
};
use super::load::PreparedExtensions;
use super::{Binding, RuntimeBinding, RuntimeGeneration};
use crate::{Catalog, ExtensionSet, ProfileId};

pub(super) struct Plugin;

impl bevy::prelude::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, project.in_set(AccessSet).after(ExtensionSet::Step));
    }
}

#[derive(SystemSet, Clone, Debug, PartialEq, Eq, Hash)]
pub(super) struct AccessSet;

fn project(
    root: Query<(Entity, &ProfileId, &RuntimeGeneration), With<Binding>>,
    prepared: Query<&PreparedExtensions>,
    bindings: Query<(&crate::Id, &ChildOf), With<RuntimeBinding>>,
    catalog: Catalog,
    mut servers: Query<&mut ExtensionBridgeServer>,
    mut brokers: Query<(
        &mut BridgeSubscriptions,
        &mut BridgeResponseCache,
        &mut PendingBridgeEvents,
        Option<&mut ConformanceWakeTimer>,
    )>,
) {
    let mut projected = HashMap::new();
    if let Ok((root, profile, generation)) = root.single()
        && let Some(ticket) = catalog.ticket(root)
        && ticket.generation == generation.0
        && let Ok(snapshot) = catalog.snapshot(root)
        && let Ok(prepared) = prepared.single()
    {
        for runtime in &prepared.0 {
            let Some(association) = catalog.association(root, &runtime.extension_id, &profile.0)
            else {
                continue;
            };
            if !bindings
                .iter()
                .any(|(id, parent)| id.0 == runtime.extension_id && parent.parent() == association)
            {
                continue;
            }
            let Some(entry) = snapshot
                .entries
                .iter()
                .find(|entry| entry.id == runtime.extension_id)
            else {
                continue;
            };
            if !entry.enabled_for(&profile.0) || entry.source_hash != runtime.source_hash {
                continue;
            }
            let grants = entry.grants_for(&profile.0);
            let mut permissions = HashSet::new();
            for permission in &grants.permissions {
                if runtime.granted_permissions.contains(permission) {
                    permissions.insert(permission.clone());
                }
            }
            let mut host_permissions = Vec::new();
            let mut valid = true;
            for permission in &grants.host_permissions {
                if !runtime.granted_host_permissions.contains(permission) {
                    continue;
                }
                match crate::match_pattern::ExtensionMatchPattern::parse(permission) {
                    Ok(permission) => host_permissions.push(permission),
                    Err(_) => {
                        valid = false;
                        break;
                    }
                }
            }
            if !valid {
                continue;
            }
            projected.insert(
                entry.id.clone(),
                BridgeAuthorization {
                    permissions,
                    host_permissions,
                    conformance: false,
                },
            );
        }
    }
    for mut server in &mut servers {
        let changed = server.project_authorizations(projected.clone());
        for (mut subscriptions, mut cache, mut pending, mut timer) in &mut brokers {
            for id in &changed {
                subscriptions.0.remove(id);
                cache.revoke(id);
                pending.revoke(id);
                if let Some(timer) = &mut timer {
                    timer.revoke(id)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::runtime::PreparedRuntime;
    use super::*;
    use crate::snapshot::{ExtEntry, ExtensionGrants, Index};
    use crate::{
        CorePlugin, Mutation, Request, Root, Seed, Ticket, WriteCompleted, WriteTaskRequest,
    };
    use bevy::ecs::system::SystemState;
    use vmux_api::extension::{ExtToggleRequest, ExtUninstallRequest};

    const ID: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    struct Fixture {
        app: App,
        root: Entity,
        server: Entity,
        ticket: Ticket,
    }

    impl Fixture {
        fn new() -> Self {
            let mut app = App::new();
            app.add_plugins((CorePlugin, Plugin));
            let mut entry = ExtEntry {
                id: ID.into(),
                name: "Example".into(),
                version: "1".into(),
                source_hash: "source-hash".into(),
                permissions: vec!["storage".into()],
                optional_permissions: vec!["history".into()],
                ..Default::default()
            };
            for profile in ["personal", "work"] {
                entry.profile_enabled.insert(profile.into(), true);
                entry.approved_grants.insert(
                    profile.into(),
                    ExtensionGrants {
                        permissions: vec!["storage".into(), "history".into()],
                        host_permissions: Vec::new(),
                    },
                );
            }
            let mut index = Index::default();
            index.entries.push(entry);
            let root = app
                .world_mut()
                .spawn((
                    Binding,
                    Root("/extensions".into()),
                    Seed(Ok(index)),
                    ProfileId("personal".into()),
                ))
                .id();
            app.update();
            let mut state = SystemState::<Catalog>::new(app.world_mut());
            let ticket = state
                .get(app.world())
                .unwrap()
                .ticket(root)
                .unwrap()
                .clone();
            app.world_mut()
                .entity_mut(root)
                .insert(RuntimeGeneration(ticket.generation));
            let association = state
                .get(app.world())
                .unwrap()
                .association(root, ID, "personal")
                .unwrap();
            app.world_mut()
                .spawn((RuntimeBinding, crate::Id(ID.into()), ChildOf(association)));
            let mut runtime = PreparedRuntime::fixture(ID, "runtime");
            runtime.granted_permissions = vec!["storage".into(), "history".into()];
            app.world_mut().spawn(PreparedExtensions(vec![runtime]));
            let server = app
                .world_mut()
                .spawn(ExtensionBridgeServer::start("personal", [ID]).unwrap())
                .id();
            app.update();
            Self {
                app,
                root,
                server,
                ticket,
            }
        }

        fn authorization(&self) -> Option<&BridgeAuthorization> {
            self.app
                .world()
                .get::<ExtensionBridgeServer>(self.server)
                .unwrap()
                .authorization(ID)
        }

        fn apply(&mut self, mutation: Mutation) {
            self.app.world_mut().spawn(
                Request {
                    ticket: self.ticket.clone(),
                    profile: "personal".into(),
                    mutation,
                }
                .bundle(),
            );
            self.app.update();
            let write = self
                .app
                .world_mut()
                .resource_mut::<Messages<WriteTaskRequest>>()
                .drain()
                .next()
                .unwrap();
            self.app.world_mut().write_message(WriteCompleted {
                ticket: write.ticket,
                operation: write.operation,
                revision: write.revision,
                result: Ok(()),
            });
            self.app.update();
        }
    }

    #[test]
    fn current_profile_grants_and_removal_restrict_an_already_loaded_bridge() {
        let mut fixture = Fixture::new();
        assert!(
            fixture
                .authorization()
                .unwrap()
                .permissions
                .contains("history")
        );
        fixture.apply(Mutation::Grant {
            id: ID.into(),
            grants: ExtensionGrants {
                permissions: vec!["storage".into()],
                host_permissions: Vec::new(),
            },
        });
        assert!(
            fixture
                .authorization()
                .unwrap()
                .permissions
                .contains("storage")
        );
        assert!(
            !fixture
                .authorization()
                .unwrap()
                .permissions
                .contains("history")
        );
        fixture.apply(Mutation::Remove(ExtUninstallRequest { id: ID.into() }));
        assert!(fixture.authorization().is_none());
        let mut state = SystemState::<Catalog>::new(fixture.app.world_mut());
        assert!(
            state
                .get(fixture.app.world())
                .unwrap()
                .snapshot(fixture.root)
                .unwrap()
                .entries[0]
                .installed_for("work")
        );
        let entry = state
            .get(fixture.app.world())
            .unwrap()
            .snapshot(fixture.root)
            .unwrap()
            .entries
            .remove(0);
        fixture.apply(Mutation::Install(Box::new(entry)));
        fixture.apply(Mutation::Toggle(ExtToggleRequest {
            id: ID.into(),
            enabled: true,
            approve_permissions: true,
        }));
        assert!(fixture.authorization().is_none());
    }

    #[test]
    fn replacement_store_generation_cannot_authorize_an_old_prepared_runtime() {
        let mut fixture = Fixture::new();
        assert!(fixture.authorization().is_some());
        let mut state = SystemState::<Catalog>::new(fixture.app.world_mut());
        let snapshot = state
            .get(fixture.app.world())
            .unwrap()
            .snapshot(fixture.root)
            .unwrap();
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.root)
            .insert(Seed(Ok(snapshot)));
        fixture.app.update();
        assert!(fixture.authorization().is_none());
    }
}
