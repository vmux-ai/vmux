use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use bevy_app::{App, Plugin};
use bevy_ecs::prelude::*;
use bevy_ecs::system::SystemParam;
use serde::{Deserialize, Serialize};
pub use vmux_ecs::store::{Phase, State};
use vmux_ecs::{OperationPlugin, Revision};

#[cfg(feature = "filesystem")]
pub mod filesystem;
mod lifecycle;

pub struct FoldPlugin;

impl Plugin for FoldPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<OperationPlugin>() {
            app.add_plugins(OperationPlugin);
        }
        app.add_plugins(lifecycle::Plugin);
    }
}

#[derive(Component, Clone, Debug, PartialEq, Eq)]
#[require(State)]
pub struct Root(pub PathBuf);

#[derive(Component)]
pub struct Loaded;

#[derive(Component)]
#[require(Revision)]
pub struct Record;

#[derive(Component, Clone, Debug, PartialEq, Eq, Hash)]
pub struct FileId(pub PathBuf);

#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct Collapsed(pub BTreeSet<u32>);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ticket {
    pub owner: Entity,
    pub path: PathBuf,
    pub generation: u64,
}

#[derive(Component)]
pub(crate) struct Scope(pub Ticket);

#[derive(Message, Clone)]
pub struct UpdateRequest {
    pub ticket: Ticket,
    pub file: FileId,
    pub mutation: Mutation,
}

#[derive(Clone)]
pub enum Mutation {
    Replace(Collapsed),
    Change {
        collapse: BTreeSet<u32>,
        expand: BTreeSet<u32>,
    },
}

impl Collapsed {
    fn apply(&mut self, mutation: &Mutation) {
        match mutation {
            Mutation::Replace(collapsed) => self.0.clone_from(&collapsed.0),
            Mutation::Change { collapse, expand } => {
                for line in expand {
                    self.0.remove(line);
                }
                self.0.extend(collapse);
            }
        }
    }
}

#[derive(Message, Clone)]
pub struct RetryRequest(pub Ticket);

#[derive(Message, Clone)]
pub struct LoadRequest(pub Ticket);

#[derive(Message)]
pub struct LoadCompleted {
    pub ticket: Ticket,
    pub result: Result<Snapshot, String>,
}

#[derive(Message, Clone)]
pub struct SaveRequest {
    pub ticket: Ticket,
    pub operation: Entity,
    pub revision: u64,
    pub snapshot: Snapshot,
}

