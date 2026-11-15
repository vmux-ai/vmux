use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
#[cfg(feature = "ecs")]
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Document {
    pub schema_version: u32,
    pub environment: String,
    pub revision: u64,
    pub issued_at: u64,
    pub valid_until: u64,
    #[serde(with = "serde_with::rust::maps_duplicate_key_is_error")]
    pub flags: BTreeMap<String, bool>,
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

    #[cfg(feature = "ecs")]
    pub(crate) fn fingerprint(&self) -> [u8; 32] {
        let mut digest = Sha256::new();
        for (key, enabled) in &self.flags {
            digest.update((key.len() as u64).to_be_bytes());
            digest.update(key.as_bytes());
            digest.update([u8::from(*enabled)]);
        }
        digest.finalize().into()
    }

    pub fn is_valid(&self, environment: &str, now: u64) -> bool {
        if now == 0
            || self.environment.is_empty()
            || self.environment.len() > 64
            || self.schema_version != 1
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
        for key in self.flags.keys() {
            if key.is_empty()
                || key.len() > 128
                || !key.bytes().all(|byte| {
                    byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._-".contains(&byte)
                })
            {
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        for (field, value) in [
            ("schema_version", 2),
            ("revision", 0),
            ("valid_until", 4601),
        ] {
            let mut invalid: serde_json::Value = serde_json::from_str(source).unwrap();
            invalid[field] = value.into();
            let invalid = Document::decode(&serde_json::to_vec(&invalid).unwrap()).unwrap();
            assert!(!invalid.is_valid("production", 1000));
        }
        assert!(Document::decode(&vec![b' '; 65537]).is_err());
    }
}
