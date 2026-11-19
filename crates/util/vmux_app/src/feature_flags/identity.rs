use std::path::PathBuf;

use bevy_app::{App, Startup, Update};
use bevy_ecs::prelude::*;
use uuid::Uuid;
use vmux_ecs::{JobOwner, JobPlugin, JobRequest, JobResult, JobSet, Operation};
use vmux_feature::flag::{Attribute, Context, ContextFailure as Failure, Value};
use vmux_feature::{InstallationScope, Set};

use super::identity_io::{Create, Read};

pub(super) struct IdentityPlugin;

impl bevy_app::Plugin for IdentityPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((JobPlugin::<Read>::default(), JobPlugin::<Create>::default()))
            .add_systems(Startup, start.after(super::Configured))
            .add_systems(
                Update,
                (read, created)
                    .chain()
                    .after(JobSet::Apply)
                    .before(Set::Evaluate),
            );
    }
}

#[derive(Component)]
pub(super) struct Location(pub(super) PathBuf);

#[derive(Component)]
struct Candidate(String);

#[derive(Component)]
struct Retried;

fn start(
    scopes: Query<(Entity, Option<&Location>), With<InstallationScope>>,
    mut commands: Commands,
) {
    for (entity, location) in &scopes {
        let path = match location {
            Some(location) => location.0.clone(),
            None => vmux_profile::ProfilePaths::current()
                .application_data()
                .join("flags/installation-id"),
        };
        commands.entity(entity).insert(Location(path.clone()));
        commands.spawn((
            Operation::bundle("flags.identity.read").expect("identity read kind"),
            ChildOf(entity),
            Read { path: path.clone() },
            JobRequest(Read { path }),
        ));
    }
}

type Reads<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static ChildOf,
        &'static JobOwner<Read>,
        &'static mut JobResult<Read>,
        Has<Retried>,
    ),
>;

fn read(
    mut results: Reads,
    scopes: Query<&Location, With<InstallationScope>>,
    mut commands: Commands,
) {
    for (entity, owner, accepted, mut result, retried) in &mut results {
        let scope = owner.parent();
        let Ok(location) = scopes.get(scope) else {
            commands.entity(entity).despawn();
            continue;
        };
        if !accepted.accepts(Some(owner)) {
            commands.entity(entity).despawn();
            continue;
        }
        let result = match std::mem::replace(&mut result.0, Err("consumed identity read".into())) {
            Ok(result) => result,
            Err(error) => Err(error),
        };
        commands.entity(entity).despawn();
        match result {
            Ok(Some(bytes)) => {
                let identity = match std::str::from_utf8(&bytes)
                    .ok()
                    .and_then(|text| Uuid::parse_str(text.trim()).ok())
                {
                    Some(identity) => identity,
                    None => {
                        commands
                            .entity(scope)
                            .insert(Failure("installation identity is corrupt".into()));
                        continue;
                    }
                };
                let mut context = Context::installation(&identity.to_string());
                context.generation = 1;
                context.attributes.insert(
                    Attribute::Platform,
                    Value::String(std::env::consts::OS.into()),
                );
                context.attributes.insert(
                    Attribute::AppVersion,
                    Value::String(env!("CARGO_PKG_VERSION").into()),
                );
                commands.entity(scope).insert(context).remove::<Failure>();
            }
            Ok(None) if !retried => {
                let identity = Uuid::new_v4().to_string();
                commands.spawn((
                    Operation::bundle("flags.identity.create").expect("identity create kind"),
                    ChildOf(scope),
                    Candidate(identity.clone()),
                    Create {
                        path: location.0.clone(),
                        identity: identity.clone(),
                    },
                    JobRequest(Create {
                        path: location.0.clone(),
                        identity,
                    }),
                ));
            }
            Ok(None) => {
                commands
                    .entity(scope)
                    .insert(Failure("installation identity was not created".into()));
            }
            Err(error) => {
                commands.entity(scope).insert(Failure(error));
            }
        }
    }
}

type Creates<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static ChildOf,
        &'static Candidate,
        &'static JobOwner<Create>,
        &'static mut JobResult<Create>,
    ),
>;

fn created(
    mut results: Creates,
    scopes: Query<&Location, With<InstallationScope>>,
    mut commands: Commands,
) {
    for (entity, owner, candidate, accepted, mut result) in &mut results {
        let scope = owner.parent();
        let Ok(location) = scopes.get(scope) else {
            commands.entity(entity).despawn();
            continue;
        };
        if !accepted.accepts(Some(owner)) {
            commands.entity(entity).despawn();
            continue;
        }
        let result =
            match std::mem::replace(&mut result.0, Err("consumed identity creation".into())) {
                Ok(result) => result,
                Err(error) => Err(error),
            };
        commands.entity(entity).despawn();
        match result {
            Ok(true) => {
                let mut context = Context::installation(&candidate.0);
                context.generation = 1;
                context.attributes.insert(
                    Attribute::Platform,
                    Value::String(std::env::consts::OS.into()),
                );
                context.attributes.insert(
                    Attribute::AppVersion,
                    Value::String(env!("CARGO_PKG_VERSION").into()),
                );
                commands.entity(scope).insert(context).remove::<Failure>();
            }
            Ok(false) | Err(_) => {
                commands.spawn((
                    Operation::bundle("flags.identity.read").expect("identity reread kind"),
                    ChildOf(scope),
                    Retried,
                    Read {
                        path: location.0.clone(),
                    },
                    JobRequest(Read {
                        path: location.0.clone(),
                    }),
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;

    struct Fixture {
        app: App,
        scope: Entity,
    }

    impl Fixture {
        fn new(path: &std::path::Path) -> Self {
            let mut app = App::new();
            app.add_plugins(IdentityPlugin);
            let scope = app
                .world_mut()
                .spawn((
                    Name::new("Installation flags"),
                    InstallationScope,
                    Location(path.to_path_buf()),
                ))
                .id();
            Self { app, scope }
        }

        fn settle(&mut self) -> Result<String, String> {
            let until = Instant::now() + Duration::from_secs(5);
            loop {
                self.app.update();
                if let Some(context) = self.app.world().get::<Context>(self.scope) {
                    return Ok(context.subject.key.clone());
                }
                if let Some(failure) = self.app.world().get::<Failure>(self.scope) {
                    return Err(failure.0.clone());
                }
                assert!(
                    Instant::now() < until,
                    "installation identity did not settle"
                );
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    }

    #[test]
    fn first_run_and_restart_keep_the_same_durable_installation_subject() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("flags/installation-id");
        let first = Fixture::new(&path).settle().unwrap();
        assert!(Uuid::parse_str(&first).is_ok());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), first);
        assert_eq!(Fixture::new(&path).settle().unwrap(), first);
    }

    #[test]
    fn competing_writers_adopt_the_same_subject_without_overwriting_an_identity() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("installation-id");
        let mut first = Fixture::new(&path);
        let mut second = Fixture::new(&path);
        first.app.update();
        second.app.update();
        let first = first.settle().unwrap();
        assert_eq!(second.settle().unwrap(), first);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), first);
    }

    #[test]
    fn a_corrupt_identity_preserves_its_bytes_and_never_publishes_context() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("installation-id");
        std::fs::write(&path, "corrupt previous identity").unwrap();
        let mut fixture = Fixture::new(&path);
        assert!(fixture.settle().unwrap_err().contains("corrupt"));
        assert!(fixture.app.world().get::<Context>(fixture.scope).is_none());
        assert_eq!(
            std::fs::read_to_string(path).unwrap(),
            "corrupt previous identity"
        );
    }
}
