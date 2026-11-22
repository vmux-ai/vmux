use bevy_app::{App, Startup};
use bevy_ecs::prelude::*;
use vmux_feature::http::{Endpoint, HttpPlugin};
use vmux_feature::{FeaturePlugin, InstallationScope, Provider, ReleaseProvider};

mod identity;
mod identity_io;

#[derive(SystemSet, Clone, Debug, PartialEq, Eq, Hash)]
struct Configured;

pub(crate) struct ReleaseFlagsPlugin;

#[derive(Resource, Default)]
pub(crate) struct WriterRole(Role);

#[derive(Default)]
enum Role {
    #[default]
    Desktop,
    #[cfg(any(feature = "daemon", test))]
    Daemon,
}

impl WriterRole {
    #[cfg(any(feature = "daemon", test))]
    pub(crate) fn daemon() -> Self {
        Self(Role::Daemon)
    }

    fn is_daemon(&self) -> bool {
        match self.0 {
            Role::Desktop => false,
            #[cfg(any(feature = "daemon", test))]
            Role::Daemon => true,
        }
    }

    fn filename(&self) -> &'static str {
        match self.0 {
            Role::Desktop => "desktop.json",
            #[cfg(any(feature = "daemon", test))]
            Role::Daemon => "daemon.json",
        }
    }
}

#[derive(Component)]
struct FlagCache;

impl bevy_app::Plugin for ReleaseFlagsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WriterRole>()
            .add_plugins((
                FeaturePlugin,
                HttpPlugin,
                vmux_feature::cache::CachePlugin,
                identity::IdentityPlugin,
            ))
            .add_systems(Startup, configure_release.in_set(Configured))
            .add_systems(
                bevy_app::Update,
                bind_cache
                    .after(vmux_feature::Set::Clock)
                    .after(vmux_api::control::ControlCredentialSet)
                    .before(vmux_feature::cache::Bind),
            );
        #[cfg(feature = "account-cloud")]
        app.add_systems(
            bevy_app::Update,
            bind_credentials
                .after(vmux_feature::Set::Clock)
                .before(vmux_feature::Set::Prepare),
        );
    }
}

fn configure_release(role: Res<WriterRole>, mut commands: Commands) {
    if matches!(vmux_profile::build_profile(), "dev" | "local") {
        commands.insert_resource(vmux_feature::OverridePolicy::development());
    }
    let mut environment = "production".to_string();
    let mut endpoint = Some(
        Endpoint::new("https://api.vmux.ai/v1/flags/snapshot").expect("compiled release endpoint"),
    );
    if matches!(vmux_profile::build_profile(), "dev" | "local")
        && let Ok(url) = std::env::var("VMUX_RELEASE_FLAGS_ENDPOINT")
    {
        environment = std::env::var("VMUX_RELEASE_FLAGS_ENVIRONMENT")
            .unwrap_or_else(|_| "development".into());
        endpoint = None;
        if url.starts_with("http://") {
            endpoint = Endpoint::new(&url).ok();
        }
    }
    let provider = commands
        .spawn((
            Provider::bundle(&environment),
            ReleaseProvider,
            vmux_feature::http::RequireAuthorization,
        ))
        .id();
    if let Some(endpoint) = endpoint {
        commands.entity(provider).insert(endpoint);
    }
    commands.spawn((Name::new("Installation flags"), InstallationScope));
    let path = vmux_profile::ProfilePaths::current()
        .application_data()
        .join("flags")
        .join(role.filename());
    commands.spawn((
        Name::new("Feature cache"),
        FlagCache,
        vmux_feature::cache::Store,
        vmux_feature::cache::Location(path),
    ));
}

#[derive(bevy_ecs::system::SystemParam)]
struct CacheContext<'w, 's> {
    role: Res<'w, WriterRole>,
    credentials: Query<'w, 's, &'static vmux_feature::http::Credential>,
    #[cfg(feature = "account-cloud")]
    roots: Query<'w, 's, (Entity, &'static vmux_account::Server), With<vmux_account::Root>>,
    #[cfg(feature = "account-cloud")]
    accounts: Query<
        'w,
        's,
        (&'static ChildOf, &'static vmux_account::Binding),
        With<vmux_account::Account>,
    >,
}

impl CacheContext<'_, '_> {
    fn current(
        &self,
        environment: &str,
        endpoint: Option<&Endpoint>,
        source: Option<&vmux_feature::http::CredentialSource>,
    ) -> (String, Option<String>, Option<String>) {
        let credential = source.and_then(|source| self.credentials.get(source.0).ok());
        if self.role.is_daemon() {
            let Some(credential) = credential else {
                return ("anonymous".into(), None, None);
            };
            if credential.environment() != environment {
                return ("anonymous".into(), None, None);
            }
            let Some(endpoint) = endpoint else {
                return ("anonymous".into(), None, None);
            };
            let account = credential.account().to_string();
            let workspace = credential.workspace().to_string();
            return (
                format!("{}/{}/{}", endpoint.as_str(), account, workspace),
                Some(account),
                Some(workspace),
            );
        }
        #[cfg(feature = "account-cloud")]
        if let Ok((root, server)) = self.roots.single() {
            for (owner, binding) in &self.accounts {
                if owner.parent() != root || binding.server != server.authority() {
                    continue;
                }
                let Some(account) = &binding.account_id else {
                    continue;
                };
                if !uuid::Uuid::parse_str(account).is_ok_and(|id| !id.is_nil()) {
                    continue;
                }
                let mut workspace = None;
                if let Some(credential) = credential
                    && credential.account().to_string() == *account
                    && credential.environment() == environment
                {
                    workspace = Some(credential.workspace().to_string());
                }
                return (
                    format!("{}/{}", server.authority(), account),
                    Some(account.clone()),
                    workspace,
                );
            }
        }
        ("anonymous".into(), None, None)
    }
}

type CacheProviders<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Name,
        Option<&'static Endpoint>,
        Option<&'static vmux_feature::http::CredentialSource>,
        Option<&'static vmux_feature::cache::Binding>,
    ),
    With<ReleaseProvider>,
