use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use vmux_api::remote::{Live, LiveDecisionSet};
use vmux_feature::flag::{Decision, Retention, Value};
use vmux_feature::manifest::{FeatureManifest, FeatureManifestOwner};
use vmux_feature::{Definition, EvaluationFailure, InstallationScope, ReleaseProvider, Set};

pub struct RemoteDomainPlugin;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RemoteDomainSet {
    Define,
    Publish,
}

impl Plugin for RemoteDomainPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<vmux_feature::FeaturePlugin>() {
            app.add_plugins(vmux_feature::FeaturePlugin);
        }
        app.add_plugins(vmux_feature::manifest::FeaturePlugin::<crate::Feature>::default())
            .add_systems(
                Update,
                define.in_set(RemoteDomainSet::Define).before(Set::Accept),
            )
            .add_systems(
                Update,
                publish
                    .in_set(RemoteDomainSet::Publish)
                    .in_set(LiveDecisionSet)
                    .after(Set::Publish),
            );
    }
}

fn define(
    providers: Query<Entity, With<ReleaseProvider>>,
    scopes: Query<Entity, With<InstallationScope>>,
    sources: Query<(Entity, &FeatureManifest), With<FeatureManifestOwner<crate::Feature>>>,
    existing: Query<(), With<Live>>,
    mut commands: Commands,
) {
    if !existing.is_empty() {
        return;
    }
    let Ok(provider) = providers.single() else {
        return;
    };
    let Ok(scope) = scopes.single() else {
        return;
    };
    let Ok((source, manifest)) = sources.single() else {
        return;
    };
    let (index, declaration) = manifest.flag("remote.live").expect("Live declaration");
    commands.spawn((
        Live,
        Definition::bundle(provider, source, index, scope, declaration),
        Retention::restoring(),
    ));
}

type Decisions<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        Option<&'static Decision<Value>>,
        Option<&'static EvaluationFailure>,
        Option<&'static Decision<bool>>,
    ),
    With<Live>,
>;

