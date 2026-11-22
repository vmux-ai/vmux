use std::cmp::Ordering;

#[cfg(feature = "ecs")]
use bevy_ecs::prelude::Component;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{Attribute, Context, Subject, Value, ValueType};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ecs", derive(Component))]
#[cfg_attr(feature = "ecs", component(immutable))]
#[serde(deny_unknown_fields)]
pub struct Configuration {
    pub value: Value,
    #[serde(default)]
    pub variant: Option<String>,
    #[serde(default)]
    pub rules: Vec<Rule>,
    #[serde(default)]
    pub experiment_generation: Option<String>,
}

impl Configuration {
    pub fn encode(&self) -> String {
        serde_json::to_string(self).expect("validated flag configuration")
    }

    pub fn constant(value: Value) -> Self {
        Self {
            value,
            variant: None,
            rules: Vec::new(),
            experiment_generation: None,
        }
    }

    pub fn boolean(value: bool) -> Self {
        Self {
            value: Value::Boolean(value),
            variant: None,
            rules: Vec::new(),
            experiment_generation: None,
        }
    }

    pub(crate) fn legacy_boolean(&self) -> Option<bool> {
        if self.variant.is_some() || !self.rules.is_empty() || self.experiment_generation.is_some()
        {
            return None;
        }
        match self.value {
            Value::Boolean(value) => Some(value),
            _ => None,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        self.value.validate()?;
        Self::identifier(&self.variant)?;
        Self::identifier(&self.experiment_generation)?;
        if self.rules.len() > 32 {
            return Err("too many targeting rules".into());
        }
        for rule in &self.rules {
            rule.validate(self.value.value_type())?;
        }
        Ok(())
    }

    fn identifier(value: &Option<String>) -> Result<(), String> {
        if let Some(value) = value
            && (value.is_empty() || value.len() > 128 || value.chars().any(char::is_control))
        {
            return Err("invalid flag variant or experiment generation".into());
        }
        Ok(())
    }

    #[cfg(any(feature = "ecs", test))]
    pub(crate) fn hash(&self, digest: &mut Sha256) {
        let value = serde_json::to_value(self).expect("validated flag configuration");
        Self::hash_json(&value, digest);
    }

    #[cfg(any(feature = "ecs", test))]
    fn hash_json(value: &serde_json::Value, digest: &mut Sha256) {
        match value {
            serde_json::Value::Null => digest.update([0]),
            serde_json::Value::Bool(value) => digest.update([1, u8::from(*value)]),
            serde_json::Value::Number(value) => {
                let value = value.to_string();
                digest.update([2]);
                digest.update((value.len() as u64).to_be_bytes());
                digest.update(value.as_bytes());
            }
            serde_json::Value::String(value) => {
                digest.update([3]);
                digest.update((value.len() as u64).to_be_bytes());
                digest.update(value.as_bytes());
            }
            serde_json::Value::Array(values) => {
                digest.update([4]);
                digest.update((values.len() as u64).to_be_bytes());
                for value in values {
                    Self::hash_json(value, digest);
                }
            }
            serde_json::Value::Object(values) => {
                let mut entries = values.iter().collect::<Vec<_>>();
                entries.sort_by_key(|(left, _)| *left);
                digest.update([5]);
                digest.update((entries.len() as u64).to_be_bytes());
                for (key, value) in entries {
                    digest.update((key.len() as u64).to_be_bytes());
                    digest.update(key.as_bytes());
                    Self::hash_json(value, digest);
                }
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    #[serde(default)]
    pub conditions: Vec<Condition>,
    #[serde(default)]
    pub rollout: Option<Rollout>,
    pub value: Value,
    #[serde(default)]
    pub variant: Option<String>,
}

impl Rule {
    fn validate(&self, expected: ValueType) -> Result<(), String> {
        self.value.validate()?;
        if self.value.value_type() != expected || self.conditions.len() > 32 {
            return Err("incompatible targeting rule".into());
        }
        Configuration::identifier(&self.variant)?;
        for condition in &self.conditions {
            condition.validate()?;
        }
        if let Some(rollout) = &self.rollout {
            rollout.validate()?;
        }
        Ok(())
    }

    pub(super) fn matches(&self, context: &Context, key: &str) -> Match {
        let mut missing = false;
        for condition in &self.conditions {
            match condition.matches(context) {
                Match::Yes => {}
                Match::No => return Match::No,
                Match::MissingContext => missing = true,
            }
        }
        if missing {
            return Match::MissingContext;
        }
        if let Some(rollout) = &self.rollout
            && !rollout.contains(key, &context.subject)
        {
            return Match::No;
        }
        Match::Yes
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Condition {
    pub attribute: Attribute,
    pub operator: Operator,
    pub values: Vec<Value>,
}

impl Condition {
    fn validate(&self) -> Result<(), String> {
        if self.values.is_empty() || self.values.len() > 64 {
            return Err("invalid targeting operand count".into());
        }
        if !matches!(self.operator, Operator::OneOf | Operator::NotOneOf) && self.values.len() != 1
        {
            return Err("targeting operator requires one operand".into());
        }
        for value in &self.values {
            self.attribute.validate(value)?;
        }
        if self.operator == Operator::SemverMatches {
            if self.attribute != Attribute::AppVersion {
                return Err("semantic-version targeting requires app_version".into());
            }
            let Value::String(requirement) = &self.values[0] else {
                return Err("semantic-version targeting requires a string".into());
            };
            semver::VersionReq::parse(requirement).map_err(|error| error.to_string())?;
        }
        if matches!(
            self.operator,
            Operator::Less | Operator::LessOrEqual | Operator::Greater | Operator::GreaterOrEqual
        ) && matches!(self.values[0], Value::Boolean(_) | Value::Json(_))
        {
            return Err("targeting operand is not ordered".into());
        }
        Ok(())
    }

    fn matches(&self, context: &Context) -> Match {
        let Some(actual) = context.attributes.get(&self.attribute) else {
            return Match::MissingContext;
        };
        let Some(expected) = self.values.first() else {
            return Match::No;
        };
        if actual.value_type() != expected.value_type() {
            return Match::MissingContext;
        }
        let matches = match self.operator {
            Operator::Equal => actual == expected,
            Operator::NotEqual => actual != expected,
            Operator::OneOf => self.values.contains(actual),
            Operator::NotOneOf => !self.values.contains(actual),
            Operator::Less => Self::compare(actual, expected) == Some(Ordering::Less),
            Operator::LessOrEqual => {
                matches!(
                    Self::compare(actual, expected),
                    Some(Ordering::Less | Ordering::Equal)
                )
            }
            Operator::Greater => Self::compare(actual, expected) == Some(Ordering::Greater),
            Operator::GreaterOrEqual => {
                matches!(
                    Self::compare(actual, expected),
                    Some(Ordering::Greater | Ordering::Equal)
                )
            }
            Operator::SemverMatches => {
                let (Value::String(actual), Value::String(expected)) = (actual, expected) else {
                    return Match::MissingContext;
                };
                let Ok(actual) = semver::Version::parse(actual) else {
                    return Match::MissingContext;
                };
                let Ok(expected) = semver::VersionReq::parse(expected) else {
                    return Match::No;
                };
                expected.matches(&actual)
            }
        };
        if matches { Match::Yes } else { Match::No }
    }

    fn compare(actual: &Value, expected: &Value) -> Option<Ordering> {
        match (actual, expected) {
            (Value::String(actual), Value::String(expected)) => Some(actual.cmp(expected)),
            (Value::Integer(actual), Value::Integer(expected)) => Some(actual.cmp(expected)),
            (Value::Float(actual), Value::Float(expected)) => actual.partial_cmp(expected),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Operator {
    Equal,
    NotEqual,
    Less,
    LessOrEqual,
    Greater,
    GreaterOrEqual,
    OneOf,
    NotOneOf,
    SemverMatches,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rollout {
    pub salt: String,
    pub percent: u16,
}

impl Rollout {
    pub fn validate(&self) -> Result<(), String> {
        if self.salt.is_empty()
            || self.salt.len() > 128
            || self.salt.chars().any(char::is_control)
            || self.percent > 10000
        {
            return Err("invalid percentage rollout".into());
        }
        Ok(())
    }

    pub fn bucket(&self, key: &str, subject: &Subject) -> u16 {
        let mut hash = Sha256::new();
        for field in [key, &self.salt, subject.kind.as_str(), &subject.key] {
            hash.update((field.len() as u64).to_be_bytes());
            hash.update(field.as_bytes());
        }
        let bytes: [u8; 8] = hash.finalize()[..8].try_into().expect("SHA-256 prefix");
        (u64::from_be_bytes(bytes) % 10000) as u16
    }

    pub fn contains(&self, key: &str, subject: &Subject) -> bool {
        if self.validate().is_err() || subject.validate().is_err() {
            return false;
        }
        self.bucket(key, subject) < self.percent
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Match {
    Yes,
    No,
    MissingContext,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flag::SubjectKind;

    #[test]
    fn rollout_matches_fixed_utf8_length_framed_cross_platform_vectors() {
        for (key, salt, kind, subject, expected) in [
            (
                "remote.live",
                "r1",
                SubjectKind::Installation,
                "device-a",
                9036,
            ),
            (
                "remote.live",
                "r1",
                SubjectKind::Installation,
                "ユーザー🖥️",
                6127,
            ),
            ("a", "bc", SubjectKind::Account, "x", 1170),
            ("ab", "c", SubjectKind::Account, "x", 8157),
            (
                "exp.mode",
                "2026-generation",
                SubjectKind::Workspace,
                "w-1",
                8661,
            ),
            (
                "remote.live",
                "r2",
                SubjectKind::Installation,
                "device-a",
                5198,
            ),
        ] {
            let subject = Subject {
                kind,
                key: subject.into(),
            };
            let mut rollout = Rollout {
                salt: salt.into(),
                percent: 0,
            };
            assert_eq!(rollout.bucket(key, &subject), expected);
            assert!(!rollout.contains(key, &subject));
            rollout.percent = expected;
            assert!(!rollout.contains(key, &subject));
            rollout.percent = expected + 1;
            assert!(rollout.contains(key, &subject));
            rollout.percent = 10000;
            assert!(rollout.contains(key, &subject));
        }
    }

    #[test]
    fn typed_rule_decoding_refuses_unknown_operators_attributes_and_coercion() {
        let source = r#"{
            "value":{"Boolean":false},
            "rules":[{"conditions":[{"attribute":"platform","operator":"equal","values":[{"String":"macos"}]}],"value":{"Boolean":true}}]
        }"#;
        let valid: Configuration = serde_json::from_str(source).unwrap();
        valid.validate().unwrap();
        for invalid in [
            source.replace("\"equal\"", "\"evaluate_script\""),
            source.replace("\"platform\"", "\"access_token\""),
            source.replace("{\"String\":\"macos\"}", "true"),
        ] {
            assert!(serde_json::from_str::<Configuration>(&invalid).is_err());
        }
        for invalid in [
            source.replace("{\"String\":\"macos\"}", "{\"Boolean\":true}"),
            source.replace(
                "\"value\":{\"Boolean\":true}",
                "\"value\":{\"String\":\"true\"}",
            ),
            source.replace("\"equal\"", "\"semver_matches\""),
        ] {
            let configuration: Configuration = serde_json::from_str(&invalid).unwrap();
            assert!(configuration.validate().is_err());
        }
    }
}
