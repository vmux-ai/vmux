#[vmux_api::contract(Default, Eq)]
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
}

impl TryFrom<&JsonValue> for serde_json::Value {
    type Error = serde_json::Error;

    fn try_from(value: &JsonValue) -> Result<Self, Self::Error> {
        match value {
            JsonValue::Null => Ok(Self::Null),
            JsonValue::Bool(value) => Ok(Self::Bool(*value)),
            JsonValue::Number(value) => serde_json::from_str(value),
            JsonValue::String(value) => Ok(Self::String(value.clone())),
            JsonValue::Array(values) => {
                let mut converted = Vec::with_capacity(values.len());
                for value in values {
                    converted.push(Self::try_from(value)?);
                }
                Ok(Self::Array(converted))
            }
            JsonValue::Object(fields) => {
                let mut converted = serde_json::Map::new();
                for (name, value) in fields {
                    converted.insert(name.clone(), Self::try_from(value)?);
                }
                Ok(Self::Object(converted))
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
        assert_eq!(serde_json::Value::try_from(&wire).unwrap(), value);
    }

    #[test]
    fn malformed_json_becomes_plain_text_when_requested() {
        assert_eq!(
            JsonValue::parse_or_string("not json"),
            JsonValue::String("not json".to_string())
        );
    }
}
