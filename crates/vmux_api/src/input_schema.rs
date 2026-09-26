use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::Value;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct InputSchema {
    #[serde(
        rename = "type",
        default,
        deserialize_with = "InputSchema::deserialize_type"
    )]
    schema_type: Option<InputSchemaType>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    required: Vec<String>,
    #[serde(default)]
    properties: BTreeMap<String, InputSchema>,
    #[serde(default)]
    additional_properties: bool,
    #[serde(default)]
    definitions: BTreeMap<String, InputSchema>,
    #[serde(default)]
    values: Vec<String>,
    #[serde(default)]
    min_length: Option<u64>,
    #[serde(default)]
    max_length: Option<u64>,
    #[serde(default)]
    minimum: Option<i64>,
    #[serde(default)]
    maximum: Option<i64>,
    #[serde(default, deserialize_with = "InputSchema::deserialize_items")]
    items: Option<Box<InputSchema>>,
    #[serde(default)]
    min_items: Option<u64>,
    #[serde(default)]
    max_items: Option<u64>,
    #[serde(default)]
    one_of: Vec<InputSchema>,
    #[serde(default)]
    reference: Option<String>,
    #[serde(default)]
    constant: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
pub enum InputSchemaType {
    Object,
    String,
    Integer,
    Number,
    Boolean,
    Array,
}

impl InputSchemaType {
    fn json_name(self) -> &'static str {
        match self {
            Self::Object => "object",
            Self::String => "string",
            Self::Integer => "integer",
            Self::Number => "number",
            Self::Boolean => "boolean",
            Self::Array => "array",
        }
    }

    fn accepts(self, value: &Value) -> bool {
        match self {
            Self::Object => value.is_object(),
            Self::String => value.is_string(),
            Self::Integer => value.as_i64().is_some() || value.as_u64().is_some(),
            Self::Number => value.is_number(),
            Self::Boolean => value.is_boolean(),
            Self::Array => value.is_array(),
        }
    }
}

impl InputSchema {
    pub fn object() -> Self {
        Self::new(Some(InputSchemaType::Object))
    }

    pub fn string() -> Self {
        Self::new(Some(InputSchemaType::String))
    }

    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn optional(mut self, name: impl Into<String>, schema: Self) -> Self {
        self.properties.insert(name.into(), schema);
        self
    }

    pub fn required(mut self, name: impl Into<String>, schema: Self) -> Self {
        let name = name.into();
        self.required.push(name.clone());
        self.properties.insert(name, schema);
        self
    }

    pub fn to_json(&self) -> Value {
        let mut object = serde_json::Map::new();
        if let Some(schema_type) = self.schema_type {
            object.insert(
                "type".to_string(),
                Value::String(schema_type.json_name().to_string()),
            );
        }
        if let Some(description) = &self.description {
            object.insert(
                "description".to_string(),
                Value::String(description.clone()),
            );
        }
        if self.schema_type == Some(InputSchemaType::Object) {
            if !self.required.is_empty() {
                object.insert("required".to_string(), serde_json::json!(self.required));
            }
            let properties = self
                .properties
                .iter()
                .map(|(name, schema)| (name.clone(), schema.to_json()))
                .collect();
            object.insert("properties".to_string(), Value::Object(properties));
            object.insert(
                "additionalProperties".to_string(),
                Value::Bool(self.additional_properties),
            );
        }
        if !self.definitions.is_empty() {
            let definitions = self
                .definitions
                .iter()
                .map(|(name, schema)| (name.clone(), schema.to_json()))
                .collect();
            object.insert("$defs".to_string(), Value::Object(definitions));
        }
        if !self.values.is_empty() {
            object.insert("enum".to_string(), serde_json::json!(self.values));
        }
        if let Some(min_length) = self.min_length {
            object.insert("minLength".to_string(), Value::from(min_length));
        }
        if let Some(max_length) = self.max_length {
            object.insert("maxLength".to_string(), Value::from(max_length));
        }
        if let Some(minimum) = self.minimum {
            object.insert("minimum".to_string(), Value::from(minimum));
        }
        if let Some(maximum) = self.maximum {
            object.insert("maximum".to_string(), Value::from(maximum));
        }
        if let Some(items) = &self.items {
            object.insert("items".to_string(), items.to_json());
        }
        if let Some(min_items) = self.min_items {
            object.insert("minItems".to_string(), Value::from(min_items));
        }
        if let Some(max_items) = self.max_items {
            object.insert("maxItems".to_string(), Value::from(max_items));
        }
        if !self.one_of.is_empty() {
            object.insert(
                "oneOf".to_string(),
                Value::Array(self.one_of.iter().map(Self::to_json).collect()),
            );
        }
        if let Some(reference) = &self.reference {
            object.insert("$ref".to_string(), Value::String(reference.clone()));
        }
        if let Some(constant) = &self.constant {
            object.insert("const".to_string(), Value::String(constant.clone()));
        }
        Value::Object(object)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.schema_type == Some(InputSchemaType::Array) && self.items.is_none() {
            return Err("array input schema must define items".to_string());
        }
        if self.schema_type.is_none()
            && self.one_of.is_empty()
            && self.reference.is_none()
            && self.constant.is_none()
            && self.description.is_none()
        {
            return Err("input schema must define a type or schema keyword".to_string());
        }
        for schema in self.properties.values() {
            schema.validate()?;
        }
        for schema in self.definitions.values() {
            schema.validate()?;
        }
        if let Some(items) = &self.items {
            items.validate()?;
        }
        for schema in &self.one_of {
            schema.validate()?;
        }
        Ok(())
    }

