use std::collections::BTreeMap;

use bevy_app::{App, Update};
use bevy_ecs::prelude::*;
use bevy_ecs::system::SystemParam;
use vmux_ecs::Revision;

use crate::flag::{
    Configuration, Context, ContextFailure, Decision, Declaration, OfflinePolicy, Retention,
    Snapshot, Source, Value,
};
use crate::manifest::FeatureManifest;
use crate::snapshot::Document;

pub struct FeaturePlugin;

impl bevy_app::Plugin for FeaturePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Now>()
            .init_resource::<OverridePolicy>()
            .add_message::<Tick>()
            .add_message::<Receive>()
            .add_message::<SnapshotAccepted>()
            .add_message::<RefreshFailed>()
            .add_message::<Changed>()
            .add_message::<Decided>()
            .configure_sets(
                Update,
                (
                    Set::Clock,
                    Set::Prepare,
                    Set::Accept,
                    Set::Evaluate,
                    Set::Retain,
                    Set::Publish,
                )
                    .chain(),
            )
            .add_systems(Update, advance.in_set(Set::Clock))
            .add_systems(Update, (accept, fail, expire).chain().in_set(Set::Accept))
            .add_systems(Update, evaluate.in_set(Set::Evaluate))
            .add_systems(Update, (install, project).chain().in_set(Set::Publish));
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Set {
    Clock,
    Prepare,
    Accept,
    Evaluate,
    Retain,
    Publish,
}

#[derive(Component)]
#[require(Revision, Generation, Lease)]
pub struct Provider;

#[derive(Component)]
pub struct ReleaseProvider;

#[derive(Component)]
pub struct InstallationScope;

impl Provider {
    pub fn bundle(environment: &str) -> impl Bundle {
        (Self, Name::new(environment.to_string()))
    }
}

#[derive(Component)]
#[component(immutable)]
#[require(Current, Revision)]
pub struct Definition {
    pub source: Entity,
    pub index: usize,
    pub scope: Entity,
}

impl Definition {
    pub fn bundle(
        provider: Entity,
        source: Entity,
        index: usize,
        scope: Entity,
        declaration: &Declaration,
    ) -> impl Bundle {
        (
            Self {
                source,
                index,
                scope,
            },
            Name::new(declaration.key.clone()),
            ChildOf(provider),
        )
    }
}

#[derive(Component, Default, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Current(pub bool);

#[derive(Component)]
pub struct Failure;

#[derive(Component, Default)]
pub struct Generation(pub u64);

#[derive(Message)]
pub struct Tick(pub u64);

#[derive(Message)]
pub struct Receive {
    pub provider: Entity,
    pub generation: u64,
    pub document: Document,
    pub source: Source,
    pub receipt: Option<Entity>,
}

#[derive(Message)]
pub struct SnapshotAccepted {
    pub provider: Entity,
    pub generation: u64,
    pub receipt: Option<Entity>,
    pub document: Document,
    pub source: Source,
}

#[derive(Message)]
pub struct RefreshFailed {
    pub provider: Entity,
    pub generation: u64,
}

#[derive(Message)]
pub struct Changed {
    pub entity: Entity,
    pub enabled: bool,
    pub revision: u64,
}

#[derive(Message)]
pub struct Decided {
    pub entity: Entity,
    pub decision: Decision<Value>,
}

#[derive(Component)]
pub struct DevelopmentOverride(pub Value);

#[derive(Resource, Default)]
pub struct OverridePolicy {
    allowed: bool,
}

impl OverridePolicy {
    pub fn development() -> Self {
        Self { allowed: true }
    }

    pub fn allows_overrides(&self) -> bool {
        self.allowed
    }
}

#[derive(Component, Clone, PartialEq, Eq)]
pub struct EvaluationFailure(pub String);

#[derive(Component)]
#[allow(unknown_lints, vmux_ecs_ownership)]
pub(crate) struct Evaluated(pub(crate) Result<Decision<Value>, EvaluationFailure>);

#[derive(Component, Default)]
pub(crate) struct Lease {
    pub(crate) generation: u64,
    pub(crate) issued_at: u64,
    pub(crate) valid_until: u64,
    pub(crate) fingerprint: [u8; 32],
    pub(crate) available: bool,
    pub(crate) source: Source,
}

