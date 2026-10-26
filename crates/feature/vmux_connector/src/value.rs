use bevy::prelude::*;
use serde::{Deserialize, Deserializer, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Reflect, Serialize)]
struct Text(String);

impl Text {
    fn new(value: impl Into<String>) -> Result<Self, String> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err("value must not be empty".to_string());
        }
        Ok(Self(value))
    }

    fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for Text {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

#[derive(Component, Clone, Debug, PartialEq, Eq, Hash, Reflect, Serialize)]
#[component(immutable)]
#[reflect(Component)]
#[type_path = "vmux_connector"]
#[serde(transparent)]
pub struct ProviderKey(Text);

impl ProviderKey {
    pub fn new(value: impl Into<String>) -> Result<Self, String> {
        Text::new(value).map(Self)
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl std::fmt::Display for ProviderKey {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Component, Clone, Debug, PartialEq, Eq, Hash, Reflect, Serialize, Deserialize)]
#[component(immutable)]
#[reflect(Component)]
#[type_path = "vmux_connector"]
#[serde(transparent)]
pub struct ExternalHost(Text);

impl ExternalHost {
    pub fn new(value: impl Into<String>) -> Result<Self, String> {
        Text::new(value).map(Self)
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

#[derive(Component, Clone, Debug, PartialEq, Eq, Hash, Reflect, Serialize, Deserialize)]
#[component(immutable)]
#[reflect(Component)]
#[type_path = "vmux_connector"]
#[serde(transparent)]
pub struct ExternalNamespace(Text);

impl ExternalNamespace {
    pub fn new(value: impl Into<String>) -> Result<Self, String> {
        Text::new(value).map(Self)
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

#[derive(Component, Clone, Debug, PartialEq, Eq, Hash, Reflect, Serialize, Deserialize)]
#[component(immutable)]
#[reflect(Component)]
#[type_path = "vmux_connector"]
#[serde(transparent)]
pub struct AccountExternalId(Text);

impl AccountExternalId {
    pub fn new(value: impl Into<String>) -> Result<Self, String> {
        Text::new(value).map(Self)
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

#[derive(Component, Clone, Debug, PartialEq, Eq, Hash, Reflect, Serialize, Deserialize)]
#[component(immutable)]
#[reflect(Component)]
#[type_path = "vmux_connector"]
#[serde(transparent)]
pub struct ExternalId(Text);

impl ExternalId {
    pub fn new(value: impl Into<String>) -> Result<Self, String> {
        Text::new(value).map(Self)
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

#[derive(Component, Clone, Debug, PartialEq, Eq, Hash, Reflect, Serialize, Deserialize)]
#[component(immutable)]
#[reflect(Component)]
#[type_path = "vmux_connector"]
#[serde(transparent)]
pub struct ExternalKey(Text);

impl ExternalKey {
    pub fn new(value: impl Into<String>) -> Result<Self, String> {
        Text::new(value).map(Self)
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

#[derive(Component, Clone, Debug, PartialEq, Eq, Hash, Reflect, Serialize)]
#[component(immutable)]
#[reflect(Component)]
#[type_path = "vmux_connector"]
#[serde(transparent)]
pub struct ExternalUrl(String);

impl ExternalUrl {
    pub fn new(value: impl Into<String>) -> Result<Self, String> {
        let value = value.into();
        let url = url::Url::parse(&value).map_err(|error| error.to_string())?;
        if !matches!(url.scheme(), "http" | "https") {
            return Err("external URL must use http or https".to_string());
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for ExternalUrl {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

#[derive(Component, Clone, Debug, PartialEq, Eq, Hash, Reflect, Serialize, Deserialize)]
#[component(immutable)]
#[reflect(Component)]
#[type_path = "vmux_connector"]
#[serde(transparent)]
pub struct ResourceKind(Text);

impl ResourceKind {
    pub fn new(value: impl Into<String>) -> Result<Self, String> {
        Text::new(value).map(Self)
    }

    pub fn repository() -> Self {
        Self(Text("repository".to_string()))
    }

    pub fn issue() -> Self {
        Self(Text("issue".to_string()))
    }

    pub fn pull_request() -> Self {
        Self(Text("pull-request".to_string()))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Reflect, Serialize, Deserialize)]
#[type_path = "vmux_connector"]
#[serde(transparent)]
pub struct CanonicalField(Text);

impl CanonicalField {
    pub fn new(value: impl Into<String>) -> Result<Self, String> {
        Text::new(value).map(Self)
    }

    pub fn title() -> Self {
        Self(Text("title".to_string()))
    }

    pub fn description() -> Self {
        Self(Text("description".to_string()))
    }

    pub fn workflow_state() -> Self {
        Self(Text("workflow-state".to_string()))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_reject_empty_values() {
        assert!(ProviderKey::new(" ").is_err());
        assert!(ExternalId::new("").is_err());
        assert!(ResourceKind::new("\n").is_err());
    }

    #[test]
    fn external_urls_reject_unsafe_schemes() {
        assert!(ExternalUrl::new("javascript:alert(1)").is_err());
        assert!(ExternalUrl::new("https://github.com/vmux-ai/vmux").is_ok());
    }
}
