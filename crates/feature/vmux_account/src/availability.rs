use bevy_app::{App, Update};
use bevy_ecs::prelude::*;
use vmux_feature::manifest::{FeatureManifest, FeatureManifestOwner};
use vmux_feature::{Current, Definition, InstallationScope, ReleaseProvider};

use crate::{CloudAvailable, Connection, Root};

pub(crate) struct AvailabilityPlugin;

impl bevy_app::Plugin for AvailabilityPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, define.before(vmux_feature::Set::Accept))
            .add_systems(
                Update,
                (evaluate, cancel)
                    .chain()
                    .after(vmux_feature::Set::Publish)
                    .before(crate::connection::Flow::Intent),
            );
    }
}

#[derive(Component)]
pub struct AccountCloudFlag;

#[derive(Component)]
pub(crate) struct SecureCredentials;

fn define(
    providers: Query<Entity, With<ReleaseProvider>>,
    scopes: Query<Entity, With<InstallationScope>>,
    sources: Query<(Entity, &FeatureManifest), With<FeatureManifestOwner<crate::Feature>>>,
    flags: Query<(), With<AccountCloudFlag>>,
    mut commands: Commands,
) {
    if !flags.is_empty() {
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
    let (index, declaration) = manifest
        .flag("account.cloud")
        .expect("Account cloud declaration");
    commands.spawn((
        AccountCloudFlag,
        Definition::bundle(provider, source, index, scope, declaration),
    ));
}

fn evaluate(
    flags: Query<&Current, With<AccountCloudFlag>>,
    roots: Query<(Entity, Has<SecureCredentials>, Has<CloudAvailable>), With<Root>>,
    mut commands: Commands,
) {
    let enabled = flags.single().is_ok_and(|current| current.0);
    for (root, credentials, available) in &roots {
        if enabled && credentials {
            if !available {
                commands.entity(root).insert(CloudAvailable);
            }
        } else if available {
            commands.entity(root).remove::<CloudAvailable>();
        }
    }
}

fn cancel(
    roots: Query<(), (With<Root>, With<CloudAvailable>)>,
    connections: Query<(Entity, &ChildOf), With<Connection>>,
    mut commands: Commands,
) {
    for (connection, parent) in &connections {
        if !roots.contains(parent.parent()) {
            commands.entity(connection).despawn();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use vmux_feature::flag::{Configuration, Context, Source};
    use vmux_feature::snapshot::Document;
    use vmux_feature::{FeaturePlugin, Provider, Receive, RefreshFailed, Tick};

    use super::*;

    struct Fixture {
        app: App,
        provider: Entity,
        root: Entity,
    }

    impl Fixture {
        fn new(supported: bool) -> Self {
            let mut app = App::new();
            app.add_plugins((FeaturePlugin, crate::AccountPlugin, AvailabilityPlugin));
            let provider = app
                .world_mut()
                .spawn((Provider::bundle("production"), ReleaseProvider))
                .id();
            app.world_mut().spawn((
                Name::new("Installation flags"),
                InstallationScope,
                Context::installation("device-a"),
            ));
            let root = app.world_mut().spawn(Root).id();
            if supported {
                app.world_mut().entity_mut(root).insert(SecureCredentials);
            }
            app.world_mut().write_message(Tick(1000));
            app.update();
            Self {
                app,
                provider,
                root,
            }
        }

        fn decision(&mut self, revision: u64, enabled: bool) {
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
            self.app.update();
        }

        fn available(&self) -> bool {
            self.app.world().get::<CloudAvailable>(self.root).is_some()
        }
    }

    #[test]
    fn initial_off_and_expiry_close_admission_without_creating_a_connection() {
        let mut fixture = Fixture::new(true);
        assert!(!fixture.available());
        fixture.decision(1, true);
        assert!(fixture.available());
        fixture.app.world_mut().write_message(Tick(1100));
        fixture.app.update();
        assert!(!fixture.available());
        let mut connections = fixture
            .app
            .world_mut()
            .query_filtered::<Entity, With<Connection>>();
        assert_eq!(connections.iter(fixture.app.world()).count(), 0);
    }

    #[test]
    fn current_off_and_cache_expiry_cancel_active_work_but_keep_local_records() {
        let mut fixture = Fixture::new(true);
        fixture.decision(1, true);
        let local = fixture
            .app
            .world_mut()
            .spawn((crate::Profile, ChildOf(fixture.root)))
            .id();
        let connection = fixture
            .app
            .world_mut()
            .spawn((Connection, ChildOf(fixture.root)))
            .id();
        fixture.decision(2, false);
        assert!(!fixture.available());
        assert!(fixture.app.world().get_entity(connection).is_err());
        assert!(fixture.app.world().get_entity(local).is_ok());
        fixture.decision(3, true);
        let connection = fixture
            .app
            .world_mut()
            .spawn((Connection, ChildOf(fixture.root)))
            .id();
        fixture.app.world_mut().write_message(RefreshFailed {
            provider: fixture.provider,
            generation: 0,
        });
        fixture.app.update();
        assert!(fixture.available());
        assert!(fixture.app.world().get_entity(connection).is_ok());
        fixture.app.world_mut().write_message(Tick(1100));
        fixture.app.update();
        assert!(!fixture.available());
        assert!(fixture.app.world().get_entity(connection).is_err());
        assert!(fixture.app.world().get_entity(local).is_ok());
    }

    #[test]
    fn a_cloud_on_never_bypasses_the_platform_credential_capability() {
        let mut fixture = Fixture::new(false);
        fixture.decision(1, true);
        assert!(!fixture.available());
    }
}
