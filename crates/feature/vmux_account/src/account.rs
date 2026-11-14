use crate::{Account, Owner, Profile};
use bevy_app::{App, Update};
use bevy_ecs::prelude::*;
use vmux_ecs::record::Schema;
use vmux_ecs::{JobSet, ProfileId};

pub struct Plugin;

impl bevy_app::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<vmux_ecs::record::Plugin>() {
            app.add_plugins(vmux_ecs::record::Plugin);
        }
        if !app.is_plugin_added::<vmux_ecs::OperationPlugin>() {
            app.add_plugins(vmux_ecs::OperationPlugin);
        }
        app.add_plugins((crate::catalog::Plugin, crate::mutations::Plugin))
            .configure_sets(
                Update,
                (
                    Set::Receive.after(JobSet::Apply),
                    Set::Seed,
                    vmux_ecs::record::Set::Restore,
                    Set::Adopt,
                    Set::Prepare,
                    vmux_ecs::record::Set::Detect,
                    vmux_ecs::record::Set::Capture,
                    Set::Write,
                    Set::Dispatch,
                    Set::Apply,
                    vmux_ecs::record::Set::Confirm,
                    Set::Reply,
                    Set::Publish,
                )
                    .chain(),
            );
        let mut schema = app.world_mut().resource_mut::<Schema>();
        schema.register::<Account>("account", 1);
        schema.register::<Profile>("profile", 1);
        schema.register::<Name>("name", 1);
        schema.register::<ProfileId>("directory", 1);
        schema.register_relationship::<Owner>("account", 1);
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Set {
    Receive,
    Seed,
    Adopt,
    Prepare,
    Write,
    Dispatch,
    Apply,
    Reply,
    Publish,
}

#[cfg(all(test, feature = "local"))]
mod tests {
    use super::*;
    use crate::{CreateRequest, Error, Failure, MutationCompleted, RenameRequest, Root};
    use uuid::Uuid;
    use vmux_ecs::{Ready, Revision, Uuid as Id};

    struct Fixture {
        app: App,
        root: Entity,
    }

    impl Fixture {
        fn at(data: &std::path::Path) -> Self {
            let mut app = App::new();
            app.add_plugins((Plugin, crate::local::Plugin));
            let root = app.world_mut().spawn(Root::at(data, "personal")).id();
            let mut fixture = Self { app, root };
            fixture.ready();
            fixture
        }

        fn ready(&mut self) {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            loop {
                self.app.update();
                if self.app.world().get::<Ready>(self.root).is_some() {
                    return;
                }
                if let Some(error) = self.app.world().get::<Error>(self.root) {
                    panic!("account load failed: {:?}", error.0)
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "account load timed out"
                );
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
        }

        fn profile(&mut self, directory_id: &str) -> (Uuid, String, Uuid, u64) {
            let mut rows = self
                .app
                .world_mut()
                .query_filtered::<(&Id, &ProfileId, &Name, &Owner, &Revision), With<Profile>>();
            let (_, id, name, owner, revision) = {
                let row = rows
                    .iter(self.app.world())
                    .find(|(_, directory, _, _, _)| directory.0 == directory_id)
                    .expect("profile exists");
                (
                    row.1.0.clone(),
                    row.0.0.parse().unwrap(),
                    row.2.as_str().to_string(),
                    row.3.0,
                    row.4.0,
                )
            };
            let account = self
                .app
                .world()
                .get::<Id>(owner)
                .unwrap()
                .0
                .parse()
                .unwrap();
            (id, name, account, revision)
        }

        fn entity(&mut self, directory_id: &str) -> Entity {
            let mut profiles = self
                .app
                .world_mut()
                .query_filtered::<(Entity, &ProfileId), With<Profile>>();
            for (entity, id) in profiles.iter(self.app.world()) {
                if id.0 == directory_id {
                    return entity;
                }
            }
            panic!("profile does not exist: {directory_id}")
        }

        fn create(&mut self, id: Uuid, name: &str) -> MutationCompleted {
            self.app.world_mut().write_message(CreateRequest {
                root: self.root,
                operation_id: id,
                name: name.into(),
            });
            self.result(id)
        }

        fn rename(&mut self, id: Uuid, profile_id: Uuid, name: &str) -> MutationCompleted {
            self.app.world_mut().write_message(RenameRequest {
                root: self.root,
                operation_id: id,
                profile_id,
                name: name.into(),
            });
            self.result(id)
        }

        fn result(&mut self, id: Uuid) -> MutationCompleted {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            loop {
                self.app.update();
                let results: Vec<_> = self
                    .app
                    .world_mut()
                    .resource_mut::<Messages<MutationCompleted>>()
                    .drain()
                    .collect();
                for result in results {
                    if result.operation_id == id {
                        return result;
                    }
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "profile mutation timed out"
                );
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
        }
    }

    #[test]
    fn legacy_profiles_and_offline_edits_restore_with_stable_identity_and_ownership() {
        let data = tempfile::tempdir().unwrap();
        let directory = data.path().join("profiles/personal");
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("display_name"), "Original").unwrap();
        std::fs::write(directory.join("history"), "preserved").unwrap();
        let mut fixture = Fixture::at(data.path());
        let before = fixture.profile("personal");
        assert_eq!(before.1, "Original");
        let result = fixture.rename(Uuid::new_v4(), before.0, "Renamed");
        assert_eq!(result.result.unwrap().revision, 2);
        let created = fixture.create(Uuid::new_v4(), "Work");
        let receipt = created.result.unwrap();
        assert_eq!(created.directory_id.as_deref(), Some("work"));
        assert_eq!(fixture.profile("work").2, before.2);
        drop(fixture);
        let mut restored = Fixture::at(data.path());
        assert_eq!(
            restored.profile("personal"),
            (before.0, "Renamed".into(), before.2, 2)
        );
        assert_eq!(
            restored.profile("work"),
            (receipt.entity_id, "Work".into(), before.2, 1)
        );
        assert_eq!(
            std::fs::read_to_string(directory.join("history")).unwrap(),
            "preserved"
        );
    }