    pub fn validate_value(&self, value: &Value) -> Result<(), String> {
        self.validate_value_with_root(value, self)
    }

    fn validate_value_with_root(&self, value: &Value, root: &Self) -> Result<(), String> {
        if let Some(reference) = &self.reference {
            let name = reference
                .strip_prefix("#/$defs/")
                .ok_or_else(|| format!("unsupported input schema reference: {reference}"))?;
            let schema = root
                .definitions
                .get(name)
                .ok_or_else(|| format!("unknown input schema reference: {reference}"))?;
            schema.validate_value_with_root(value, root)?;
        }
        if let Some(schema_type) = self.schema_type
            && !schema_type.accepts(value)
        {
            return Err(format!("expected {}", schema_type.json_name()));
        }
        if let Some(constant) = &self.constant
            && value.as_str() != Some(constant)
        {
            return Err(format!("expected {constant}"));
        }
        if !self.values.is_empty()
            && !value
                .as_str()
                .is_some_and(|value| self.values.iter().any(|allowed| allowed == value))
        {
            return Err(format!("expected one of {}", self.values.join(", ")));
        }
        if let Some(value) = value.as_str() {
            let length = value.chars().count() as u64;
            if self.min_length.is_some_and(|minimum| length < minimum) {
                return Err(format!(
                    "must contain at least {} characters",
                    self.min_length.unwrap()
                ));
            }
            if self.max_length.is_some_and(|maximum| length > maximum) {
                return Err(format!(
                    "must contain at most {} characters",
                    self.max_length.unwrap()
                ));
            }
        }
        if let Some(value) = value.as_i64() {
            if self.minimum.is_some_and(|minimum| value < minimum) {
                return Err(format!("must be at least {}", self.minimum.unwrap()));
            }
            if self.maximum.is_some_and(|maximum| value > maximum) {
                return Err(format!("must be at most {}", self.maximum.unwrap()));
            }
        }
        if let Some(values) = value.as_array() {
            if self
                .min_items
                .is_some_and(|minimum| values.len() < minimum as usize)
            {
                return Err(format!(
                    "must contain at least {} items",
                    self.min_items.unwrap()
                ));
            }
            if self
                .max_items
                .is_some_and(|maximum| values.len() > maximum as usize)
            {
                return Err(format!(
                    "must contain at most {} items",
                    self.max_items.unwrap()
                ));
            }
            if let Some(items) = &self.items {
                for (index, value) in values.iter().enumerate() {
                    items
                        .validate_value_with_root(value, root)
                        .map_err(|error| format!("item {index}: {error}"))?;
                }
            }
        }
        if let Some(object) = value.as_object() {
            for required in &self.required {
                if !object.contains_key(required) {
                    return Err(format!("{required} is required"));
                }
            }
            if !self.additional_properties {
                for name in object.keys() {
                    if !self.properties.contains_key(name) {
                        return Err(format!("unknown argument {name}"));
                    }
                }
            }
            for (name, schema) in &self.properties {
                if let Some(value) = object.get(name) {
                    schema
                        .validate_value_with_root(value, root)
                        .map_err(|error| format!("{name}: {error}"))?;
                }
            }
        }
        if !self.one_of.is_empty() {
            let matches = self
                .one_of
                .iter()
                .filter(|schema| schema.validate_value_with_root(value, root).is_ok())
                .count();
            if matches != 1 {
                return Err("must match exactly one input shape".to_string());
            }
        }
        Ok(())
    }

