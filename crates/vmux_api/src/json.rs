#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[rkyv(serialize_bounds(__S: rkyv::ser::Writer + rkyv::ser::Allocator, __S::Error: rkyv::rancor::Source))]
#[rkyv(deserialize_bounds(__D::Error: rkyv::rancor::Source))]
#[rkyv(bytecheck(bounds(__C: rkyv::validation::ArchiveContext, __C::Error: rkyv::rancor::Source)))]
pub enum JsonValue {
    #[default]
    Null,
    Bool(bool),
    Number(String),
    String(String),
    Array(#[rkyv(omit_bounds)] Vec<Self>),
    Object(#[rkyv(omit_bounds)] Vec<(String, Self)>),
}

impl JsonValue {
    pub fn parse(value: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str::<serde_json::Value>(value).map(Self::from)
    }

    pub fn parse_or_string(value: &str) -> Self {
        Self::parse(value).unwrap_or_else(|_| Self::String(value.to_string()))
    }

    pub fn to_serde(&self) -> Result<serde_json::Value, serde_json::Error> {
        match self {
            Self::Null => Ok(serde_json::Value::Null),
            Self::Bool(value) => Ok(serde_json::Value::Bool(*value)),
            Self::Number(value) => serde_json::from_str(value),
            Self::String(value) => Ok(serde_json::Value::String(value.clone())),
            Self::Array(values) => {
                let mut converted = Vec::with_capacity(values.len());
                for value in values {
                    converted.push(value.to_serde()?);
                }
                Ok(serde_json::Value::Array(converted))
            }
            Self::Object(fields) => {
                let mut converted = serde_json::Map::new();
                for (name, value) in fields {
                    converted.insert(name.clone(), value.to_serde()?);
                }
                Ok(serde_json::Value::Object(converted))
            }
        }
    }
}

impl From<serde_json::Value> for JsonValue {
    fn from(value: serde_json::Value) -> Self {
        match value {
            serde_json::Value::Null => Self::Null,
            serde_json::Value::Bool(value) => Self::Bool(value),
            serde_json::Value::Number(value) => Self::Number(value.to_string()),
            serde_json::Value::String(value) => Self::String(value),
            serde_json::Value::Array(values) => {
                Self::Array(values.into_iter().map(Self::from).collect())
            }
            serde_json::Value::Object(fields) => Self::Object(
                fields
                    .into_iter()
                    .map(|(name, value)| (name, Self::from(value)))
                    .collect(),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde_value_round_trips_every_json_kind() {
        let value = serde_json::json!({
            "array": [null, true, -12.5, "text"],
            "object": {"nested": 7}
        });
        let wire = JsonValue::from(value.clone());
        assert_eq!(wire.to_serde().unwrap(), value);
    }

    #[test]
    fn malformed_json_becomes_plain_text_when_requested() {
        assert_eq!(
            JsonValue::parse_or_string("not json"),
            JsonValue::String("not json".to_string())
        );
    }
}
