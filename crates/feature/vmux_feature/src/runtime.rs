use std::collections::BTreeMap;

use bevy_app::{App, Update};
use bevy_ecs::prelude::*;
use vmux_ecs::Revision;

use crate::snapshot::Document;

pub struct FeaturePlugin;

impl bevy_app::Plugin for FeaturePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Now>()
            .add_message::<Tick>()
            .add_message::<Receive>()
            .add_message::<RefreshFailed>()
            .add_message::<Changed>()
            .configure_sets(Update, (Set::Clock, Set::Accept, Set::Evaluate).chain())
            .add_systems(Update, advance.in_set(Set::Clock))
            .add_systems(Update, (accept, fail, expire).chain().in_set(Set::Accept))
            .add_systems(Update, evaluate.in_set(Set::Evaluate));
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Set {
    Clock,
    Accept,
    Evaluate,
}

#[derive(Component)]
#[require(Revision, Generation, Lease)]
pub struct Provider;

impl Provider {
    pub fn bundle(environment: &str) -> impl Bundle {
        (Self, Name::new(environment.to_string()))
    }
}

#[derive(Component)]
#[component(immutable)]
#[require(Current, Revision)]
pub struct Definition {
    pub default: bool,
}

impl Definition {
    pub fn bundle(provider: Entity, key: &str, default: bool) -> impl Bundle {
        (
            Self { default },
            Name::new(key.to_string()),
            ChildOf(provider),
            Current(default),
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

#[derive(Component, Default)]
pub(crate) struct Lease {
    pub(crate) generation: u64,
    pub(crate) issued_at: u64,
    pub(crate) valid_until: u64,
    pub(crate) fingerprint: [u8; 32],
    pub(crate) available: bool,
}

#[derive(Component)]
struct Published(bool);

#[derive(Resource, Default)]
pub(crate) struct Now(pub(crate) u64);

fn advance(mut ticks: MessageReader<Tick>, mut now: ResMut<Now>) {
    for tick in ticks.read() {
        now.0 = now.0.max(tick.0);
    }
}

fn accept(
    mut input: MessageReader<Receive>,
    mut providers: Query<(&Name, &mut Revision, &Generation, &mut Lease), With<Provider>>,
    mut values: Query<(Entity, &Name, &ChildOf, &mut Published), Without<Provider>>,
    now: Res<Now>,
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
        if !input.document.is_valid(environment.as_str(), now.0) {
            lease.available = false;
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
        };
        accepted.insert(input.provider, input);
        commands.entity(input.provider).remove::<Failure>();
    }
    for (provider, input) in accepted {
        let mut remaining = input.document.flags.clone();
        for (entity, name, owner, mut value) in &mut values {
            if owner.parent() != provider {
                continue;
            }
            if let Some(published) = remaining.remove(name.as_str()) {
                value.0 = published;
            } else {
                commands.entity(entity).despawn();
            }
        }
        for (key, value) in remaining {
            commands.spawn((Name::new(key), ChildOf(provider), Published(value)));
        }
    }
}

fn fail(
    mut failures: MessageReader<RefreshFailed>,
    mut providers: Query<(&Generation, &mut Lease), With<Provider>>,
    mut commands: Commands,
) {
    for failure in failures.read() {
        let Ok((generation, mut lease)) = providers.get_mut(failure.provider) else {
            continue;
        };
        if generation.0 != failure.generation {
            continue;
        }
        lease.available = false;
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
        &'static Name,
        &'static ChildOf,
        &'static Definition,
        &'static mut Current,
        &'static mut Revision,
    ),
    Without<Provider>,
>;

fn evaluate(
    providers: Query<(&Revision, &Lease), With<Provider>>,
    published: Query<(&Name, &ChildOf, &Published)>,
    mut definitions: Definitions,
    mut changes: MessageWriter<Changed>,
) {
    for (entity, name, owner, definition, mut current, mut revision) in &mut definitions {
        let mut enabled = definition.default;
        let mut accepted_revision = 0;
        if let Ok((provider_revision, lease)) = providers.get(owner.parent()) {
            accepted_revision = provider_revision.0;
            if lease.available {
                for (key, source, value) in &published {
                    if source.parent() == owner.parent() && key.as_str() == name.as_str() {
                        enabled = value.0;
                        break;
                    }
                }
            }
        }
        if current.0 != enabled || revision.0 != accepted_revision {
            current.0 = enabled;
            revision.0 = accepted_revision;
            changes.write(Changed {
                entity,
                enabled,
                revision: accepted_revision,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture {
        app: App,
        provider: Entity,
        flag: Entity,
    }

    impl Fixture {
        fn new() -> Self {
            let mut app = App::new();
            app.add_plugins(FeaturePlugin);
            let provider = app.world_mut().spawn(Provider::bundle("production")).id();
            let flag = app
                .world_mut()
                .spawn(Definition::bundle(provider, "account.cloud", false))
                .id();
            app.world_mut().write_message(Tick(1000));
            app.update();
            Self {
                app,
                provider,
                flag,
            }
        }

        fn receive(&mut self, revision: u64, enabled: bool) {
            self.app.world_mut().write_message(Receive {
                provider: self.provider,
                generation: 0,
                document: Document {
                    schema_version: 1,
                    environment: "production".into(),
                    revision,
                    issued_at: 1000,
                    valid_until: 1100,
                    flags: BTreeMap::from([("account.cloud".into(), enabled)]),
                },
            });
        }

        fn enabled(&self) -> bool {
            self.app.world().get::<Current>(self.flag).unwrap().0
        }
    }

    #[test]
    fn unavailable_expired_and_failed_providers_close_admission() {
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
        assert!(!fixture.enabled());
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
            provider: fixture.provider,
            generation: 1,
            document: Document {
                schema_version: 1,
                environment: "production".into(),
                revision: 2,
                issued_at: 1000,
                valid_until: 1100,
                flags: BTreeMap::from([("account.cloud".into(), true)]),
            },
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
            .spawn(Definition::bundle(fixture.provider, "account.cloud", false))
            .id();
        fixture.app.update();
        assert!(fixture.app.world().get::<Current>(late).unwrap().0);
        fixture.app.world_mut().write_message(Receive {
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
            provider: fixture.provider,
            generation: 0,
            document: Document {
                schema_version: 1,
                environment: "staging".into(),
                revision: 2,
                issued_at: 1000,
                valid_until: 1100,
                flags: BTreeMap::from([("account.cloud".into(), true)]),
            },
        });
        fixture.app.update();
        assert!(!fixture.enabled());
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