    fn deserialize_type<'de, D>(deserializer: D) -> Result<Option<InputSchemaType>, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        InputSchemaType::deserialize(deserializer).map(Some)
    }

    fn deserialize_items<'de, D>(deserializer: D) -> Result<Option<Box<Self>>, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Self::deserialize(deserializer).map(|schema| Some(Box::new(schema)))
    }

    fn new(schema_type: Option<InputSchemaType>) -> Self {
        Self {
            schema_type,
            description: None,
            required: Vec::new(),
            properties: BTreeMap::new(),
            additional_properties: false,
            definitions: BTreeMap::new(),
            values: Vec::new(),
            min_length: None,
            max_length: None,
            minimum: None,
            maximum: None,
            items: None,
            min_items: None,
            max_items: None,
            one_of: Vec::new(),
            reference: None,
            constant: None,
        }
    }

    fn strings(value: Value) -> Result<Vec<String>, String> {
        value
            .as_array()
            .ok_or("input schema value must be an array")?
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .map(str::to_string)
                    .ok_or("input schema array values must be strings".to_string())
            })
            .collect()
    }

    fn schemas(value: Value) -> Result<BTreeMap<String, Self>, String> {
        let Value::Object(object) = value else {
            return Err("input schema properties must be an object".to_string());
        };
        object
            .into_iter()
            .map(|(name, schema)| Self::try_from(schema).map(|schema| (name, schema)))
            .collect()
    }

    fn take_u64(
        object: &mut serde_json::Map<String, Value>,
        name: &str,
    ) -> Result<Option<u64>, String> {
        object
            .remove(name)
            .map(|value| {
                value
                    .as_u64()
                    .ok_or_else(|| format!("input schema {name} must be an unsigned integer"))
            })
            .transpose()
    }

    fn take_i64(
        object: &mut serde_json::Map<String, Value>,
        name: &str,
    ) -> Result<Option<i64>, String> {
        object
            .remove(name)
            .map(|value| {
                value
                    .as_i64()
                    .ok_or_else(|| format!("input schema {name} must be an integer"))
            })
            .transpose()
    }
}

impl TryFrom<Value> for InputSchema {
    type Error = String;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        let Value::Object(mut object) = value else {
            return Err("input schema must be an object".to_string());
        };
        let schema_type = object
            .remove("type")
            .map(|value| {
                let Some(value) = value.as_str() else {
                    return Err("input schema type must be a string".to_string());
                };
                match value {
                    "object" => Ok(InputSchemaType::Object),
                    "string" => Ok(InputSchemaType::String),
                    "integer" => Ok(InputSchemaType::Integer),
                    "number" => Ok(InputSchemaType::Number),
                    "boolean" => Ok(InputSchemaType::Boolean),
                    "array" => Ok(InputSchemaType::Array),
                    _ => Err(format!("unsupported input schema type: {value}")),
                }
            })
            .transpose()?;
        let mut schema = Self::new(schema_type);
        schema.description = object
            .remove("description")
            .map(|value| {
                value
                    .as_str()
                    .map(str::to_string)
                    .ok_or("input schema description must be a string".to_string())
            })
            .transpose()?;
        schema.required = object
            .remove("required")
            .map(Self::strings)
            .transpose()?
            .unwrap_or_default();
        schema.properties = object
            .remove("properties")
            .map(Self::schemas)
            .transpose()?
            .unwrap_or_default();
        schema.additional_properties = object
            .remove("additionalProperties")
            .map(|value| {
                value
                    .as_bool()
                    .ok_or("input schema additionalProperties must be a boolean".to_string())
            })
            .transpose()?
            .unwrap_or_default();
        schema.definitions = object
            .remove("$defs")
            .map(Self::schemas)
            .transpose()?
            .unwrap_or_default();
        schema.values = object
            .remove("enum")
            .map(Self::strings)
            .transpose()?
            .unwrap_or_default();
        schema.min_length = Self::take_u64(&mut object, "minLength")?;
        schema.max_length = Self::take_u64(&mut object, "maxLength")?;
        schema.minimum = Self::take_i64(&mut object, "minimum")?;
        schema.maximum = Self::take_i64(&mut object, "maximum")?;
        schema.items = object
            .remove("items")
            .map(Self::try_from)
            .transpose()?
            .map(Box::new);
        schema.min_items = Self::take_u64(&mut object, "minItems")?;
        schema.max_items = Self::take_u64(&mut object, "maxItems")?;
        schema.one_of = object
            .remove("oneOf")
            .or_else(|| object.remove("anyOf"))
            .map(|value| {
                value
                    .as_array()
                    .ok_or("input schema oneOf must be an array")?
                    .iter()
                    .cloned()
                    .map(Self::try_from)
                    .collect()
            })
            .transpose()?
            .unwrap_or_default();
        schema.reference = object
            .remove("$ref")
            .map(|value| {
                value
                    .as_str()
                    .map(str::to_string)
                    .ok_or("input schema reference must be a string".to_string())
            })
            .transpose()?;
        schema.constant = object
            .remove("const")
            .map(|value| {
                value
                    .as_str()
                    .map(str::to_string)
                    .ok_or("input schema const must be a string".to_string())
            })
            .transpose()?;
        if let Some(name) = object.keys().next() {
            return Err(format!("unsupported input schema keyword: {name}"));
        }
        schema.validate()?;
        Ok(schema)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_object_schema_rejects_unknown_and_invalid_fields() {
        let schema = InputSchema::object().optional("url", InputSchema::string());

        assert!(schema.validate_value(&serde_json::json!({})).is_ok());
        assert!(
            schema
                .validate_value(&serde_json::json!({"url": "https://vmux.ai"}))
                .is_ok()
        );
        assert!(
            schema
                .validate_value(&serde_json::json!({"url": 42}))
                .is_err()
        );
        assert!(
            schema
                .validate_value(&serde_json::json!({"other": true}))
                .is_err()
        );
        assert!(schema.validate_value(&Value::Null).is_err());
    }
}
