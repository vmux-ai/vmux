use serde::{Deserialize, Serialize};

pub use assignment::Assignment;
pub use context::{Attribute, Context, ContextFailure, Subject, SubjectKind};
pub use decision::{CastError, Decision, Details, Reason, Retention, Snapshot, Source};
pub use targeting::{Condition, Configuration, Operator, Rollout, Rule};

mod assignment;
mod context;
mod decision;
mod targeting;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Value {
    Boolean(bool),
    String(String),
    Integer(i64),
    Float(f64),
    Json(serde_json::Value),
}

impl Value {
    pub fn as_boolean(&self) -> Option<bool> {
        match self {
            Self::Boolean(value) => Some(*value),
            _ => None,
        }
    }

    pub fn encode(&self) -> String {
        let value = match self {
            Self::Boolean(value) => serde_json::Value::Bool(*value),
            Self::String(value) => serde_json::Value::String(value.clone()),
            Self::Integer(value) => serde_json::Value::Number((*value).into()),
            Self::Float(value) => serde_json::json!(value),
            Self::Json(value) => value.clone(),
        };
        value.to_string()
    }

    pub fn decode(value_type: ValueType, text: &str) -> Result<Self, String> {
        if text.len() > 16384 {
            return Err("flag value exceeds 16 KiB".into());
        }
        let value: serde_json::Value =
            serde_json::from_str(text).map_err(|error| error.to_string())?;
        let result = match (value_type, value) {
            (ValueType::Boolean, serde_json::Value::Bool(value)) => Self::Boolean(value),
            (ValueType::String, serde_json::Value::String(value)) => Self::String(value),
            (ValueType::Integer, serde_json::Value::Number(value)) if value.is_i64() => {
                Self::Integer(value.as_i64().unwrap())
            }
            (ValueType::Float, serde_json::Value::Number(value)) if value.is_f64() => {
                Self::Float(value.as_f64().unwrap())
            }
            (ValueType::Json, value) => Self::Json(value),
            _ => return Err("flag value does not match its declared type".into()),
        };
        result.validate()?;
        Ok(result)
    }

    pub fn value_type(&self) -> ValueType {
        match self {
            Self::Boolean(_) => ValueType::Boolean,
            Self::String(_) => ValueType::String,
            Self::Integer(_) => ValueType::Integer,
            Self::Float(_) => ValueType::Float,
            Self::Json(_) => ValueType::Json,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if let Self::Float(value) = self
            && !value.is_finite()
        {
            return Err("flag float must be finite".into());
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ValueType {
    Boolean,
    String,
    Integer,
    Float,
    Json,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Class {
    Release,
    Experiment,
    Operational,
    Config,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum OfflinePolicy {
    #[default]
    Default,
    RetainOff,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Lifecycle {
    #[default]
    Persistent,
    Temporary,
    Retired,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Declaration {
    pub key: String,
    #[serde(rename = "type")]
    pub value_type: ValueType,
    pub class: Class,
    pub default: Value,
    #[serde(default)]
    pub owner: String,
    #[serde(default)]
    pub expiry: Option<String>,
    #[serde(default)]
    pub lifecycle: Lifecycle,
    #[serde(default)]
    pub offline: OfflinePolicy,
}

impl Declaration {
    pub fn validate(&self) -> Result<(), String> {
        if self.key.is_empty()
            || self.key.len() > 128
            || !self.key.bytes().all(|byte| {
                byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || matches!(byte, b'.' | b'_' | b'-')
            })
        {
            return Err("invalid flag key".into());
        }
        self.default.validate()?;
        if self.default.value_type() != self.value_type {
            return Err(format!(
                "flag {} default does not match its declared type",
                self.key
            ));
        }
        if matches!(self.lifecycle, Lifecycle::Temporary | Lifecycle::Retired)
            && (self.owner.trim().is_empty() || self.expiry.is_none())
        {
            return Err(format!(
                "temporary flag {} requires owner and UTC expiry",
                self.key
            ));
        }
        if let Some(expiry) = &self.expiry {
            let date = chrono::DateTime::parse_from_rfc3339(expiry)
                .map_err(|_| format!("invalid UTC expiry for flag {}", self.key))?;
            if date.offset().local_minus_utc() != 0 {
                return Err(format!("flag {} expiry must use UTC", self.key));
            }
        }
        if self.offline == OfflinePolicy::RetainOff && self.value_type != ValueType::Boolean {
            return Err(format!("flag {} RetainOff requires a boolean", self.key));
        }
        Ok(())
    }
}
