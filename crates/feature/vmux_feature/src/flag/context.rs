use std::collections::BTreeMap;

#[cfg(feature = "ecs")]
use bevy_ecs::prelude::Component;
use serde::{Deserialize, Serialize};

use super::Value;

#[cfg_attr(feature = "ecs", derive(Component))]
pub struct ContextFailure(pub String);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ecs", derive(Component))]
#[serde(deny_unknown_fields)]
pub struct Context {
    pub generation: u64,
    pub subject: Subject,
    #[serde(with = "serde_with::rust::maps_duplicate_key_is_error")]
    pub attributes: BTreeMap<Attribute, Value>,
}

impl Context {
    pub fn installation(key: &str) -> Self {
        Self {
            generation: 0,
            subject: Subject {
                kind: SubjectKind::Installation,
                key: key.into(),
            },
            attributes: BTreeMap::new(),
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        self.subject.validate()?;
        for (attribute, value) in &self.attributes {
            attribute.validate(value)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Subject {
    pub kind: SubjectKind,
    pub key: String,
}

impl Subject {
    pub(super) fn validate(&self) -> Result<(), String> {
        if self.key.is_empty() || self.key.len() > 256 || self.key.chars().any(char::is_control) {
            return Err("invalid targeting subject".into());
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubjectKind {
    Installation,
    Device,
    Account,
    Workspace,
    Project,
    Session,
}

impl SubjectKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Installation => "installation",
            Self::Device => "device",
            Self::Account => "account",
            Self::Workspace => "workspace",
            Self::Project => "project",
            Self::Session => "session",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Attribute {
    Account,
    Workspace,
    Project,
    Session,
    Installation,
    Device,
    Platform,
    AppVersion,
    AppBuild,
    EntitlementTier,
    Entitled,
}

impl Attribute {
    pub(super) fn validate(self, value: &Value) -> Result<(), String> {
        let valid = match (self, value) {
            (Self::AppBuild, Value::Integer(build)) => *build >= 0,
            (Self::Entitled, Value::Boolean(_)) => true,
            (Self::AppBuild | Self::Entitled, _) => false,
            (_, Value::String(text)) => {
                !text.is_empty() && text.len() <= 256 && !text.chars().any(char::is_control)
            }
            _ => false,
        };
        if !valid {
            return Err(format!("invalid targeting attribute {self:?}"));
        }
        Ok(())
    }
}