>;

fn bind_cache(
    providers: CacheProviders,
    stores: Query<Entity, With<FlagCache>>,
    scopes: Query<(Entity, &vmux_feature::flag::Context), With<InstallationScope>>,
    identity: CacheContext,
    mut commands: Commands,
) {
    let Ok(store) = stores.single() else {
        return;
    };
    let Ok((scope, context)) = scopes.single() else {
        return;
    };
    if context.validate().is_err() {
        return;
    }
    let Ok((provider, name, endpoint, source, existing)) = providers.single() else {
        return;
    };
    let (identity, account, workspace) = identity.current(name.as_str(), endpoint, source);
    let mut updated = context.clone();
    for (attribute, value) in [
        (vmux_feature::flag::Attribute::Account, account),
        (vmux_feature::flag::Attribute::Workspace, workspace),
    ] {
        match value {
            Some(value) => {
                updated
                    .attributes
                    .insert(attribute, vmux_feature::flag::Value::String(value));
            }
            None => {
                updated.attributes.remove(&attribute);
            }
        }
    }
    if updated.attributes != context.attributes {
        updated.generation = context.generation.saturating_add(1);
        commands.entity(scope).insert(updated);
    }
    let binding = vmux_feature::cache::Binding {
        store,
        identity,
        installation: context.subject.key.clone(),
    };
    if existing != Some(&binding) {
        commands.entity(provider).insert(binding);
    }
}

#[cfg(feature = "account-cloud")]
fn bind_credentials(
    providers: Query<
        (Entity, &Name, Option<&vmux_feature::http::CredentialSource>),
        With<ReleaseProvider>,
    >,
    roots: Query<Entity, With<vmux_account::Root>>,
    brokers: Query<(Entity, &ChildOf, &vmux_account::control::ControlBroker)>,
    mut commands: Commands,
) {
    let Ok(root) = roots.single() else {
        return;
    };
    for (provider, name, existing) in &providers {
        let mut selected = None;
        for (entity, owner, broker) in &brokers {
            if owner.parent() == root && broker.environment == name.as_str() {
                selected = Some(entity);
                break;
            }
        }
        let source = selected.unwrap_or_else(|| {
            commands
                .spawn((
                    Name::new("Feature authorization"),
                    vmux_account::control::ControlBroker {
                        environment: name.as_str().into(),
                    },
                    ChildOf(root),
                ))
                .id()
        });
        if existing.is_none_or(|existing| existing.0 != source) {
            commands
                .entity(provider)
                .insert(vmux_feature::http::CredentialSource(source));
        }
    }
}

#[cfg(feature = "feature-flags")]
pub(crate) struct FeatureFlagsPlugin;

#[cfg(feature = "feature-flags")]
impl bevy_app::Plugin for FeatureFlagsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(vmux_feature::registration::FeaturePagePlugin);
        #[cfg(target_os = "macos")]
        app.add_systems(Startup, configure_administration.after(configure_release));
    }
}

