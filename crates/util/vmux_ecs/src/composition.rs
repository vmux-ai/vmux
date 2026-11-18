use bevy_app::{App, Plugin};
use bevy_ecs::prelude::*;
use bevy_ecs::schedule::{ScheduleLabel, Schedules};
use bevy_ecs::system::SystemParam;
use semver::{Version, VersionReq};

use crate::feature_metadata::Metadata;
use crate::manifest::FeatureManifest;

#[derive(Default)]
pub struct CompositionPlugin(pub Profile);

impl Plugin for CompositionPlugin {
    fn build(&self, app: &mut App) {
        app.init_schedule(ValidationSchedule)
            .add_systems(ValidationSchedule, validate);
        app.world_mut()
            .spawn((Name::new("Application composition"), self.0.clone()));
    }

    fn finish(&self, app: &mut App) {
        app.world_mut().run_schedule(ValidationSchedule);
        app.world_mut()
            .resource_mut::<Schedules>()
            .remove(ValidationSchedule);
    }
}

#[derive(Clone, Debug, Component)]
pub struct Profile {
    target: String,
    capabilities: Vec<String>,
    permissions: Vec<String>,
}

impl Profile {
    pub fn new(target: impl Into<String>) -> Self {
        Self {
            target: target.into(),
            capabilities: Vec::new(),
            permissions: Vec::new(),
        }
    }

    pub fn capability(mut self, capability: impl Into<String>) -> Self {
        self.capabilities.push(capability.into());
        self
    }

    pub fn permission(mut self, permission: impl Into<String>) -> Self {
        self.permissions.push(permission.into());
        self
    }

    pub fn target(&self) -> &str {
        &self.target
    }

    fn validate(&self) -> Result<(), String> {
        Metadata::identifier(&self.target)?;
        for values in [&self.capabilities, &self.permissions] {
            if values.len() > 256 {
                return Err("composition profile exceeds metadata limits".into());
            }
            for value in values {
                Metadata::identifier(value)?;
            }
        }
        Ok(())
    }
}

impl Default for Profile {
    fn default() -> Self {
        Self::new("headless")
    }
}

#[derive(SystemParam)]
struct Features<'w, 's> {
    manifests: Query<'w, 's, &'static FeatureManifest>,
}

impl Features<'_, '_> {
    fn metadata(&self, id: &str) -> Option<&Metadata> {
        for manifest in &self.manifests {
            if let Some(metadata) = &manifest.metadata
                && metadata.id == id
            {
                return Some(metadata);
            }
        }
        None
    }

    fn provides(&self, capability: &str) -> bool {
        for manifest in &self.manifests {
            if let Some(metadata) = &manifest.metadata
                && metadata.provides.iter().any(|value| value == capability)
            {
                return true;
            }
        }
        false
    }

    fn acyclic(&self, ids: &std::collections::BTreeSet<&str>) -> Result<(), String> {
        let mut ready = std::collections::BTreeSet::new();
        while ready.len() < ids.len() {
            let before = ready.len();
            for id in ids {
                let metadata = self.metadata(id).expect("feature IDs are registered");
                if metadata
                    .dependencies
                    .iter()
                    .all(|dependency| ready.contains(dependency.id.as_str()))
                {
                    ready.insert(*id);
                }
            }
            if before == ready.len() {
                for id in ids {
                    if !ready.contains(id) {
                        return Err(format!("cyclic feature dependencies include {id}"));
                    }
                }
            }
        }
        Ok(())
    }
}

impl Features<'_, '_> {
    fn check(&self, profile: &Profile) -> Result<(), String> {
        let sdk = Version::parse(env!("CARGO_PKG_VERSION")).expect("SDK has a package version");
        let mut ids = std::collections::BTreeSet::new();
        for manifest in &self.manifests {
            let Some(metadata) = &manifest.metadata else {
                continue;
            };
            metadata.validate()?;
            if !ids.insert(metadata.id.as_str()) {
                return Err(format!("duplicate feature {}", metadata.id));
            }
        }
        for id in &ids {
            let metadata = self.metadata(id).expect("feature IDs are registered");
            if let Some(required) = &metadata.sdk {
                let requirement = VersionReq::parse(required).map_err(|error| error.to_string())?;
                if !requirement.matches(&sdk) {
                    return Err(format!(
                        "feature {} requires SDK {required}, running {sdk}",
                        metadata.id
                    ));
                }
            }
            if let Some(bevy) = &metadata.bevy
                && bevy != "0.19"
            {
                return Err(format!(
                    "feature {} requires Bevy {bevy}, running 0.19",
                    metadata.id
                ));
            }
            if !metadata.targets.is_empty() && !metadata.targets.contains(&profile.target) {
                return Err(format!(
                    "feature {} does not support target {}",
                    metadata.id, profile.target
                ));
            }
            for dependency in &metadata.dependencies {
                let Some(found) = self.metadata(&dependency.id) else {
                    return Err(format!(
                        "feature {} requires missing feature {}",
                        metadata.id, dependency.id
                    ));
                };
                let version = Version::parse(&found.version).map_err(|error| error.to_string())?;
                let requirement =
                    VersionReq::parse(&dependency.version).map_err(|error| error.to_string())?;
                if !requirement.matches(&version) {
                    return Err(format!(
                        "feature {} requires {} {}, found {}",
                        metadata.id, dependency.id, dependency.version, found.version
                    ));
                }
            }
            for capability in &metadata.capabilities {
                if !profile.capabilities.contains(capability) && !self.provides(capability) {
                    return Err(format!(
                        "feature {} requires capability {capability}",
                        metadata.id
                    ));
                }
            }
            for permission in &metadata.permissions {
                if !profile.permissions.contains(permission) {
                    return Err(format!(
                        "feature {} requires host policy permission {permission}",
                        metadata.id
                    ));
                }
            }
        }
        self.acyclic(&ids)
    }
}

#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash)]
struct ValidationSchedule;

fn validate(features: Features, profiles: Query<&Profile>) {
    let profile = profiles
        .single()
        .expect("one application composition profile is required");
    profile
        .validate()
        .unwrap_or_else(|error| panic!("invalid SDK composition: {error}"));
    features
        .check(profile)
        .unwrap_or_else(|error| panic!("invalid SDK composition: {error}"));
}
