use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
#[cfg(any(feature = "ecs", test))]
use sha2::{Digest, Sha256};

use crate::flag::Configuration;

#[derive(Clone, Debug, PartialEq)]
pub struct Document {
    pub schema_version: u32,
    pub environment: String,
    pub revision: u64,
    pub issued_at: u64,
    pub valid_until: u64,
    pub flags: BTreeMap<String, Configuration>,
}

impl Document {
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > 65536 {
            return Err("feature snapshot exceeds 64 KiB".into());
        }
        let document: Self = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
        if document.flags.len() > 128 {
            return Err("too many feature flags".into());
        }
        Ok(document)
    }

    #[cfg(any(feature = "ecs", test))]
    pub(crate) fn fingerprint(&self) -> [u8; 32] {
        let mut digest = Sha256::new();
        for (key, configuration) in &self.flags {
            digest.update((key.len() as u64).to_be_bytes());
            digest.update(key.as_bytes());
            configuration.hash(&mut digest);
        }
        digest.finalize().into()
    }

    pub fn is_valid(&self, environment: &str, now: u64) -> bool {
        if now == 0
            || self.environment.is_empty()
            || self.environment.len() > 64
            || !matches!(self.schema_version, 1 | 2)
            || self.environment != environment
            || self.revision == 0
            || self.issued_at > now
            || self.valid_until <= now
            || self.valid_until <= self.issued_at
            || self.valid_until - self.issued_at > 3600
            || self.flags.len() > 128
        {
            return false;
        }
        for (key, configuration) in &self.flags {
            if key.is_empty()
                || key.len() > 128
                || !key.bytes().all(|byte| {
                    byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._-".contains(&byte)
                })
            {
                return false;
            }
            if configuration.validate().is_err()
                || self.schema_version == 1 && configuration.legacy_boolean().is_none()
            {
                return false;
            }
        }
        true
    }
}