#[cfg(all(feature = "feature-flags", target_os = "macos"))]
fn configure_administration(
    mut commands: Commands,
    providers: Query<(Entity, &Name, Option<&Endpoint>), With<Provider>>,
) {
    let public =
        vmux_feature::registration::Endpoint::new("https://api.vmux.ai/", "production", None)
            .expect("static public endpoint");
    let snapshot =
        Endpoint::new("https://api.vmux.ai/v1/flags/snapshot").expect("static snapshot endpoint");
    let mut production = None;
    for (entity, name, endpoint) in &providers {
        if name.as_str() == "production" && endpoint == Some(&snapshot) {
            production = Some(entity);
            break;
        }
    }
    match production {
        Some(entity) => {
            commands.entity(entity).insert(public);
        }
        None => {
            commands.spawn((Provider::bundle("production"), public, snapshot));
        }
    }
    if !matches!(vmux_profile::build_profile(), "dev" | "local") {
        return;
    }
    let Ok(url) = std::env::var("VMUX_FEATURE_ADMIN_ENDPOINT") else {
        return;
    };
    let credential = std::env::var("VMUX_FEATURE_ADMIN_TOKEN").ok();
    let environment =
        std::env::var("VMUX_FEATURE_ADMIN_ENVIRONMENT").unwrap_or_else(|_| "development".into());
    let endpoint = match vmux_feature::registration::Endpoint::new(&url, &environment, credential) {
        Ok(endpoint) if endpoint.is_loopback() && environment != "production" => endpoint,
        _ => {
            tracing::warn!("local flag administration endpoint rejected");
            return;
        }
    };
    let snapshot_url = format!("{}/v1/flags/snapshot", url.trim_end_matches('/'));
    let snapshot = match Endpoint::new(&snapshot_url) {
        Ok(endpoint) => endpoint,
        Err(_) => {
            tracing::warn!("local flag snapshot endpoint rejected");
            return;
        }
    };
    for (entity, name, existing) in &providers {
        if name.as_str() != environment {
            continue;
        }
        if existing != Some(&snapshot) {
            tracing::warn!("local flag administration differs from the release snapshot endpoint");
            return;
        }
        commands.entity(entity).insert(endpoint);
        return;
    }
    commands.spawn((Provider::bundle(&environment), endpoint, snapshot));
}

#[cfg(test)]
mod cache_context_tests {
    use super::*;
    use vmux_ecs::Revision;
    use vmux_feature::flag::{Attribute, Context, Value};
    use vmux_feature::http::{Credential, CredentialSource};

    #[test]
    fn daemon_cache_context_tracks_its_own_credential_replacement_and_withdrawal() {
        let directory = tempfile::tempdir().unwrap();
        let mut app = App::new();
        app.insert_resource(WriterRole::daemon());
        app.add_plugins(ReleaseFlagsPlugin);
        app.update();
        let world = app.world_mut();
        let provider = world
            .query_filtered::<Entity, With<ReleaseProvider>>()
            .single(world)
            .unwrap();
        let scope = world
            .query_filtered::<Entity, With<InstallationScope>>()
            .single(world)
            .unwrap();
        let cache = world
            .query_filtered::<Entity, With<FlagCache>>()
            .single(world)
            .unwrap();
        world.entity_mut(scope).insert((
            identity::Location(directory.path().join("installation-id")),
            Context::installation("10000000-0000-0000-0000-000000000001"),
        ));
        world
            .entity_mut(cache)
            .insert(vmux_feature::cache::Location(
                directory.path().join("daemon.json"),
            ));
        world.entity_mut(provider).remove::<Endpoint>();
        let endpoint = Endpoint::new("http://127.0.0.1:9/v1/flags/snapshot").unwrap();
        world.entity_mut(provider).insert(endpoint.clone());
        let account = uuid::Uuid::new_v4();
        let workspace = uuid::Uuid::new_v4();
        let source = world
            .spawn((
                Name::new("Accepted daemon credential"),
                Credential::new(
                    "fixture-control-token".into(),
                    account,
                    uuid::Uuid::new_v4(),
                    workspace,
                    "production".into(),
                    u64::MAX,
                )
                .unwrap(),
                Revision(1),
            ))
            .id();
        world.entity_mut(provider).insert(CredentialSource(source));
        app.update();
        let context = app.world().get::<Context>(scope).unwrap();
        assert_eq!(
            context.attributes.get(&Attribute::Account),
            Some(&Value::String(account.to_string()))
        );
        assert_eq!(
            context.attributes.get(&Attribute::Workspace),
            Some(&Value::String(workspace.to_string()))
        );
        let first = app
            .world()
            .get::<vmux_feature::cache::Binding>(provider)
            .unwrap()
            .identity
            .clone();
        let replacement = uuid::Uuid::new_v4();
        app.world_mut().entity_mut(source).insert((
            Credential::new(
                "replacement-control-token".into(),
                replacement,
                uuid::Uuid::new_v4(),
                workspace,
                "production".into(),
                u64::MAX,
            )
            .unwrap(),
            Revision(2),
        ));
        app.update();
        let context = app.world().get::<Context>(scope).unwrap();
        assert_eq!(
            context.attributes.get(&Attribute::Account),
            Some(&Value::String(replacement.to_string()))
        );
        assert_ne!(
            app.world()
                .get::<vmux_feature::cache::Binding>(provider)
                .unwrap()
                .identity,
            first
        );
        app.world_mut().entity_mut(source).remove::<Credential>();
        app.update();
        let context = app.world().get::<Context>(scope).unwrap();
        assert!(!context.attributes.contains_key(&Attribute::Account));
        assert!(!context.attributes.contains_key(&Attribute::Workspace));
        assert_eq!(
            app.world()
                .get::<vmux_feature::cache::Binding>(provider)
                .unwrap()
                .identity,
            "anonymous"
        );
    }
}