#[derive(Component)]
struct Published;

#[derive(Resource, Default)]
pub(crate) struct Now(pub(crate) u64);

#[cfg(feature = "http")]
impl Now {
    pub(crate) fn fresh(&self) -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(self.0, |time| time.as_secs().max(self.0))
    }
}

fn advance(mut ticks: MessageReader<Tick>, mut now: ResMut<Now>) {
    for tick in ticks.read() {
        now.0 = now.0.max(tick.0);
    }
}

fn accept(
    mut input: MessageReader<Receive>,
    mut providers: Query<(&Name, &mut Revision, &Generation, &mut Lease), With<Provider>>,
    values: Query<(Entity, &Name, &ChildOf), With<Published>>,
    now: Res<Now>,
    mut receipts: MessageWriter<SnapshotAccepted>,
    mut commands: Commands,
) {
    let mut accepted = BTreeMap::new();
    for input in input.read() {
        let Ok((environment, mut revision, generation, mut lease)) =
            providers.get_mut(input.provider)
        else {
            continue;
        };
        if generation.0 != input.generation {
            continue;
        }
        if !input.document.is_valid(environment.as_str(), now.0)
            || !matches!(input.source, Source::Cloud | Source::Cache)
        {
            commands.entity(input.provider).insert(Failure);
            continue;
        }
        if input.document.revision < revision.0 {
            continue;
        }
        let fingerprint = input.document.fingerprint();
        if input.document.revision == revision.0
            && (fingerprint != lease.fingerprint || input.document.issued_at < lease.issued_at)
        {
            continue;
        }
        revision.0 = input.document.revision;
        *lease = Lease {
            generation: input.generation,
            issued_at: input.document.issued_at,
            valid_until: input.document.valid_until,
            fingerprint,
            available: true,
            source: input.source,
        };
        accepted.insert(input.provider, input);
        commands.entity(input.provider).remove::<Failure>();
    }
    for (provider, input) in accepted {
        receipts.write(SnapshotAccepted {
            provider,
            generation: input.generation,
            receipt: input.receipt,
            document: input.document.clone(),
            source: input.source,
        });
        let mut remaining = input.document.flags.clone();
        for (entity, name, owner) in &values {
            if owner.parent() != provider {
                continue;
            }
            if let Some(published) = remaining.remove(name.as_str()) {
                commands.entity(entity).insert(published);
            } else {
                commands.entity(entity).despawn();
            }
        }
        for (key, value) in remaining {
            commands.spawn((Name::new(key), ChildOf(provider), Published, value));
        }
    }
}

fn fail(
    mut failures: MessageReader<RefreshFailed>,
    mut providers: Query<(&Generation, &mut Lease), With<Provider>>,
    mut commands: Commands,
) {
    for failure in failures.read() {
        let Ok((generation, _lease)) = providers.get_mut(failure.provider) else {
            continue;
        };
        if generation.0 != failure.generation {
            continue;
        }
        commands.entity(failure.provider).insert(Failure);
    }
}

fn expire(mut providers: Query<(&Generation, &mut Lease), With<Provider>>, now: Res<Now>) {
    for (generation, mut lease) in &mut providers {
        if now.0 >= lease.valid_until || generation.0 != lease.generation {
            lease.available = false;
        }
    }
}

type Definitions<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static ChildOf,
        &'static Definition,
        Option<&'static Retention>,
        Option<&'static DevelopmentOverride>,
    ),
    Without<Provider>,
>;