    #[test]
    fn replay_returns_the_original_receipt_without_reverting_newer_profile_edits() {
        let data = tempfile::tempdir().unwrap();
        let mut fixture = Fixture::at(data.path());
        let operation = Uuid::new_v4();
        let first = fixture.create(operation, "Work").result.unwrap();
        fixture
            .rename(Uuid::new_v4(), first.entity_id, "Latest")
            .result
            .unwrap();
        let replay = fixture.create(operation, "Work");
        assert_eq!(replay.result.unwrap(), first);
        assert_eq!(replay.directory_id.as_deref(), Some("work"));
        assert_eq!(fixture.profile("work").1, "Latest");
        assert!(!data.path().join("profiles/work-2").exists());
    }

    #[test]
    fn concurrent_hosts_refuse_stale_writes_and_refresh_without_losing_the_draft() {
        let data = tempfile::tempdir().unwrap();
        let mut first = Fixture::at(data.path());
        let mut second = Fixture::at(data.path());
        let profile = first.profile("personal").0;
        assert_eq!(second.profile("personal").0, profile);
        first
            .rename(Uuid::new_v4(), profile, "Accepted")
            .result
            .unwrap();
        let refused = second.rename(Uuid::new_v4(), profile, "Unaccepted");
        assert_eq!(refused.result.unwrap_err(), Failure::Conflict);
        second.ready();
        assert_eq!(second.profile("personal").1, "Accepted");
        second
            .rename(Uuid::new_v4(), profile, "Retried")
            .result
            .unwrap();
        drop(first);
        drop(second);
        assert_eq!(Fixture::at(data.path()).profile("personal").1, "Retried");
    }

    #[test]
    fn failed_writes_preserve_committed_names_and_can_retry_after_a_database_lock() {
        let data = tempfile::tempdir().unwrap();
        let mut fixture = Fixture::at(data.path());
        let before = fixture.profile("personal");
        let held = rusqlite::Connection::open(data.path().join("accounts.sqlite")).unwrap();
        held.execute_batch("begin immediate").unwrap();
        let rejected = fixture.rename(Uuid::new_v4(), before.0, "Blocked");
        assert!(matches!(rejected.result, Err(Failure::Storage(_))));
        assert_eq!(fixture.profile("personal"), before);
        held.execute_batch("rollback").unwrap();
        fixture
            .rename(Uuid::new_v4(), before.0, "Retry")
            .result
            .unwrap();
        assert_eq!(fixture.profile("personal").1, "Retry");
    }

    #[test]
    fn failed_capture_returns_a_result_and_allows_a_later_valid_write() {
        let data = tempfile::tempdir().unwrap();
        let mut fixture = Fixture::at(data.path());
        let before = fixture.profile("personal");
        let entity = fixture.entity("personal");
        let scope = fixture
            .app
            .world()
            .get::<vmux_ecs::record::Scope>(entity)
            .unwrap()
            .0;
        fixture
            .app
            .world_mut()
            .entity_mut(entity)
            .insert(vmux_ecs::record::Scope(Uuid::new_v4()));
        let rejected = fixture.rename(Uuid::new_v4(), before.0, "Rejected");
        assert!(matches!(rejected.result, Err(Failure::Storage(_))));
        assert_eq!(fixture.profile("personal"), before);
        fixture
            .app
            .world_mut()
            .entity_mut(entity)
            .insert(vmux_ecs::record::Scope(scope));
        fixture
            .rename(Uuid::new_v4(), before.0, "Retry")
            .result
            .unwrap();
        drop(fixture);
        assert_eq!(Fixture::at(data.path()).profile("personal").1, "Retry");
    }

    #[test]
    fn exhausted_revisions_return_a_result_without_writing_a_new_name() {
        let data = tempfile::tempdir().unwrap();
        let mut fixture = Fixture::at(data.path());
        let before = fixture.profile("personal");
        let entity = fixture.entity("personal");
        fixture
            .app
            .world_mut()
            .get_mut::<Revision>(entity)
            .unwrap()
            .0 = i64::MAX as u64;
        let rejected = fixture.rename(Uuid::new_v4(), before.0, "Rejected");
        assert_eq!(rejected.result.unwrap_err(), Failure::NotReady);
        assert_eq!(fixture.profile("personal").1, before.1);
        fixture
            .app
            .world_mut()
            .get_mut::<Revision>(entity)
            .unwrap()
            .0 = before.3;
        fixture
            .rename(Uuid::new_v4(), before.0, "Retry")
            .result
            .unwrap();
        drop(fixture);
        assert_eq!(Fixture::at(data.path()).profile("personal").1, "Retry");
    }
}
