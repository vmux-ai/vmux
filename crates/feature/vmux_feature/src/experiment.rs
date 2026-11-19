use bevy_app::{App, Update};
use bevy_ecs::prelude::*;
use bevy_ecs::system::SystemParam;

use crate::cache::{AssignmentCapacity, Binding, SaveAssignment, StoreFailure};
use crate::flag::{Assignment, Class, Configuration, Decision, Details, Lifecycle, Reason, Value};
use crate::runtime::{Bindings, Evaluated};
use crate::{Definition, EvaluationFailure, Set};

pub struct ExperimentPlugin;

impl bevy_app::Plugin for ExperimentPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Activate>()
            .add_systems(
                Update,
                (restore, select)
                    .chain()
                    .after(Set::Evaluate)
                    .before(Set::Publish),
            )
            .add_systems(Update, activate.after(Set::Publish));
    }
}

#[derive(Message)]
pub struct Activate {
    pub evaluation: Entity,
    pub activation: String,
    pub context_generation: u64,
}

#[derive(Component, Clone)]
#[component(immutable)]
pub struct Exposure {
    pub activation: String,
    pub assignment: Assignment,
    pub decision: Details,
}

#[derive(Component)]
struct Selecting(Assignment);

#[derive(Component)]
struct Selected(Entity);

#[derive(SystemParam)]
struct Catalog<'w, 's> {
    configurations: Query<'w, 's, (&'static Name, &'static ChildOf, &'static Configuration)>,
    assignments: Query<'w, 's, (Entity, &'static ChildOf, &'static Assignment)>,
}

impl Catalog<'_, '_> {
    fn generation(&self, provider: Entity, key: &str) -> Option<&str> {
        for (name, parent, configuration) in &self.configurations {
            if parent.parent() == provider && name.as_str() == key {
                return configuration.experiment_generation.as_deref();
            }
        }
        None
    }