fn evaluate(
    providers: Query<(&Name, &Revision, &Generation, &Lease), With<Provider>>,
    published: Query<(&Name, &ChildOf, &Configuration), With<Published>>,
    definitions: Definitions,
    bindings: Bindings,
    now: Res<Now>,
    overrides: Res<OverridePolicy>,
    mut commands: Commands,
) {
    for (entity, owner, definition, retained, development_override) in &definitions {
        let (declaration, context) = match bindings.resolve(definition) {
            Ok(bound) => bound,
            Err(failure) => {
                commands.entity(entity).insert(Evaluated(Err(failure)));
                continue;
            }
        };
        let mut snapshot = None;
        let mut configuration = None;
        if let Ok((environment, revision, generation, lease)) = providers.get(owner.parent())
            && revision.0 > 0
            && lease.generation == generation.0
        {
            snapshot = Some(Snapshot {
                environment: environment.as_str().into(),
                revision: revision.0,
                issued_at: lease.issued_at,
                valid_until: lease.valid_until,
                source: lease.source,
            });
            if lease.available {
                for (key, provider, value) in &published {
                    if provider.parent() == owner.parent() && key.as_str() == declaration.key {
                        configuration = Some(value);
                        break;
                    }
                }
            }
        }
        let retention = match declaration.offline {
            OfflinePolicy::Default => Retention::absent(),
            OfflinePolicy::RetainOff => retained.copied().unwrap_or(Retention::restoring()),
        };
        let development_override = if overrides.allowed {
            development_override.map(|value| &value.0)
        } else {
            None
        };
        let decision = declaration
            .decide(
                context,
                configuration,
                snapshot.as_ref(),
                development_override,
                retention,
                now.0,
            )
            .map_err(EvaluationFailure);
        commands.entity(entity).insert(Evaluated(decision));
    }
}

#[derive(SystemParam)]
pub(crate) struct Bindings<'w, 's> {
    sources: Query<'w, 's, &'static FeatureManifest>,
    contexts: Query<'w, 's, &'static Context>,
    failures: Query<'w, 's, &'static ContextFailure>,
}

impl Bindings<'_, '_> {
    pub(crate) fn resolve(
        &self,
        definition: &Definition,
    ) -> Result<(&Declaration, &Context), EvaluationFailure> {
        let source = self
            .sources
            .get(definition.source)
            .map_err(|_| EvaluationFailure("flag declaration source is missing".into()))?;
        let declaration = source
            .flags
            .get(definition.index)
            .ok_or_else(|| EvaluationFailure("flag declaration is missing".into()))?;
        let context = self.contexts.get(definition.scope).map_err(|_| {
            EvaluationFailure(
                self.failures
                    .get(definition.scope)
                    .map_or("flag context is missing", |failure| failure.0.as_str())
                    .into(),
            )
        })?;
        Ok((declaration, context))
    }
}

type Evaluations<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Evaluated,
        Option<&'static Decision<Value>>,
        Option<&'static EvaluationFailure>,
    ),
>;

fn install(evaluations: Evaluations, mut changes: MessageWriter<Changed>, mut commands: Commands) {
    for (entity, evaluated, current, failure) in &evaluations {
        let mut target = commands.entity(entity);
        target.remove::<Evaluated>();
        match &evaluated.0 {
            Ok(decision) => {
                if current != Some(decision) {
                    target.insert(decision.clone());
                }
                target.remove::<EvaluationFailure>();
            }
            Err(error) => {
                if failure == Some(error) {
                    continue;
                }
                target.remove::<(
                    Decision<Value>,
                    Decision<bool>,
                    Decision<String>,
                    Decision<i64>,
                    Decision<f64>,
                    Decision<serde_json::Value>,
                )>();
                target.insert((Current(false), Revision(0), error.clone()));
                changes.write(Changed {
                    entity,
                    enabled: false,
                    revision: 0,
                });
            }
        }
    }
}