impl Serialize for Document {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let wire = WireDocument::new(self).map_err(serde::ser::Error::custom)?;
        wire.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Document {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::try_from(WireDocument::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

impl TryFrom<WireDocument> for Document {
    type Error = String;

    fn try_from(wire: WireDocument) -> Result<Self, Self::Error> {
        if !matches!(wire.schema_version, 1 | 2) {
            return Err("unsupported feature snapshot schema".into());
        }
        let mut flags = BTreeMap::new();
        for (key, value) in wire.flags {
            let configuration = value.decode(wire.schema_version)?;
            flags.insert(key, configuration);
        }
        Ok(Self {
            schema_version: wire.schema_version,
            environment: wire.environment,
            revision: wire.revision,
            issued_at: wire.issued_at,
            valid_until: wire.valid_until,
            flags,
        })
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireDocument {
    schema_version: u32,
    environment: String,
    revision: u64,
    issued_at: u64,
    valid_until: u64,
    #[serde(with = "serde_with::rust::maps_duplicate_key_is_error")]
    flags: BTreeMap<String, WireFlag>,
}

impl WireDocument {
    fn new(document: &Document) -> Result<Self, String> {
        let mut flags = BTreeMap::new();
        for (key, configuration) in &document.flags {
            let flag = match document.schema_version {
                1 => WireFlag::Boolean(
                    configuration
                        .legacy_boolean()
                        .ok_or("typed flag cannot use snapshot schema 1")?,
                ),
                2 => WireFlag::Typed(configuration.clone()),
                _ => return Err("unsupported feature snapshot schema".into()),
            };
            flags.insert(key.clone(), flag);
        }
        Ok(Self {
            schema_version: document.schema_version,
            environment: document.environment.clone(),
            revision: document.revision,
            issued_at: document.issued_at,
            valid_until: document.valid_until,
            flags,
        })
    }
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
pub(crate) enum WireFlag {
    Boolean(bool),
    Typed(Configuration),
}

impl WireFlag {
    pub(crate) fn decode(self, schema: u32) -> Result<Configuration, String> {
        let configuration = match (schema, self) {
            (1, Self::Boolean(value)) => Configuration::boolean(value),
            (2, Self::Typed(configuration)) => configuration,
            _ => return Err("flag value does not match its wire schema".into()),
        };
        configuration.validate()?;
        Ok(configuration)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_and_typed_boolean_snapshots_share_one_configuration_and_fingerprint() {
        let legacy = r#"{"schema_version":1,"environment":"production","revision":1,"issued_at":1000,"valid_until":1100,"flags":{"account.cloud":true}}"#;
        let typed = r#"{"schema_version":2,"environment":"production","revision":1,"issued_at":1000,"valid_until":1100,"flags":{"account.cloud":{"value":{"Boolean":true}}}}"#;
        let first = Document::decode(legacy.as_bytes()).unwrap();
        let second = Document::decode(typed.as_bytes()).unwrap();
        assert_eq!(first.flags, second.flags);
        assert_eq!(first.fingerprint(), second.fingerprint());
        assert!(first.is_valid("production", 1000));
        assert!(second.is_valid("production", 1000));
        let roundtrip = serde_json::to_value(&first).unwrap();
        assert_eq!(
            roundtrip,
            serde_json::from_str::<serde_json::Value>(legacy).unwrap()
        );
        for invalid in [
            legacy.replace("\"schema_version\":1", "\"schema_version\":2"),
            typed.replace("\"schema_version\":2", "\"schema_version\":1"),
        ] {
            assert!(Document::decode(invalid.as_bytes()).is_err());
        }
    }

    #[test]
    fn typed_snapshot_values_survive_roundtrip_and_keep_rule_order() {
        let source = r#"{
            "schema_version":2,"environment":"development","revision":7,"issued_at":1000,"valid_until":1100,
            "flags":{
                "a.boolean":{"value":{"Boolean":false}},
                "b.string":{"value":{"String":"default"},"rules":[
                    {"value":{"String":"first"},"variant":"one"},
                    {"value":{"String":"second"},"variant":"two"}
                ]},
                "c.integer":{"value":{"Integer":9223372036854775807}},
                "d.float":{"value":{"Float":1.25}},
                "e.json":{"value":{"Json":{"nested":[true,"value",1]}}}
            }
        }"#;
        let document = Document::decode(source.as_bytes()).unwrap();
        assert!(document.is_valid("development", 1000));
        let bytes = serde_json::to_vec(&document).unwrap();
        let restored = Document::decode(&bytes).unwrap();
        assert_eq!(restored, document);
        assert_eq!(restored.fingerprint(), document.fingerprint());
        let encoded: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            encoded["flags"]["c.integer"]["value"]["Integer"].as_i64(),
            Some(i64::MAX)
        );
        let mut changed = document.clone();
        changed.flags.get_mut("b.string").unwrap().rules.reverse();
        assert_ne!(changed.fingerprint(), document.fingerprint());
    }

    #[test]
    fn schema_two_refuses_duplicate_keys_invalid_rules_and_unknown_fields() {
        let valid = r#"{"schema_version":2,"environment":"development","revision":7,"issued_at":1000,"valid_until":1100,"flags":{"remote.live":{"value":{"Boolean":false}}}}"#;
        for invalid in [
            valid.replace("\"remote.live\":{\"value\":{\"Boolean\":false}}", "\"remote.live\":{\"value\":{\"Boolean\":false}},\"remote.live\":{\"value\":{\"Boolean\":true}}"),
            valid.replace("\"Boolean\":false", "\"Boolean\":\"false\""),
            valid.replace("\"value\":{\"Boolean\":false}", "\"value\":{\"Boolean\":false},\"unknown\":true"),
            valid.replace("\"value\":{\"Boolean\":false}", "\"value\":{\"Boolean\":false},\"rules\":[{\"value\":{\"String\":\"on\"}}]"),
        ] {
            assert!(Document::decode(invalid.as_bytes()).is_err());
        }
    }

    #[test]
    fn ambiguous_or_coerced_wire_values_are_rejected() {
        for flags in [
            r#"{"account.cloud":false,"account.cloud":true}"#,
            r#"{"account.cloud":"true"}"#,
            r#"{"account.cloud":1}"#,
        ] {
            let body = format!(
                r#"{{"schema_version":1,"environment":"production","revision":1,"issued_at":1000,"valid_until":1100,"flags":{flags}}}"#
            );
            assert!(Document::decode(body.as_bytes()).is_err());
        }
    }

    #[test]
    fn unsupported_expired_or_unbounded_documents_never_become_valid() {
        let source = r#"{"schema_version":1,"environment":"production","revision":1,"issued_at":1000,"valid_until":1100,"flags":{"account.cloud":true}}"#;
        let document = Document::decode(source.as_bytes()).unwrap();
        assert!(document.is_valid("production", 1000));
        assert!(!document.is_valid("staging", 1000));
        assert!(!document.is_valid("production", 1100));
        assert!(!document.is_valid("production", 999));
        for (field, value) in [("revision", 0), ("valid_until", 4601)] {
            let mut invalid: serde_json::Value = serde_json::from_str(source).unwrap();
            invalid[field] = value.into();
            let invalid = Document::decode(&serde_json::to_vec(&invalid).unwrap()).unwrap();
            assert!(!invalid.is_valid("production", 1000));
        }
        assert!(Document::decode(&vec![b' '; 65537]).is_err());
        assert!(
            Document::decode(
                source
                    .replace("\"schema_version\":1", "\"schema_version\":3")
                    .as_bytes()
            )
            .is_err()
        );
    }
}