#[derive(Message)]
pub struct SaveCompleted {
    pub ticket: Ticket,
    pub operation: Entity,
    pub revision: u64,
    pub result: Result<(), String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Snapshot {
    pub files: BTreeMap<String, Vec<u32>>,
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FoldSet {
    Restore,
    Mutate,
    Save,
    Project,
}

#[derive(SystemParam)]
pub struct Preferences<'w, 's> {
    roots: Query<'w, 's, (&'static Root, &'static Scope, Option<&'static Loaded>)>,
    records: Query<
        'w,
        's,
        (
            Entity,
            &'static FileId,
            &'static Collapsed,
            &'static Revision,
            &'static ChildOf,
        ),
        With<Record>,
    >,
}

impl Preferences<'_, '_> {
    pub fn ticket(&self, root: Entity) -> Option<&Ticket> {
        let (path, scope, _) = self.roots.get(root).ok()?;
        if path.0 != scope.0.path {
            return None;
        }
        Some(&scope.0)
    }

    pub fn ready(&self, ticket: &Ticket) -> bool {
        self.roots
            .get(ticket.owner)
            .is_ok_and(|(path, scope, loaded)| {
                path.0 == ticket.path && scope.0 == *ticket && loaded.is_some()
            })
    }

    pub fn get(&self, ticket: &Ticket, file: &Path) -> Option<(Entity, &Collapsed, &Revision)> {
        if !self.ready(ticket) {
            return None;
        }
        let mut found = None;
        for (entity, id, collapsed, revision, parent) in &self.records {
            if parent.parent() == ticket.owner && id.0 == file {
                if found.is_some() {
                    return None;
                }
                found = Some((entity, collapsed, revision));
            }
        }
        found
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_ecs::system::SystemState;
    use vmux_ecs::OperationState;

    struct Fixture {
        app: App,
        root: Entity,
        ticket: Ticket,
    }

    impl Fixture {
        fn new() -> Self {
            let mut app = App::new();
            app.add_plugins(FoldPlugin);
            let root = app.world_mut().spawn(Root("/store/folds.ron".into())).id();
            app.update();
            let ticket = app
                .world_mut()
                .resource_mut::<Messages<LoadRequest>>()
                .drain()
                .next()
                .unwrap()
                .0;
            app.world_mut().write_message(LoadCompleted {
                ticket: ticket.clone(),
                result: Ok(Snapshot::default()),
            });
            app.update();
            Self { app, root, ticket }
        }

        fn change(&mut self, lines: &[u32]) -> SaveRequest {
            self.app.world_mut().write_message(UpdateRequest {
                ticket: self.ticket.clone(),
                file: FileId("/files/shared.rs".into()),
                mutation: Mutation::Replace(Collapsed(lines.iter().copied().collect())),
            });
            self.app.update();
            self.take_save()
        }

        fn take_save(&mut self) -> SaveRequest {
            self.app
                .world_mut()
                .resource_mut::<Messages<SaveRequest>>()
                .drain()
                .next()
                .unwrap()
        }

        fn finish(&mut self, request: &SaveRequest, result: Result<(), String>) {
            self.app.world_mut().write_message(SaveCompleted {
                ticket: request.ticket.clone(),
                operation: request.operation,
                revision: request.revision,
                result,
            });
            self.app.update();
        }
    }

    #[test]
    fn failed_saves_retain_dirty_state_and_wait_for_explicit_retry() {
        let mut fixture = Fixture::new();
        let request = fixture.change(&[7, 2]);
        fixture.finish(&request, Err("disk full".into()));
        let state = fixture.app.world().get::<State>(fixture.root).unwrap();
        assert_eq!(state.dirty(), Some(request.revision));
        assert_eq!(state.error(), Some("disk full"));
        fixture.app.update();
        assert!(
            fixture
                .app
                .world_mut()
                .resource_mut::<Messages<SaveRequest>>()
                .drain()
                .next()
                .is_none()
        );
        fixture
            .app
            .world_mut()
            .write_message(RetryRequest(fixture.ticket.clone()));
        fixture.app.update();
        let retry = fixture.take_save();
        assert_eq!(retry.snapshot.files["/files/shared.rs"], [2, 7]);
        fixture.finish(&retry, Ok(()));
        let state = fixture.app.world().get::<State>(fixture.root).unwrap();
        assert!(!state.is_dirty());
        assert!(state.error().is_none());
        assert_eq!(
            fixture.app.world().get::<OperationState>(retry.operation),
            Some(&OperationState::Succeeded)
        );
    }

    #[test]
    fn older_save_success_cannot_clear_newer_fold_changes() {
        let mut fixture = Fixture::new();
        let first = fixture.change(&[2]);
        fixture.app.world_mut().write_message(UpdateRequest {
            ticket: fixture.ticket.clone(),
            file: FileId("/files/shared.rs".into()),
            mutation: Mutation::Replace(Collapsed([9].into())),
        });
        fixture.app.update();
        fixture.finish(&first, Ok(()));
        let second = fixture.take_save();
        assert!(second.revision > first.revision);
        assert_eq!(second.snapshot.files["/files/shared.rs"], [9]);
        fixture.finish(&first, Ok(()));
        assert_eq!(
            fixture
                .app
                .world()
                .get::<State>(fixture.root)
                .unwrap()
                .saving(),
            Some(second.revision)
        );
        fixture.finish(&second, Ok(()));
        assert_eq!(
            fixture
                .app
                .world()
                .get::<State>(fixture.root)
                .unwrap()
                .loaded(),
            Some(second.revision)
        );
    }

    #[test]
    fn roots_isolate_file_identity_and_view_deltas_compose() {
        let mut fixture = Fixture::new();
        let other = fixture
            .app
            .world_mut()
            .spawn(Root("/other/folds.ron".into()))
            .id();
        fixture.app.update();
        let ticket = fixture
            .app
            .world_mut()
            .resource_mut::<Messages<LoadRequest>>()
            .drain()
            .next()
            .unwrap()
            .0;
        fixture.app.world_mut().write_message(LoadCompleted {
            ticket: ticket.clone(),
            result: Ok(Snapshot::default()),
        });
        fixture.app.update();
        for line in [2, 7] {
            fixture.app.world_mut().write_message(UpdateRequest {
                ticket: fixture.ticket.clone(),
                file: FileId("/files/shared.rs".into()),
                mutation: Mutation::Change {
                    collapse: [line].into(),
                    expand: Default::default(),
                },
            });
        }
        fixture.app.world_mut().write_message(UpdateRequest {
            ticket: ticket.clone(),
            file: FileId("/files/shared.rs".into()),
            mutation: Mutation::Replace(Collapsed([99].into())),
        });
        fixture.app.update();
        let mut state = SystemState::<Preferences>::new(fixture.app.world_mut());
        let preferences = state.get(fixture.app.world()).unwrap();
        assert_eq!(
            preferences
                .get(&fixture.ticket, Path::new("/files/shared.rs"))
                .unwrap()
                .1
                .0,
            [2, 7].into()
        );
        assert_eq!(
            preferences
                .get(&ticket, Path::new("/files/shared.rs"))
                .unwrap()
                .1
                .0,
            [99].into()
        );
        assert_ne!(other, fixture.root);
    }

    #[test]
    fn owner_removal_cancels_the_operation_and_refuses_its_late_result() {
        let mut fixture = Fixture::new();
        let request = fixture.change(&[2]);
        fixture.app.world_mut().despawn(fixture.root);
        let replacement = fixture
            .app
            .world_mut()
            .spawn(Root(request.ticket.path.clone()))
            .id();
        fixture.app.update();
        assert_eq!(
            fixture.app.world().get::<OperationState>(request.operation),
            Some(&OperationState::Canceled)
        );
        fixture.finish(&request, Ok(()));
        assert_eq!(
            fixture
                .app
                .world()
                .get::<State>(replacement)
                .unwrap()
                .loaded(),
            None
        );
        assert!(fixture.app.world().get::<Loaded>(replacement).is_none());
    }

    #[test]
    fn replacing_a_root_invalidates_mutations_and_load_results_from_its_old_generation() {
        let mut fixture = Fixture::new();
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.root)
            .insert(Root(fixture.ticket.path.clone()));
        fixture.app.world_mut().write_message(UpdateRequest {
            ticket: fixture.ticket.clone(),
            file: FileId("/files/shared.rs".into()),
            mutation: Mutation::Replace(Collapsed([2].into())),
        });
        fixture.app.update();
        let next = fixture
            .app
            .world_mut()
            .resource_mut::<Messages<LoadRequest>>()
            .drain()
            .next()
            .unwrap()
            .0;
        assert_ne!(next.generation, fixture.ticket.generation);
        fixture.app.world_mut().write_message(LoadCompleted {
            ticket: fixture.ticket.clone(),
            result: Ok(Snapshot {
                files: [("/files/shared.rs".into(), vec![2])].into(),
            }),
        });
        fixture.app.update();
        assert!(fixture.app.world().get::<Loaded>(fixture.root).is_none());
        fixture.app.world_mut().write_message(LoadCompleted {
            ticket: next,
            result: Ok(Snapshot::default()),
        });
        fixture.app.update();
        assert_eq!(
            fixture
                .app
                .world_mut()
                .query_filtered::<Entity, With<Record>>()
                .iter(fixture.app.world())
                .count(),
            0
        );
    }

    #[test]
    fn unreadable_store_stays_failed_and_cannot_be_overwritten_by_an_update() {
        let mut fixture = Fixture::new();
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.root)
            .insert(Root("/bad/folds.ron".into()));
        fixture.app.update();
        let ticket = fixture
            .app
            .world_mut()
            .resource_mut::<Messages<LoadRequest>>()
            .drain()
            .next()
            .unwrap()
            .0;
        fixture.app.world_mut().write_message(LoadCompleted {
            ticket: ticket.clone(),
            result: Err("invalid RON".into()),
        });
        fixture.app.world_mut().write_message(UpdateRequest {
            ticket: ticket.clone(),
            file: FileId("/files/shared.rs".into()),
            mutation: Mutation::Replace(Collapsed([2].into())),
        });
        fixture.app.update();
        assert_eq!(
            fixture
                .app
                .world()
                .get::<State>(fixture.root)
                .unwrap()
                .phase(),
            Phase::Failed
        );
        assert!(
            fixture
                .app
                .world_mut()
                .resource_mut::<Messages<SaveRequest>>()
                .drain()
                .next()
                .is_none()
        );
        fixture.app.world_mut().write_message(RetryRequest(ticket));
        fixture.app.update();
        assert!(
            fixture
                .app
                .world_mut()
                .resource_mut::<Messages<LoadRequest>>()
                .drain()
                .next()
                .is_some()
        );
    }
}