fn project(
    decisions: Query<(Entity, &Decision<Value>), bevy_ecs::query::Changed<Decision<Value>>>,
    mut changes: MessageWriter<Changed>,
    mut decided: MessageWriter<Decided>,
    mut commands: Commands,
) {
    for (entity, decision) in &decisions {
        let revision = decision
            .details
            .snapshot
            .as_ref()
            .map_or(0, |snapshot| snapshot.revision);
        let mut target = commands.entity(entity);
        target.remove::<(
            Decision<bool>,
            Decision<String>,
            Decision<i64>,
            Decision<f64>,
            Decision<serde_json::Value>,
        )>();
        let enabled = match &decision.value {
            Value::Boolean(value) => {
                target.insert(
                    decision
                        .clone()
                        .cast::<bool>()
                        .expect("typed boolean decision"),
                );
                *value
            }
            Value::String(_) => {
                target.insert(
                    decision
                        .clone()
                        .cast::<String>()
                        .expect("typed string decision"),
                );
                false
            }
            Value::Integer(_) => {
                target.insert(
                    decision
                        .clone()
                        .cast::<i64>()
                        .expect("typed integer decision"),
                );
                false
            }
            Value::Float(_) => {
                target.insert(
                    decision
                        .clone()
                        .cast::<f64>()
                        .expect("typed finite float decision"),
                );
                false
            }
            Value::Json(_) => {
                target.insert(
                    decision
                        .clone()
                        .cast::<serde_json::Value>()
                        .expect("typed JSON decision"),
                );
                false
            }
        };
        target.insert((Current(enabled), Revision(revision)));
        changes.write(Changed {
            entity,
            enabled,
            revision,
        });
        decided.write(Decided {
            entity,
            decision: decision.clone(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flag::{Attribute, Condition, Operator, Rule, Subject, SubjectKind};
    use crate::manifest::FeatureManifestSource;

    struct FixtureFeature;

    impl FeatureManifestSource for FixtureFeature {
        const SOURCE: &'static str = r#"(flags: [
            (key: "account.cloud", type: Boolean, class: Release, default: Boolean(false)),
            (key: "remote.live", type: Boolean, class: Release, default: Boolean(true), owner: "vmux", expiry: Some("2027-06-01T00:00:00Z"), lifecycle: Temporary, offline: RetainOff),
            (key: "typed.string", type: String, class: Config, default: String("default")),
            (key: "typed.integer", type: Integer, class: Config, default: Integer(0)),
            (key: "typed.float", type: Float, class: Config, default: Float(0.0)),
            (key: "typed.json", type: Json, class: Config, default: Json({})),
        ])"#;
    }

    struct Fixture {
        app: App,
        provider: Entity,
        flag: Entity,
        source: Entity,
        scope: Entity,
    }

    impl Fixture {
        fn bind(&mut self, index: usize, scope: Entity) -> Entity {
            self.app
                .world_mut()
                .spawn(Definition::bundle(
                    self.provider,
                    self.source,
                    index,
                    scope,
                    &FeatureManifest::of::<FixtureFeature>().flags[index],
                ))
                .id()
        }
        fn new() -> Self {
            let mut app = App::new();
            app.add_plugins((
                FeaturePlugin,
                crate::manifest::FeaturePlugin::<FixtureFeature>::default(),
            ));
            let source = app.world_mut().query_filtered::<Entity, With<crate::manifest::FeatureManifestOwner<FixtureFeature>>>().single(app.world()).unwrap();
            let scope = app
                .world_mut()
                .spawn((
                    Name::new("Installation flags"),
                    Context {
                        generation: 0,
                        subject: Subject {
                            kind: SubjectKind::Installation,
                            key: "device-a".into(),
                        },
                        attributes: BTreeMap::new(),
                    },
                ))
                .id();
            let provider = app.world_mut().spawn(Provider::bundle("production")).id();
            let flag = app
                .world_mut()
                .spawn(Definition::bundle(
                    provider,
                    source,
                    0,
                    scope,
                    &FeatureManifest::of::<FixtureFeature>().flags[0],
                ))
                .id();
            app.world_mut().write_message(Tick(1000));
            app.update();
            Self {
                app,
                provider,
                flag,
                source,
                scope,
            }
        }

        fn receive(&mut self, revision: u64, enabled: bool) {
            self.app.world_mut().write_message(Receive {
                receipt: None,
                provider: self.provider,
                generation: 0,
                document: Document {
                    schema_version: 1,
                    environment: "production".into(),
                    revision,
                    issued_at: 1000,
                    valid_until: 1100,
                    flags: BTreeMap::from([(
                        "account.cloud".into(),
                        Configuration::boolean(enabled),
                    )]),
                },
                source: Source::Cloud,
            });
        }

        fn enabled(&self) -> bool {
            self.app.world().get::<Current>(self.flag).unwrap().0
        }
    }

    #[test]
    fn canonical_declarations_publish_all_five_typed_decisions() {
        let mut fixture = Fixture::new();
        let string = fixture.bind(2, fixture.scope);
        let integer = fixture.bind(3, fixture.scope);
        let float = fixture.bind(4, fixture.scope);
        let json = fixture.bind(5, fixture.scope);
        let structured = serde_json::json!({"items":[true,"value",1]});
        let mut flags = BTreeMap::new();
        for (key, value) in [
            ("account.cloud", Value::Boolean(true)),
            ("typed.string", Value::String("selected".into())),
            ("typed.integer", Value::Integer(i64::MAX)),
            ("typed.float", Value::Float(1.25)),
            ("typed.json", Value::Json(structured.clone())),
        ] {
            flags.insert(
                key.into(),
                Configuration {
                    value,
                    variant: Some("selected".into()),
                    rules: Vec::new(),
                    experiment_generation: None,
                },
            );
        }
        fixture.app.world_mut().write_message(Receive {
            receipt: None,
            provider: fixture.provider,
            generation: 0,
            source: Source::Cloud,
            document: Document {
                schema_version: 2,
                environment: "production".into(),
                revision: 8,
                issued_at: 1000,
                valid_until: 1100,
                flags,
            },
        });
        fixture.app.update();
        assert!(
            fixture
                .app
                .world()
                .get::<Decision<bool>>(fixture.flag)
                .unwrap()
                .value
        );
        assert_eq!(
            fixture
                .app
                .world()
                .get::<Decision<String>>(string)
                .unwrap()
                .value,
            "selected"
        );
        assert_eq!(
            fixture
                .app
                .world()
                .get::<Decision<i64>>(integer)
                .unwrap()
                .value,
            i64::MAX
        );
        assert_eq!(
            fixture
                .app
                .world()
                .get::<Decision<f64>>(float)
                .unwrap()
                .value,
            1.25
        );
        assert_eq!(
            fixture
                .app
                .world()
                .get::<Decision<serde_json::Value>>(json)
                .unwrap()
                .value,
            structured
        );
        fixture.app.world_mut().write_message(Tick(1100));
        fixture.app.update();
        assert_eq!(
            fixture
                .app
                .world()
                .get::<Decision<String>>(string)
                .unwrap()
                .value,
            "default"
        );
        assert_eq!(
            fixture
                .app
                .world()
                .get::<Decision<i64>>(integer)
                .unwrap()
                .value,
            0
        );
        assert_eq!(
            fixture
                .app
                .world()
                .get::<Decision<f64>>(float)
                .unwrap()
                .value,
            0.0
        );
        assert_eq!(
            fixture
                .app
                .world()
                .get::<Decision<serde_json::Value>>(json)
                .unwrap()
                .value,
            serde_json::json!({})
        );
    }

    #[test]
    fn one_provider_evaluates_distinct_scopes_and_reacts_to_only_the_changed_context() {
        let mut fixture = Fixture::new();
        fixture
            .app
            .world_mut()
            .get_mut::<Context>(fixture.scope)
            .unwrap()
            .attributes
            .insert(Attribute::Platform, Value::String("macos".into()));
        let first = fixture.bind(2, fixture.scope);
        let scope = fixture
            .app
            .world_mut()
            .spawn((
                Name::new("Other scope"),
                Context {
                    generation: 0,
                    subject: Subject {
                        kind: SubjectKind::Installation,
                        key: "device-b".into(),
                    },
                    attributes: BTreeMap::from([(
                        Attribute::Platform,
                        Value::String("linux".into()),
                    )]),
                },
            ))
            .id();
        let second = fixture.bind(2, scope);
        let configuration = Configuration {
            value: Value::String("fallback".into()),
            variant: None,
            experiment_generation: None,
            rules: vec![Rule {
                conditions: vec![Condition {
                    attribute: Attribute::Platform,
                    operator: Operator::Equal,
                    values: vec![Value::String("macos".into())],
                }],
                rollout: None,
                value: Value::String("mac".into()),
                variant: Some("mac".into()),
            }],
        };
        fixture.app.world_mut().write_message(Receive {
            receipt: None,
            provider: fixture.provider,
            generation: 0,
            source: Source::Cloud,
            document: Document {
                schema_version: 2,
                environment: "production".into(),
                revision: 2,
                issued_at: 1000,
                valid_until: 1100,
                flags: BTreeMap::from([("typed.string".into(), configuration)]),
            },
        });
        fixture.app.update();
        assert_eq!(
            fixture
                .app
                .world()
                .get::<Decision<String>>(first)
                .unwrap()
                .value,
            "mac"
        );
        assert_eq!(
            fixture
                .app
                .world()
                .get::<Decision<String>>(second)
                .unwrap()
                .value,
            "fallback"
        );
        {
            let mut context = fixture.app.world_mut().get_mut::<Context>(scope).unwrap();
            context.generation += 1;
            context
                .attributes
                .insert(Attribute::Platform, Value::String("macos".into()));
        }
        fixture.app.update();
        assert_eq!(
            fixture
                .app
                .world()
                .get::<Decision<String>>(second)
                .unwrap()
                .value,
            "mac"
        );
        assert_eq!(
            fixture
                .app
                .world()
                .get::<Decision<String>>(second)
                .unwrap()
                .details
                .context_generation,
            1
        );
        assert_eq!(
            fixture
                .app
                .world()
                .get::<Decision<String>>(first)
                .unwrap()
                .details
                .context_generation,
            0
        );
    }

    #[test]
    fn a_retention_scope_blocks_default_on_until_restore_and_rejects_enable_override() {
        let mut fixture = Fixture::new();
        fixture.app.insert_resource(OverridePolicy::development());
        let live = fixture.bind(1, fixture.scope);
        fixture.app.update();
        assert!(
            !fixture
                .app
                .world()
                .get::<Decision<bool>>(live)
                .unwrap()
                .value
        );
        fixture
            .app
            .world_mut()
            .entity_mut(live)
            .insert(Retention::absent());
        fixture.app.update();
        assert!(
            fixture
                .app
                .world()
                .get::<Decision<bool>>(live)
                .unwrap()
                .value
        );
        fixture
            .app
            .world_mut()
            .entity_mut(live)
            .insert((Retention::off(7), DevelopmentOverride(Value::Boolean(true))));
        fixture.app.update();
        assert!(
            !fixture
                .app
                .world()
                .get::<Decision<bool>>(live)
                .unwrap()
                .value
        );
        fixture
            .app
            .world_mut()
            .entity_mut(live)
            .insert(Retention::failed());
        fixture.app.update();
        assert!(
            !fixture
                .app
                .world()
                .get::<Decision<bool>>(live)
                .unwrap()
                .value
        );
    }

    #[test]
    fn removing_a_scope_or_declaration_removes_stale_typed_admission() {
        for source in [false, true] {
            let mut fixture = Fixture::new();
            fixture.receive(1, true);
            fixture.app.update();
            assert!(
                fixture
                    .app
                    .world()
                    .get::<Decision<bool>>(fixture.flag)
                    .unwrap()
                    .value
            );
            let removed = if source {
                fixture.source
            } else {
                fixture.scope
            };
            fixture.app.world_mut().despawn(removed);
            fixture.app.update();
            assert!(!fixture.enabled());
            assert!(
                fixture
                    .app
                    .world()
                    .get::<Decision<bool>>(fixture.flag)
                    .is_none()
            );
            assert!(
                fixture
                    .app
                    .world()
                    .get::<Decision<Value>>(fixture.flag)
                    .is_none()
            );
            assert!(
                fixture
                    .app
                    .world()
                    .get::<EvaluationFailure>(fixture.flag)
                    .is_some()
            );
        }
    }

    #[test]
    fn application_policy_restricts_overrides_to_development_composition() {
        let mut fixture = Fixture::new();
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.flag)
            .insert(DevelopmentOverride(Value::Boolean(true)));
        fixture.app.update();
        assert!(!fixture.enabled());
        fixture.app.insert_resource(OverridePolicy::development());
        fixture.app.update();
        assert!(fixture.enabled());
        fixture.app.insert_resource(OverridePolicy::default());
        fixture.app.update();
        assert!(!fixture.enabled());
    }

    #[test]
    fn valid_cache_survives_refresh_failure_until_its_existing_expiry() {
        let mut fixture = Fixture::new();
        assert!(!fixture.enabled());
        fixture.receive(1, true);
        fixture.app.update();
        assert!(fixture.enabled());
        fixture.app.world_mut().write_message(RefreshFailed {
            provider: fixture.provider,
            generation: 0,
        });
        fixture.app.update();
        assert!(fixture.enabled());
        fixture.receive(1, true);
        fixture.app.update();
        assert!(fixture.enabled());
        fixture.app.world_mut().write_message(Tick(1100));
        fixture.app.update();
        assert!(!fixture.enabled());
    }

    #[test]
    fn older_and_rewritten_revisions_cannot_reenable_a_disabled_flag() {
        let mut fixture = Fixture::new();
        fixture.receive(2, false);
        fixture.app.update();
        for revision in [1, 2] {
            fixture.receive(revision, true);
            fixture.app.update();
            assert!(!fixture.enabled());
        }
        fixture.receive(3, true);
        fixture.app.update();
        assert!(fixture.enabled());
    }

    #[test]
    fn message_batch_chooses_the_newest_revision_and_keeps_one_source_per_key() {
        let mut fixture = Fixture::new();
        fixture.receive(1, true);
        fixture.receive(3, false);
        fixture.receive(2, true);
        fixture.app.update();
        assert!(!fixture.enabled());
        assert_eq!(
            fixture.app.world().get::<Revision>(fixture.flag).unwrap().0,
            3
        );
        let mut query = fixture.app.world_mut().query::<&Published>();
        assert_eq!(query.iter(fixture.app.world()).count(), 1);
    }

    #[test]
    fn changed_context_and_stale_failures_cannot_publish_old_results() {
        let mut fixture = Fixture::new();
        fixture.receive(1, true);
        fixture.app.update();
        fixture
            .app
            .world_mut()
            .get_mut::<Generation>(fixture.provider)
            .unwrap()
            .0 = 1;
        fixture.receive(2, true);
        fixture.app.update();
        assert!(!fixture.enabled());
        fixture.app.world_mut().write_message(Receive {
            receipt: None,
            provider: fixture.provider,
            generation: 1,
            document: Document {
                schema_version: 1,
                environment: "production".into(),
                revision: 2,
                issued_at: 1000,
                valid_until: 1100,
                flags: BTreeMap::from([("account.cloud".into(), Configuration::boolean(true))]),
            },
            source: Source::Cloud,
        });
        fixture.app.world_mut().write_message(RefreshFailed {
            provider: fixture.provider,
            generation: 0,
        });
        fixture.app.update();
        assert!(fixture.enabled());
    }

    #[test]
    fn late_definitions_adopt_current_values_and_missing_keys_use_defaults() {
        let mut fixture = Fixture::new();
        fixture.receive(1, true);
        fixture.app.update();
        let late = fixture
            .app
            .world_mut()
            .spawn(Definition::bundle(
                fixture.provider,
                fixture.source,
                0,
                fixture.scope,
                &FeatureManifest::of::<FixtureFeature>().flags[0],
            ))
            .id();
        fixture.app.update();
        assert!(fixture.app.world().get::<Current>(late).unwrap().0);
        fixture.app.world_mut().write_message(Receive {
            receipt: None,
            provider: fixture.provider,
            generation: 0,
            document: Document {
                schema_version: 1,
                environment: "production".into(),
                revision: 2,
                issued_at: 1000,
                valid_until: 1100,
                flags: BTreeMap::new(),
            },
            source: Source::Cloud,
        });
        fixture.app.update();
        assert!(!fixture.enabled());
    }

    #[test]
    fn invalid_namespace_does_not_replace_accepted_content() {
        let mut fixture = Fixture::new();
        fixture.receive(1, true);
        fixture.app.update();
        fixture.app.world_mut().write_message(Receive {
            receipt: None,
            provider: fixture.provider,
            generation: 0,
            document: Document {
                schema_version: 1,
                environment: "staging".into(),
                revision: 2,
                issued_at: 1000,
                valid_until: 1100,
                flags: BTreeMap::from([("account.cloud".into(), Configuration::boolean(true))]),
            },
            source: Source::Cloud,
        });
        fixture.app.update();
        assert!(fixture.enabled());
        assert_eq!(
            fixture
                .app
                .world()
                .get::<Revision>(fixture.provider)
                .unwrap()
                .0,
            1
        );
        fixture.receive(1, true);
        fixture.app.update();
        assert!(fixture.enabled());
    }
}
