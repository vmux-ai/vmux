use semver::{Version, VersionReq};
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Metadata {
    pub id: String,
    pub version: String,
    #[serde(default)]
    pub dependencies: Vec<Dependency>,
    #[serde(default)]
    pub provides: Vec<String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default)]
    pub targets: Vec<String>,
    #[serde(default)]
    pub sdk: Option<String>,
    #[serde(default)]
    pub bevy: Option<String>,
    #[serde(default)]
    pub lifecycle: Lifecycle,
}

impl Metadata {
    pub(crate) fn validate(&self) -> Result<(), String> {
        Self::identifier(&self.id)?;
        Version::parse(&self.version)
            .map_err(|error| format!("feature {} has an invalid version: {error}", self.id))?;
        if let Some(version) = &self.sdk {
            VersionReq::parse(version).map_err(|error| {
                format!(
                    "feature {} has an invalid SDK requirement: {error}",
                    self.id
                )
            })?;
        }
        for values in [
            &self.provides,
            &self.capabilities,
            &self.permissions,
            &self.targets,
            &self.lifecycle.startup,
            &self.lifecycle.shutdown,
        ] {
            if values.len() > 256 {
                return Err(format!("feature {} exceeds metadata limits", self.id));
            }
            let mut seen = std::collections::BTreeSet::new();
            for value in values {
                Self::identifier(value)?;
                if !seen.insert(value) {
                    return Err(format!(
                        "feature {} repeats metadata value {value}",
                        self.id
                    ));
                }
            }
        }
        if self.dependencies.len() > 256 {
            return Err(format!("feature {} exceeds dependency limits", self.id));
        }
        let mut seen = std::collections::BTreeSet::new();
        for dependency in &self.dependencies {
            Self::identifier(&dependency.id)?;
            VersionReq::parse(&dependency.version)
                .map_err(|error| format!("invalid dependency {}: {error}", dependency.id))?;
            if dependency.id == self.id || !seen.insert(&dependency.id) {
                return Err(format!(
                    "feature {} has an invalid dependency {}",
                    self.id, dependency.id
                ));
            }
        }
        Ok(())
    }

    pub(crate) fn identifier(value: &str) -> Result<(), String> {
        if value.is_empty()
            || value.len() > 256
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"._-:".contains(&byte))
        {
            return Err(format!("invalid composition identifier {value:?}"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Dependency {
    pub id: String,
    pub version: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Lifecycle {
    #[serde(default)]
    pub startup: Vec<String>,
    #[serde(default)]
    pub shutdown: Vec<String>,
}