    fn assignment(
        &self,
        store: Entity,
        environment: &str,
        generation: &str,
        key: &str,
        subject: &crate::flag::Subject,
    ) -> Option<(Entity, &Assignment)> {
        for (entity, parent, assignment) in &self.assignments {
            if parent.parent() == store && assignment.matches(environment, generation, key, subject)
            {
                return Some((entity, assignment));
            }
        }
        None
    }
}

fn restore(
    mut evaluations: Query<(Entity, &Definition, &ChildOf, &mut Evaluated)>,
    bindings: Bindings,
    providers: Query<(&Name, &Binding)>,
    catalog: Catalog,
    mut commands: Commands,
) {
    for (entity, definition, parent, mut evaluated) in &mut evaluations {
        let Ok((declaration, context)) = bindings.resolve(definition) else {
            continue;
        };
        if declaration.class != Class::Experiment || declaration.lifecycle == Lifecycle::Retired {
            continue;
        }
        let Ok((environment, binding)) = providers.get(parent.parent()) else {
            continue;
        };
        let Some(generation) = catalog.generation(parent.parent(), &declaration.key) else {
            continue;
        };
        let Some((selected, assignment)) = catalog.assignment(
            binding.store,
            environment.as_str(),
            generation,
            &declaration.key,
            &context.subject,
        ) else {
            continue;
        };
        let Ok(decision) = &mut evaluated.0 else {
            continue;
        };
        if !decision.is_experiment_candidate() {
            continue;
        }
        if assignment.value.value_type() != declaration.value_type {
            evaluated.0 = Err(EvaluationFailure(
                "stored experiment assignment has an incompatible type".into(),
            ));
            continue;
        }
        decision.value = assignment.value.clone();
        decision.details.variant = assignment.variant.clone();
        decision.details.reason = Reason::StickyAssignment;
        commands
            .entity(entity)
            .remove::<Selecting>()
            .insert(Selected(selected));
    }
}

impl Decision<Value> {
    fn is_experiment_candidate(&self) -> bool {
        self.details.snapshot.as_ref().is_some_and(|snapshot| {
            matches!(
                snapshot.source,
                crate::flag::Source::Cloud | crate::flag::Source::Cache
            )
        }) && !matches!(
            self.details.reason,
            Reason::ProviderUnavailable
                | Reason::MissingContext
                | Reason::TypeError
                | Reason::DevelopmentOverride
        )
    }
}

type AssignmentFailures<'w, 's> =
    Query<'w, 's, (), Or<(With<StoreFailure>, With<AssignmentCapacity>)>>;

fn select(
    mut evaluations: Query<(
        Entity,
        &Definition,
        &ChildOf,
        &mut Evaluated,
        Option<&Selecting>,
    )>,
    bindings: Bindings,
    providers: Query<(&Name, &Binding)>,
    catalog: Catalog,
    failures: AssignmentFailures,
    mut requests: MessageWriter<SaveAssignment>,
    mut commands: Commands,
) {
    for (entity, definition, owner, mut evaluated, pending) in &mut evaluations {
        let Ok((declaration, context)) = bindings.resolve(definition) else {
            continue;
        };
        if declaration.class != Class::Experiment || declaration.lifecycle == Lifecycle::Retired {
            continue;
        }
        let Ok((environment, binding)) = providers.get(owner.parent()) else {
            continue;
        };
        let Some(generation) = catalog.generation(owner.parent(), &declaration.key) else {
            continue;
        };
        let Ok(decision) = &mut evaluated.0 else {
            continue;
        };
        if !decision.is_experiment_candidate()
            || decision.details.reason == Reason::StickyAssignment
        {
            continue;
        }
        let matches = |assignment: &Assignment| {
            assignment.matches(
                environment.as_str(),
                generation,
                &declaration.key,
                &context.subject,
            )
        };
        if failures.contains(binding.store) {
            commands.entity(entity).remove::<Selecting>();
            evaluated.0 = Err(EvaluationFailure(
                "experiment assignment storage is unavailable".into(),
            ));
            continue;
        }
        if pending.is_none_or(|pending| !matches(&pending.0)) {
            let assignment = Assignment {
                environment: environment.as_str().into(),
                generation: generation.into(),
                flag: declaration.key.clone(),
                subject: context.subject.clone(),
                value: decision.value.clone(),
                variant: decision.details.variant.clone(),
            };
            requests.write(SaveAssignment {
                store: binding.store,
                assignment: assignment.clone(),
            });
            commands.entity(entity).insert(Selecting(assignment));
        }
        evaluated.0 = Err(EvaluationFailure(
            "experiment assignment awaits durable acknowledgement".into(),
        ));
    }
}

#[derive(SystemParam)]
struct Exposures<'w, 's> {
    entries: Query<'w, 's, (&'static ChildOf, &'static Exposure)>,
    now: Res<'w, crate::runtime::Now>,
}

fn activate(
    mut requests: MessageReader<Activate>,
    evaluations: Query<(&Definition, &ChildOf, &Decision<Value>, &Selected)>,
    bindings: Bindings,
    providers: Query<&Binding>,
    assignments: Query<(&ChildOf, &Assignment)>,
    exposures: Exposures,
    mut commands: Commands,
) {
    let mut accepted = std::collections::BTreeSet::new();
    for request in requests.read() {
        if request.activation.is_empty()
            || request.activation.len() > 128
            || request.activation.chars().any(char::is_control)
        {
            continue;
        }
        if !accepted.insert((request.evaluation, request.activation.as_str())) {
            continue;
        }
        let Ok((definition, owner, decision, selected)) = evaluations.get(request.evaluation)
        else {
            continue;
        };
        if decision.details.reason != Reason::StickyAssignment {
            continue;
        }
        let Ok((declaration, context)) = bindings.resolve(definition) else {
            continue;
        };
        if decision.details.context_generation != context.generation {
            continue;
        }
        if request.context_generation != context.generation {
            continue;
        }
        let Ok(binding) = providers.get(owner.parent()) else {
            continue;
        };
        if exposures.entries.iter().any(|(parent, exposure)| {
            parent.parent() == request.evaluation && exposure.activation == request.activation
        }) {
            continue;
        }
        let Some(snapshot) = &decision.details.snapshot else {
            continue;
        };
        if !snapshot.is_valid(exposures.now.0) {
            continue;
        }
        let Ok((parent, assignment)) = assignments.get(selected.0) else {
            continue;
        };
        if parent.parent() != binding.store
            || assignment.environment != snapshot.environment
            || assignment.flag != declaration.key
            || assignment.subject != context.subject
            || assignment.value != decision.value
            || assignment.variant != decision.details.variant
        {
            continue;
        }
        commands.spawn((
            Name::new("Experiment exposure"),
            ChildOf(request.evaluation),
            Exposure {
                activation: request.activation.clone(),
                assignment: assignment.clone(),
                decision: decision.details.clone(),
            },
        ));
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::time::{Duration, Instant};

    use super::*;
    use crate::cache::{CachePlugin, Location, Store};
    use crate::flag::{Context, Source};
    use crate::manifest::{FeatureManifest, FeatureManifestOwner, FeatureManifestSource};
    use crate::{Generation, Provider, Receive, Tick};

    struct Feature;

    impl FeatureManifestSource for Feature {
        const SOURCE: &'static str = r#"(
            flags: [(key: "experiment.navigation", type: String, class: Experiment,
                default: String("control"), owner: "Navigation",
                expiry: Some("2027-06-01T00:00:00Z"), lifecycle: Temporary),
                (key: "remote.live", type: Boolean, class: Release, default: Boolean(true),
                owner: "Remote", expiry: Some("2027-06-01T00:00:00Z"), lifecycle: Temporary, offline: RetainOff)]
        )"#;
    }

    struct Fixture {
        app: App,
        provider: Entity,
        evaluation: Entity,
        scope: Entity,
        store: Entity,
        live: Entity,
        path: PathBuf,
    }

    impl Fixture {
        fn new(path: &Path) -> Self {
            let mut app = App::new();
            app.add_plugins((
                CachePlugin,
                crate::manifest::FeaturePlugin::<Feature>::default(),
            ));
            let source = app
                .world_mut()
                .query_filtered::<Entity, With<FeatureManifestOwner<Feature>>>()
                .single(app.world())
                .unwrap();
            let scope = app
                .world_mut()
                .spawn((
                    Name::new("Installation"),
                    Context::installation("installation-a"),
                ))
                .id();
            let store = app
                .world_mut()
                .spawn((Name::new("Cache"), Store, Location(path.into())))
                .id();
            let provider = app
                .world_mut()
                .spawn((
                    Provider::bundle("development"),
                    Binding {
                        store,
                        identity: "account-a".into(),
                        installation: "installation-a".into(),
                    },
                ))
                .id();
            let declaration = &FeatureManifest::of::<Feature>().flags[0];
            let evaluation = app
                .world_mut()
                .spawn(Definition::bundle(provider, source, 0, scope, declaration))
                .id();
            let live = app
                .world_mut()
                .spawn(Definition::bundle(
                    provider,
                    source,
                    1,
                    scope,
                    &FeatureManifest::of::<Feature>().flags[1],
                ))
                .id();
            app.world_mut().write_message(Tick(1000));
            Self {
                app,
                provider,
                evaluation,
                scope,
                store,
                live,
                path: path.into(),
            }
        }

        fn receive(&mut self, revision: u64, generation: &str, value: &str) {
            self.app.update();
            let provider_generation = self.app.world().get::<Generation>(self.provider).unwrap().0;
            self.app.world_mut().write_message(Receive {
                receipt: None,
                provider: self.provider,
                generation: provider_generation,
                source: Source::Cloud,
                document: crate::snapshot::Document {
                    schema_version: 2,
                    environment: "development".into(),
                    revision,
                    issued_at: 1000,
                    valid_until: 1100,
                    flags: std::collections::BTreeMap::from([(
                        "experiment.navigation".into(),
                        Configuration {
                            value: Value::String(value.into()),
                            variant: Some(value.into()),
                            rules: Vec::new(),
                            experiment_generation: Some(generation.into()),
                        },
                    )]),
                },
            });
        }

        fn settle(&mut self, expected: &str, revision: u64) {
            let until = Instant::now() + Duration::from_secs(5);
            loop {
                self.app.update();
                let pending = self
                    .app
                    .world_mut()
                    .query_filtered::<&ChildOf, With<vmux_ecs::Operation>>()
                    .iter(self.app.world())
                    .any(|parent| parent.parent() == self.store);
                let decision = self.app.world().get::<Decision<String>>(self.evaluation);
                let persisted = std::fs::read(&self.path)
                    .ok()
                    .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok());
                let cached = persisted
                    .as_ref()
                    .and_then(|value| value["snapshots"].as_array())
                    .is_some_and(|snapshots| {
                        snapshots
                            .iter()
                            .any(|snapshot| snapshot["document"]["revision"] == revision)
                    });
                if !pending
                    && cached
                    && decision.is_some_and(|decision| {
                        decision.value == expected
                            && decision.details.reason == Reason::StickyAssignment
                    })
                {
                    return;
                }
                assert!(
                    Instant::now() < until,
                    "expected {expected} at revision {revision}; decision={decision:?}; failure={:?}; persisted={persisted:?}",
                    self.app
                        .world()
                        .get::<EvaluationFailure>(self.evaluation)
                        .map(|failure| failure.0.as_str()),
                );
                std::thread::sleep(Duration::from_millis(1));
            }
        }

        fn exposures(&mut self) -> Vec<Exposure> {
            self.app
                .world_mut()
                .query::<&Exposure>()
                .iter(self.app.world())
                .cloned()
                .collect()
        }
    }

