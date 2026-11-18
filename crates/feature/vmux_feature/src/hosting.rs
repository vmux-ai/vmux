use bevy_app::{App, Plugin, Startup};
use bevy_ecs::prelude::*;
use serde::Deserialize;
use vmux_api::{BinEvent, PageReady, UiEvent};
use vmux_ecs::{EffectiveStartupUrl, Ready};

use crate::manifest::{
    FeatureManifest, FeatureManifestOwner, FeatureManifestSource, FeaturePlugin,
};

pub struct WebHostPlugin {
    contract: Contract,
    package: Package,
}

impl WebHostPlugin {
    pub fn new(contract: Contract, package: Package) -> Self {
        Self { contract, package }
    }
}

impl Plugin for WebHostPlugin {
    fn build(&self, app: &mut App) {
        assert!(
            !app.world().contains_resource::<Contract>(),
            "web host contract is already selected"
        );
        app.insert_resource(self.contract.clone())
            .add_plugins(FeaturePlugin::<Namespace>::from_manifest(
                self.package.manifest.clone(),
            ))
            .add_observer(validate)
            .add_message::<HostedApplicationReady>()
            .add_systems(Startup, publish);
        let world = app.world_mut();
        let entity = world
            .query_filtered::<Entity, With<FeatureManifestOwner<Namespace>>>()
            .single(world)
            .expect("one web application manifest");
        world.entity_mut(entity).insert((
            HostedApplication,
            Header {
                schema_version: self.package.schema_version,
                sdk_revision: self.package.sdk_revision.clone(),
                entry: self.package.entry.clone(),
            },
        ));
    }

    fn finish(&self, app: &mut App) {
        let world = app.world_mut();
        let entity = world
            .query_filtered::<Entity, With<HostedApplication>>()
            .single(world)
            .expect("one web application");
        world.trigger(Validate { entity });
        world.flush();
        if let Some(failure) = world.get::<Failure>(entity) {
            panic!("invalid prebuilt host package: {}", failure.0);
        }
        assert!(
            world.get::<Ready>(entity).is_some(),
            "web package validation must finish"
        );
    }
}

#[derive(Clone, Debug, Resource)]
pub struct Contract {
    sdk_revision: String,
    events: Vec<String>,
}

impl Contract {
    pub fn new(sdk_revision: impl Into<String>) -> Self {
        Self {
            sdk_revision: sdk_revision.into(),
            events: vec![PageReady::PERMISSION.into()],
        }
    }

    pub fn event<T: UiEvent>(mut self) -> Self {
        if !self.events.iter().any(|event| event == T::PERMISSION) {
            self.events.push(T::PERMISSION.into());
        }
        self
    }

    pub fn revision(&self) -> &str {
        &self.sdk_revision
    }
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Package {
    pub schema_version: u16,
    pub sdk_revision: String,
    pub entry: String,
    pub manifest: FeatureManifest,
}

impl Package {
    pub fn try_parse(source: &str) -> Result<Self, String> {
        if source.len() > 1024 * 1024 {
            return Err("web application package exceeds 1 MiB".into());
        }
        ron::from_str(source).map_err(|error| error.to_string())
    }
}

#[derive(Component)]
pub struct HostedApplication;

#[derive(Message, Clone, Copy)]
pub struct HostedApplicationReady {
    pub entity: Entity,
}

struct Namespace;

impl FeatureManifestSource for Namespace {
    const SOURCE: &'static str = "()";
}

#[derive(Component)]
struct Header {
    schema_version: u16,
    sdk_revision: String,
    entry: String,
}

impl Header {
    fn check(&self, contract: &Contract, manifest: &FeatureManifest) -> Result<(), String> {
        if self.schema_version != 1 {
            return Err(format!(
                "unsupported package schema {}",
                self.schema_version
            ));
        }
        for revision in [&self.sdk_revision, &contract.sdk_revision] {
            if revision.len() != 40 || !revision.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return Err("host and package need an immutable Git revision".into());
            }
        }
        if self.sdk_revision != contract.sdk_revision {
            return Err("package SDK revision does not match the host".into());
        }
        let metadata = manifest
            .metadata
            .as_ref()
            .ok_or("package needs feature metadata")?;
        if !metadata.provides.is_empty()
            || manifest.root
            || !manifest.flags.is_empty()
            || !manifest.auth_providers.is_empty()
            || !manifest.commands.is_empty()
            || !manifest.tools.is_empty()
            || manifest.cli.is_some()
            || !manifest.mcp_servers.is_empty()
            || !manifest.command_bar.is_empty()
            || !manifest.search_engines.is_empty()
            || !manifest.tool_providers.is_empty()
        {
            return Err("native contributions must come from linked feature plugins".into());
        }
        let mut entry = false;
        for page in &manifest.pages {
            let url = url::Url::parse(&page.url).map_err(|error| error.to_string())?;
            if !matches!(url.scheme(), "http" | "https")
                || !url.username().is_empty()
                || url.password().is_some()
            {
                return Err("web package pages need credential-free HTTP(S) URLs".into());
            }
            if !page.asset_host.is_empty() || !page.replaces_command.is_empty() {
                return Err(
                    "web package cannot claim embedded asset hosts or native command replacements"
                        .into(),
                );
            }
            if page.url == self.entry && page.startup {
                entry = true;
            } else if page.startup {
                return Err("web package has an unexpected startup page".into());
            }
            for permission in &page.permissions {
                if !contract.events.contains(permission) {
                    return Err(format!("host does not provide page event {permission}"));
                }
            }
        }
        if !entry {
            return Err("package entry must name its declared startup page".into());
        }
        Ok(())
    }
}

#[derive(EntityEvent)]
struct Validate {
    entity: Entity,
}

#[derive(Component)]
struct Failure(String);

#[derive(Component)]
struct Published;

fn validate(
    event: On<Validate>,
    contract: Res<Contract>,
    packages: Query<(&Header, &FeatureManifest), With<HostedApplication>>,
    mut commands: Commands,
) {
    let Ok((header, manifest)) = packages.get(event.entity) else {
        return;
    };
    let mut entity = commands.entity(event.entity);
    match header.check(&contract, manifest) {
        Ok(()) => {
            entity.insert((Ready, EffectiveStartupUrl(header.entry.clone())));
        }
        Err(error) => {
            entity.insert(Failure(error));
        }
    }
    entity.remove::<Header>();
}

type Applications<'w, 's> =
    Query<'w, 's, Entity, (With<HostedApplication>, With<Ready>, Without<Published>)>;

fn publish(
    applications: Applications,
    mut ready: MessageWriter<HostedApplicationReady>,
    mut commands: Commands,
) {
    for entity in &applications {
        ready.write(HostedApplicationReady { entity });
        commands.entity(entity).insert(Published);
    }
}