fn publish(values: Decisions, mut commands: Commands) {
    for (entity, value, failure, existing) in &values {
        if failure.is_some() {
            commands.entity(entity).remove::<Decision<bool>>();
            continue;
        }
        let Some(value) = value else {
            commands.entity(entity).remove::<Decision<bool>>();
            continue;
        };
        match value.clone().cast::<bool>() {
            Ok(decision) => {
                if existing != Some(&decision) {
                    commands.entity(entity).insert(decision);
                }
            }
            Err(_) => {
                commands.entity(entity).remove::<Decision<bool>>();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::time::{Duration, Instant};
    use vmux_feature::flag::{Configuration, Context, Source};
    use vmux_feature::{Generation, Provider, Receive, Tick};

    struct Fixture {
        app: App,
        provider: Entity,
    }

    struct ConsumerPlugin;

    #[derive(Resource, Default)]
    struct Observed(bool);

    impl Plugin for ConsumerPlugin {
        fn build(&self, app: &mut App) {
            app.init_resource::<Observed>()
                .add_systems(Update, consume.after(LiveDecisionSet));
        }
    }

    fn consume(values: Query<&Decision<bool>, With<Live>>, mut observed: ResMut<Observed>) {
        observed.0 = values.single().is_ok_and(|decision| decision.value);
    }

    #[test]
    fn consumers_close_admission_in_the_same_frame_as_the_accepted_off() {
        let directory = tempfile::tempdir().unwrap();
        let mut fixture = Fixture::new(Some(&directory.path().join("flags.json")));
        fixture.wait_available();
        assert!(fixture.app.world().resource::<Observed>().0);
        fixture.receive(1, false);
        assert!(!fixture.app.world().resource::<Observed>().0);
    }

    impl Fixture {
        fn new(path: Option<&std::path::Path>) -> Self {
            let mut app = App::new();
            app.add_plugins((RemoteDomainPlugin, ConsumerPlugin));
            let provider = app
                .world_mut()
                .spawn((Provider::bundle("production"), ReleaseProvider))
                .id();
            app.world_mut().spawn((
                Name::new("Installation"),
                InstallationScope,
                Context::installation("host-a"),
            ));
            if let Some(path) = path {
                app.add_plugins(vmux_feature::cache::CachePlugin);
                let store = app
                    .world_mut()
                    .spawn((
                        Name::new("Flags"),
                        vmux_feature::cache::Store,
                        vmux_feature::cache::Location(path.to_path_buf()),
                    ))
                    .id();
                app.world_mut()
                    .entity_mut(provider)
                    .insert(vmux_feature::cache::Binding {
                        store,
                        identity: "account-a".into(),
                        installation: "host-a".into(),
                    });
            }
            app.world_mut().write_message(Tick(1000));
            Self { app, provider }
        }

        fn decision(&mut self) -> Option<Decision<bool>> {
            self.app
                .world_mut()
                .query_filtered::<&Decision<bool>, With<Live>>()
                .single(self.app.world())
                .ok()
                .cloned()
        }

        fn wait_available(&mut self) {
            let until = Instant::now() + Duration::from_secs(5);
            loop {
                self.app.update();
                if self.decision().is_some_and(|decision| decision.value) {
                    return;
                }
                assert!(
                    Instant::now() < until,
                    "Live never became available after readable retention restore"
                );
                std::thread::sleep(Duration::from_millis(1));
            }
        }

        fn receive(&mut self, revision: u64, enabled: bool) {
            let generation = self.app.world().get::<Generation>(self.provider).unwrap().0;
            self.app.world_mut().write_message(Receive {
                receipt: None,
                provider: self.provider,
                generation,
                source: Source::Cloud,
                document: vmux_feature::snapshot::Document {
                    schema_version: 1,
                    environment: "production".into(),
                    revision,
                    issued_at: 1000,
                    valid_until: 1100,
                    flags: BTreeMap::from([(
                        "remote.live".into(),
                        Configuration::boolean(enabled),
                    )]),
                },
            });
            self.app.update();
        }
    }

    #[test]
    fn live_cannot_admit_default_or_cloud_on_before_retention_has_been_initialized() {
        let mut fixture = Fixture::new(None);
        fixture.app.update();
        assert!(!fixture.decision().unwrap().value);
        fixture.receive(1, true);
        assert!(!fixture.decision().unwrap().value);
    }

    #[test]
    fn independent_process_worlds_use_their_own_release_decision() {
        let directory = tempfile::tempdir().unwrap();
        let mut desktop = Fixture::new(Some(&directory.path().join("desktop.json")));
        let mut daemon = Fixture::new(Some(&directory.path().join("daemon.json")));
        desktop.wait_available();
        daemon.wait_available();
        desktop.receive(1, false);
        assert!(!desktop.decision().unwrap().value);
        daemon.app.update();
        assert!(daemon.decision().unwrap().value);
        daemon.receive(2, false);
        assert!(!daemon.decision().unwrap().value);
    }

    #[test]
    fn retained_off_survives_restart_expiry_and_a_requested_enable_override() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("daemon.json");
        {
            let mut fixture = Fixture::new(Some(&path));
            fixture.wait_available();
            fixture.receive(1, false);
            let until = Instant::now() + Duration::from_secs(5);
            loop {
                fixture.app.update();
                if fixture
                    .app
                    .world_mut()
                    .resource_mut::<Messages<vmux_feature::cache::RetentionAccepted>>()
                    .drain()
                    .any(|receipt| receipt.revision == 1)
                {
                    break;
                }
                assert!(
                    Instant::now() < until,
                    "retained off was not durably acknowledged"
                );
                std::thread::sleep(Duration::from_millis(1));
            }
        }
        let mut restored = Fixture::new(Some(&path));
        restored.app.world_mut().write_message(Tick(1200));
        let until = Instant::now() + Duration::from_secs(5);
        loop {
            restored.app.update();
            if restored
                .decision()
                .is_some_and(|decision| decision.details.retained_revision == Some(1))
            {
                break;
            }
            assert!(Instant::now() < until, "retained off was not restored");
            std::thread::sleep(Duration::from_millis(1));
        }
        let live = restored
            .app
            .world_mut()
            .query_filtered::<Entity, With<Live>>()
            .single(restored.app.world())
            .unwrap();
        restored
            .app
            .insert_resource(vmux_feature::OverridePolicy::development());
        restored
            .app
            .world_mut()
            .entity_mut(live)
            .insert(vmux_feature::DevelopmentOverride(Value::Boolean(true)));
        restored.app.update();
        assert!(!restored.decision().unwrap().value);
    }

    #[test]
    fn inspection_of_another_environment_does_not_redirect_live() {
        let directory = tempfile::tempdir().unwrap();
        let mut fixture = Fixture::new(Some(&directory.path().join("flags.json")));
        fixture.wait_available();
        let other = fixture
            .app
            .world_mut()
            .spawn(Provider::bundle("other"))
            .id();
        fixture.app.world_mut().write_message(Receive {
            receipt: None,
            provider: other,
            generation: 0,
            source: Source::Cloud,
            document: vmux_feature::snapshot::Document {
                schema_version: 1,
                environment: "other".into(),
                revision: 1,
                issued_at: 1000,
                valid_until: 1100,
                flags: BTreeMap::from([("remote.live".into(), Configuration::boolean(false))]),
            },
        });
        fixture.app.update();
        assert!(fixture.decision().unwrap().value);
    }
}