    #[test]
    fn sticky_selection_survives_new_configuration_and_offline_restart_until_a_new_generation() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("flags.json");
        let mut fixture = Fixture::new(&path);
        fixture.receive(1, "generation-a", "alpha");
        fixture.settle("alpha", 1);
        assert!(fixture.exposures().is_empty());
        fixture.receive(2, "generation-a", "beta");
        fixture.settle("alpha", 2);
        drop(fixture);
        let mut fixture = Fixture::new(&path);
        fixture.settle("alpha", 2);
        fixture.receive(3, "generation-b", "beta");
        fixture.settle("beta", 3);
        let saved: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(saved["assignments"].as_array().unwrap().len(), 2);
        assert!(fixture.exposures().is_empty());
    }

    #[test]
    fn exposure_requires_activation_and_deduplicates_the_same_activation_across_updates() {
        let directory = tempfile::tempdir().unwrap();
        let mut fixture = Fixture::new(&directory.path().join("flags.json"));
        fixture.receive(1, "generation-a", "alpha");
        fixture.settle("alpha", 1);
        for _ in 0..2 {
            fixture.app.world_mut().write_message(Activate {
                evaluation: fixture.evaluation,
                activation: "open-navigation".into(),
                context_generation: 0,
            });
        }
        fixture.app.update();
        fixture.app.world_mut().write_message(Activate {
            evaluation: fixture.evaluation,
            activation: "open-navigation".into(),
            context_generation: 0,
        });
        fixture.app.update();
        let exposures = fixture.exposures();
        assert_eq!(exposures.len(), 1);
        assert_eq!(exposures[0].assignment.generation, "generation-a");
        fixture
            .app
            .world_mut()
            .get_mut::<Context>(fixture.scope)
            .unwrap()
            .generation += 1;
        fixture.app.world_mut().write_message(Activate {
            evaluation: fixture.evaluation,
            activation: "old-context".into(),
            context_generation: 0,
        });
        fixture.app.world_mut().run_schedule(Update);
        assert_eq!(fixture.exposures().len(), 1);
    }

    #[test]
    fn assignment_capacity_preserves_live_retention_and_retirement_recovers_only_inactive_generations()
     {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("flags.json");
        let mut fixture = Fixture::new(&path);
        fixture.app.insert_resource(crate::cache::CacheLimits {
            assignment_count: 1,
            assignment_bytes: 65536,
        });
        fixture.receive(1, "generation-a", "alpha");
        fixture.settle("alpha", 1);
        fixture.receive(2, "generation-b", "beta");
        let until = Instant::now() + Duration::from_secs(5);
        while fixture
            .app
            .world()
            .get::<crate::cache::AssignmentCapacity>(fixture.store)
            .is_none()
        {
            fixture.app.update();
            assert!(
                Instant::now() < until,
                "capacity refusal was not materialized"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        let generation = fixture
            .app
            .world()
            .get::<Generation>(fixture.provider)
            .unwrap()
            .0;
        fixture.app.world_mut().write_message(Receive {
            receipt: None,
            provider: fixture.provider,
            generation,
            source: Source::Cloud,
            document: crate::snapshot::Document {
                schema_version: 2,
                environment: "development".into(),
                revision: 3,
                issued_at: 1000,
                valid_until: 1100,
                flags: std::collections::BTreeMap::from([
                    ("remote.live".into(), Configuration::boolean(false)),
                    (
                        "experiment.navigation".into(),
                        Configuration {
                            value: Value::String("beta".into()),
                            variant: Some("beta".into()),
                            rules: Vec::new(),
                            experiment_generation: Some("generation-b".into()),
                        },
                    ),
                ]),
            },
        });
        loop {
            fixture.app.update();
            let stored = std::fs::read(&path)
                .ok()
                .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok());
            if stored.as_ref().is_some_and(|stored| {
                stored["retained"].as_array().is_some_and(|retained| {
                    retained
                        .iter()
                        .any(|record| record["flag"] == "remote.live")
                })
            }) {
                break;
            }
            assert!(
                Instant::now() < until,
                "assignment capacity prevented the independent durable disable"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(
            !fixture
                .app
                .world()
                .get::<crate::Current>(fixture.live)
                .unwrap()
                .0
        );
        fixture
            .app
            .world_mut()
            .write_message(crate::cache::RetireAssignments {
                store: fixture.store,
                environment: "development".into(),
                flag: "experiment.navigation".into(),
                generation: "generation-a".into(),
            });
        fixture.settle("beta", 3);
        fixture.receive(4, "generation-b", "gamma");
        fixture.settle("beta", 4);
        fixture
            .app
            .world_mut()
            .write_message(crate::cache::RetireAssignments {
                store: fixture.store,
                environment: "development".into(),
                flag: "experiment.navigation".into(),
                generation: "generation-b".into(),
            });
        fixture.settle("beta", 4);
        let stored: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(stored["assignments"].as_array().unwrap().len(), 1);
        assert_eq!(stored["assignments"][0]["generation"], "generation-b");
    }
}
