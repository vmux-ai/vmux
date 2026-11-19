#[cfg(feature = "ecs")]
use bevy_ecs::prelude::Component;
use serde::{Deserialize, Serialize};

use super::targeting::Match;
use super::{Configuration, Context, Declaration, OfflinePolicy, Value, ValueType};

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "ecs", derive(Component))]
pub struct Decision<T> {
    pub value: T,
    pub details: Details,
}

impl Decision<Value> {
    fn apply_rule(&mut self, rule: &super::Rule, context: &Context, key: &str) -> Match {
        let matched = rule.matches(context, key);
        if matched == Match::Yes {
            self.value = rule.value.clone();
            self.details.variant = rule.variant.clone();
            self.details.reason = if rule.rollout.is_some() {
                Reason::Percentage
            } else {
                Reason::RuleMatch
            };
        }
        matched
    }

    fn new(declaration: &Declaration, context: &Context, snapshot: Option<&Snapshot>) -> Self {
        Self {
            value: declaration.default.clone(),
            details: Details {
                variant: None,
                reason: Reason::Default,
                source: Source::Default,
                snapshot: snapshot.cloned(),
                context_generation: context.generation,
                declaration_expiry: declaration.expiry.clone(),
                retained_revision: None,
            },
        }
    }

    pub fn cast<T>(self) -> Result<Decision<T>, CastError>
    where
        Decision<T>: TryFrom<Decision<Value>, Error = CastError>,
    {
        Decision::<T>::try_from(self)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Details {
    pub variant: Option<String>,
    pub reason: Reason,
    pub source: Source,
    pub snapshot: Option<Snapshot>,
    pub context_generation: u64,
    pub declaration_expiry: Option<String>,
    pub retained_revision: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub environment: String,
    pub revision: u64,
    pub issued_at: u64,
    pub valid_until: u64,
    pub source: Source,
}

impl Snapshot {
    pub fn is_valid(&self, now: u64) -> bool {
        now > 0
            && !self.environment.is_empty()
            && self.environment.len() <= 64
            && self.revision > 0
            && self.issued_at <= now
            && self.valid_until > now
            && self.valid_until > self.issued_at
            && self.valid_until - self.issued_at <= 3600
            && matches!(self.source, Source::Cloud | Source::Cache)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    Default,
    Static,
    RuleMatch,
    Percentage,
    StickyAssignment,
    Retired,
    DevelopmentOverride,
    RetainedOff,
    RestoringRetention,
    RetentionError,
    MissingContext,
    ProviderUnavailable,
    TypeError,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    #[default]
    Default,
    Cloud,
    Cache,
    Development,
    Retained,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ecs", derive(Component))]
pub struct Retention {
    status: RetentionStatus,
    revision: Option<u64>,
}

impl Retention {
    pub fn absent() -> Self {
        Self {
            status: RetentionStatus::Absent,
            revision: None,
        }
    }
    pub fn restoring() -> Self {
        Self {
            status: RetentionStatus::Restoring,
            revision: None,
        }
    }
    pub fn failed() -> Self {
        Self {
            status: RetentionStatus::Failed,
            revision: None,
        }
    }
    pub fn off(revision: u64) -> Self {
        Self {
            status: RetentionStatus::Off,
            revision: Some(revision),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RetentionStatus {
    Absent,
    Restoring,
    Off,
    Failed,
}

impl Declaration {
    pub fn decide(
        &self,
        context: &Context,
        configuration: Option<&Configuration>,
        snapshot: Option<&Snapshot>,
        development_override: Option<&Value>,
        retention: Retention,
        now: u64,
    ) -> Result<Decision<Value>, String> {
        self.validate()?;
        let mut decision = Decision::new(self, context, snapshot);
        if self.offline == OfflinePolicy::RetainOff && retention != Retention::absent() {
            decision.value = Value::Boolean(false);
            decision.details.source = Source::Retained;
            decision.details.reason = match retention.status {
                RetentionStatus::Restoring => Reason::RestoringRetention,
                RetentionStatus::Off => {
                    decision.details.retained_revision = retention.revision;
                    Reason::RetainedOff
                }
                RetentionStatus::Failed => Reason::RetentionError,
                RetentionStatus::Absent => unreachable!(),
            };
            return Ok(decision);
        }
        if self.lifecycle == super::Lifecycle::Retired {
            decision.details.reason = Reason::Retired;
            return Ok(decision);
        }
        if let Some(value) = development_override {
            if value.value_type() != self.value_type || value.validate().is_err() {
                decision.details.reason = Reason::TypeError;
                return Ok(decision);
            }
            decision.value = value.clone();
            decision.details.source = Source::Development;
            decision.details.reason = Reason::DevelopmentOverride;
            return Ok(decision);
        }
        if context.validate().is_err() {
            decision.details.reason = Reason::MissingContext;
            return Ok(decision);
        }
        let (Some(configuration), Some(snapshot)) = (configuration, snapshot) else {
            match (configuration, snapshot) {
                (None, None) => {}
                (None, Some(snapshot)) if snapshot.is_valid(now) => {}
                _ => decision.details.reason = Reason::ProviderUnavailable,
            }
            return Ok(decision);
        };
        if !snapshot.is_valid(now) {
            decision.details.reason = Reason::ProviderUnavailable;
            return Ok(decision);
        }
        if configuration.value.value_type() != self.value_type || configuration.validate().is_err()
        {
            decision.details.reason = Reason::TypeError;
            return Ok(decision);
        }
        decision.details.source = snapshot.source;
        let mut missing_context = false;
        for rule in &configuration.rules {
            match decision.apply_rule(rule, context, &self.key) {
                Match::Yes => {
                    return Ok(decision);
                }
                Match::No => {}
                Match::MissingContext => missing_context = true,
            }
        }
        decision.value = configuration.value.clone();
        decision.details.variant = configuration.variant.clone();
        decision.details.reason = if missing_context {
            Reason::MissingContext
        } else {
            Reason::Static
        };
        Ok(decision)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CastError {
    non_finite: bool,
    expected: Option<ValueType>,
    actual: Option<ValueType>,
}

impl CastError {
    fn mismatch(expected: ValueType, actual: ValueType) -> Self {
        Self {
            non_finite: false,
            expected: Some(expected),
            actual: Some(actual),
        }
    }

    fn non_finite() -> Self {
        Self {
            non_finite: true,
            expected: Some(ValueType::Float),
            actual: Some(ValueType::Float),
        }
    }
}

impl TryFrom<Decision<Value>> for Decision<bool> {
    type Error = CastError;

    fn try_from(decision: Decision<Value>) -> Result<Self, Self::Error> {
        match decision.value {
            Value::Boolean(value) => Ok(Self {
                value,
                details: decision.details,
            }),
            value => Err(CastError::mismatch(ValueType::Boolean, value.value_type())),
        }
    }
}

impl TryFrom<Decision<Value>> for Decision<String> {
    type Error = CastError;

    fn try_from(decision: Decision<Value>) -> Result<Self, Self::Error> {
        match decision.value {
            Value::String(value) => Ok(Self {
                value,
                details: decision.details,
            }),
            value => Err(CastError::mismatch(ValueType::String, value.value_type())),
        }
    }
}

impl TryFrom<Decision<Value>> for Decision<i64> {
    type Error = CastError;

    fn try_from(decision: Decision<Value>) -> Result<Self, Self::Error> {
        match decision.value {
            Value::Integer(value) => Ok(Self {
                value,
                details: decision.details,
            }),
            value => Err(CastError::mismatch(ValueType::Integer, value.value_type())),
        }
    }
}

impl TryFrom<Decision<Value>> for Decision<f64> {
    type Error = CastError;

    fn try_from(decision: Decision<Value>) -> Result<Self, Self::Error> {
        match decision.value {
            Value::Float(value) if value.is_finite() => Ok(Self {
                value,
                details: decision.details,
            }),
            Value::Float(_) => Err(CastError::non_finite()),
            value => Err(CastError::mismatch(ValueType::Float, value.value_type())),
        }
    }
}

impl TryFrom<Decision<Value>> for Decision<serde_json::Value> {
    type Error = CastError;

    fn try_from(decision: Decision<Value>) -> Result<Self, Self::Error> {
        match decision.value {
            Value::Json(value) => Ok(Self {
                value,
                details: decision.details,
            }),
            value => Err(CastError::mismatch(ValueType::Json, value.value_type())),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::flag::{
        Attribute, Class, Condition, Lifecycle, Operator, Rollout, Rule, Subject, SubjectKind,
    };

    struct Fixture {
        declaration: Declaration,
        context: Context,
        snapshot: Snapshot,
    }

    impl Fixture {
        fn new(default: Value) -> Self {
            Self {
                declaration: Declaration {
                    key: "example.flag".into(),
                    value_type: default.value_type(),
                    class: Class::Release,
                    default,
                    owner: "vmux".into(),
                    expiry: None,
                    lifecycle: Lifecycle::Persistent,
                    offline: OfflinePolicy::Default,
                },
                context: Context {
                    generation: 7,
                    subject: Subject {
                        kind: SubjectKind::Installation,
                        key: "device-a".into(),
                    },
                    attributes: BTreeMap::new(),
                },
                snapshot: Snapshot {
                    environment: "development".into(),
                    revision: 13,
                    issued_at: 900,
                    valid_until: 1200,
                    source: Source::Cloud,
                },
            }
        }
    }

    #[test]
    fn ordered_rules_skip_missing_context_and_select_the_first_matching_variant() {
        let mut fixture = Fixture::new(Value::String("bundled".into()));
        fixture
            .context
            .attributes
            .insert(Attribute::Account, Value::String("account-a".into()));
        fixture
            .context
            .attributes
            .insert(Attribute::Platform, Value::String("macos".into()));
        let configuration = Configuration {
            value: Value::String("fallback".into()),
            variant: Some("base".into()),
            rules: vec![
                Rule {
                    conditions: vec![Condition {
                        attribute: Attribute::Account,
                        operator: Operator::Equal,
                        values: vec![Value::String("account-a".into())],
                    }],
                    rollout: None,
                    value: Value::String("first".into()),
                    variant: Some("first-variant".into()),
                },
                Rule {
                    conditions: vec![Condition {
                        attribute: Attribute::Platform,
                        operator: Operator::OneOf,
                        values: vec![Value::String("linux".into()), Value::String("macos".into())],
                    }],
                    rollout: None,
                    value: Value::String("second".into()),
                    variant: Some("second-variant".into()),
                },
            ],
            experiment_generation: None,
        };
        for (remove, expected, reason) in [
            (None, "first", Reason::RuleMatch),
            (Some(Attribute::Account), "second", Reason::RuleMatch),
            (
                Some(Attribute::Platform),
                "fallback",
                Reason::MissingContext,
            ),
        ] {
            if let Some(remove) = remove {
                fixture.context.attributes.remove(&remove);
            }
            let decision = fixture
                .declaration
                .decide(
                    &fixture.context,
                    Some(&configuration),
                    Some(&fixture.snapshot),
                    None,
                    Retention::absent(),
                    1000,
                )
                .unwrap()
                .cast::<String>()
                .unwrap();
            assert_eq!(decision.value, expected);
            assert_eq!(decision.details.reason, reason);
        }
    }

    #[test]
    fn explicit_retirement_uses_the_final_default_without_clearing_a_retained_disable() {
        let mut fixture = Fixture::new(Value::Boolean(true));
        fixture.declaration.lifecycle = Lifecycle::Retired;
        fixture.declaration.expiry = Some("2027-06-01T00:00:00Z".into());
        let configuration = Configuration::boolean(false);
        let decision = fixture
            .declaration
            .decide(
                &fixture.context,
                Some(&configuration),
                Some(&fixture.snapshot),
                Some(&Value::Boolean(false)),
                Retention::absent(),
                1000,
            )
            .unwrap();
        assert_eq!(decision.value, Value::Boolean(true));
        assert_eq!(decision.details.reason, Reason::Retired);
        fixture.declaration.offline = OfflinePolicy::RetainOff;
        let retained = fixture
            .declaration
            .decide(
                &fixture.context,
                None,
                None,
                Some(&Value::Boolean(true)),
                Retention::off(9),
                1000,
            )
            .unwrap();
        assert_eq!(retained.value, Value::Boolean(false));
        assert_eq!(retained.details.reason, Reason::RetainedOff);
    }

    #[test]
    fn version_and_numeric_rules_use_typed_context_without_coercion() {
        let mut fixture = Fixture::new(Value::Boolean(false));
        fixture
            .context
            .attributes
            .insert(Attribute::AppVersion, Value::String("1.12.0".into()));
        fixture
            .context
            .attributes
            .insert(Attribute::AppBuild, Value::Integer(10));
        let mut configuration = Configuration {
            value: Value::Boolean(false),
            variant: None,
            rules: vec![Rule {
                conditions: vec![
                    Condition {
                        attribute: Attribute::AppVersion,
                        operator: Operator::SemverMatches,
                        values: vec![Value::String(">=1.10, <2".into())],
                    },
                    Condition {
                        attribute: Attribute::AppBuild,
                        operator: Operator::Greater,
                        values: vec![Value::Integer(9)],
                    },
                ],
                rollout: Some(Rollout {
                    salt: "stable".into(),
                    percent: 10000,
                }),
                value: Value::Boolean(true),
                variant: Some("treatment".into()),
            }],
            experiment_generation: None,
        };
        let decision = fixture
            .declaration
            .decide(
                &fixture.context,
                Some(&configuration),
                Some(&fixture.snapshot),
                None,
                Retention::absent(),
                1000,
            )
            .unwrap()
            .cast::<bool>()
            .unwrap();
        assert!(decision.value);
        fixture
            .context
            .attributes
            .insert(Attribute::AppVersion, Value::String("1.9.0".into()));
        let decision = fixture
            .declaration
            .decide(
                &fixture.context,
                Some(&configuration),
                Some(&fixture.snapshot),
                None,
                Retention::absent(),
                1000,
            )
            .unwrap()
            .cast::<bool>()
            .unwrap();
        assert!(!decision.value);
        configuration.rules[0].conditions[1].values = vec![Value::String("9".into())];
        let decision = fixture
            .declaration
            .decide(
                &fixture.context,
                Some(&configuration),
                Some(&fixture.snapshot),
                None,
                Retention::absent(),
                1000,
            )
            .unwrap()
            .cast::<bool>()
            .unwrap();
        assert!(!decision.value);
        assert_eq!(decision.details.reason, Reason::TypeError);
    }

    #[test]
    fn all_value_types_evaluate_and_cast_without_number_or_boolean_coercion() {
        for (default, published) in [
            (Value::Boolean(false), Value::Boolean(true)),
            (
                Value::String("default".into()),
                Value::String("selected".into()),
            ),
            (Value::Integer(0), Value::Integer(i64::MAX)),
            (Value::Float(0.0), Value::Float(1.25)),
            (
                Value::Json(serde_json::Value::Null),
                Value::Json(serde_json::json!({"nested":[1,true,"value"]})),
            ),
        ] {
            let fixture = Fixture::new(default);
            let configuration = Configuration {
                value: published.clone(),
                variant: Some("selected".into()),
                rules: Vec::new(),
                experiment_generation: None,
            };
            let decision = fixture
                .declaration
                .decide(
                    &fixture.context,
                    Some(&configuration),
                    Some(&fixture.snapshot),
                    None,
                    Retention::absent(),
                    1000,
                )
                .unwrap();
            assert_eq!(decision.value, published);
            assert_eq!(decision.details.source, Source::Cloud);
            match published {
                Value::Boolean(value) => assert_eq!(decision.cast::<bool>().unwrap().value, value),
                Value::String(value) => assert_eq!(decision.cast::<String>().unwrap().value, value),
                Value::Integer(value) => assert_eq!(decision.cast::<i64>().unwrap().value, value),
                Value::Float(value) => assert_eq!(decision.cast::<f64>().unwrap().value, value),
                Value::Json(value) => {
                    assert_eq!(decision.cast::<serde_json::Value>().unwrap().value, value)
                }
            }
        }
        let fixture = Fixture::new(Value::String("true".into()));
        let decision = fixture
            .declaration
            .decide(
                &fixture.context,
                None,
                None,
                None,
                Retention::absent(),
                1000,
            )
            .unwrap();
        assert!(decision.cast::<bool>().is_err());
        let fixture = Fixture::new(Value::Float(1.0));
        let mut decision = fixture
            .declaration
            .decide(
                &fixture.context,
                None,
                None,
                None,
                Retention::absent(),
                1000,
            )
            .unwrap();
        assert!(decision.clone().cast::<i64>().is_err());
        decision.value = Value::Float(f64::INFINITY);
        assert_eq!(decision.cast::<f64>(), Err(CastError::non_finite()));
        let fixture = Fixture::new(Value::Integer(1));
        let decision = fixture
            .declaration
            .decide(
                &fixture.context,
                None,
                None,
                None,
                Retention::absent(),
                1000,
            )
            .unwrap();
        assert!(decision.cast::<f64>().is_err());
    }

    #[test]
    fn cache_expiry_and_override_keep_decision_source_and_captured_context() {
        let mut fixture = Fixture::new(Value::Boolean(true));
        fixture.snapshot.source = Source::Cache;
        let configuration = Configuration {
            value: Value::Boolean(false),
            variant: None,
            rules: Vec::new(),
            experiment_generation: None,
        };
        let cached = fixture
            .declaration
            .decide(
                &fixture.context,
                Some(&configuration),
                Some(&fixture.snapshot),
                None,
                Retention::absent(),
                1000,
            )
            .unwrap()
            .cast::<bool>()
            .unwrap();
        assert!(!cached.value);
        assert_eq!(cached.details.source, Source::Cache);
        let expired = fixture
            .declaration
            .decide(
                &fixture.context,
                Some(&configuration),
                Some(&fixture.snapshot),
                None,
                Retention::absent(),
                1200,
            )
            .unwrap()
            .cast::<bool>()
            .unwrap();
        assert!(expired.value);
        assert_eq!(expired.details.reason, Reason::ProviderUnavailable);
        let overridden = fixture
            .declaration
            .decide(
                &fixture.context,
                Some(&configuration),
                Some(&fixture.snapshot),
                Some(&Value::Boolean(true)),
                Retention::absent(),
                1000,
            )
            .unwrap()
            .cast::<bool>()
            .unwrap();
        assert!(overridden.value);
        assert_eq!(overridden.details.source, Source::Development);
        fixture.context.generation += 1;
        assert_eq!(cached.details.context_generation, 7);
        let invalid = fixture
            .declaration
            .decide(
                &fixture.context,
                Some(&configuration),
                Some(&fixture.snapshot),
                Some(&Value::String("false".into())),
                Retention::absent(),
                1000,
            )
            .unwrap()
            .cast::<bool>()
            .unwrap();
        assert!(invalid.value);
        assert_eq!(invalid.details.reason, Reason::TypeError);
        fixture.context.subject.key.clear();
        let disabled = fixture
            .declaration
            .decide(
                &fixture.context,
                None,
                None,
                Some(&Value::Boolean(false)),
                Retention::absent(),
                1000,
            )
            .unwrap()
            .cast::<bool>()
            .unwrap();
        assert!(!disabled.value);
        assert_eq!(disabled.details.source, Source::Development);
    }

    #[test]
    fn unreadable_or_retained_off_blocks_defaults_overrides_and_expired_declarations() {
        let mut fixture = Fixture::new(Value::Boolean(true));
        fixture.declaration.offline = OfflinePolicy::RetainOff;
        fixture.declaration.lifecycle = Lifecycle::Temporary;
        fixture.declaration.expiry = Some("2020-01-01T00:00:00Z".into());
        let configuration = Configuration {
            value: Value::Boolean(true),
            variant: None,
            rules: Vec::new(),
            experiment_generation: None,
        };
        for retention in [
            Retention::restoring(),
            Retention::off(12),
            Retention::failed(),
        ] {
            for now in [1000, 1200] {
                for publication in [None, Some(&configuration)] {
                    let decision = fixture
                        .declaration
                        .decide(
                            &fixture.context,
                            publication,
                            Some(&fixture.snapshot),
                            Some(&Value::Boolean(true)),
                            retention,
                            now,
                        )
                        .unwrap()
                        .cast::<bool>()
                        .unwrap();
                    assert!(!decision.value);
                    assert_eq!(decision.details.source, Source::Retained);
                }
            }
        }
        let first_run = fixture
            .declaration
            .decide(
                &fixture.context,
                None,
                None,
                None,
                Retention::absent(),
                1000,
            )
            .unwrap()
            .cast::<bool>()
            .unwrap();
        assert!(first_run.value);
    }
}
