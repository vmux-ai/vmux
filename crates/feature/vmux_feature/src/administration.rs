use std::collections::BTreeMap;

use crate::flag::Configuration;
use crate::snapshot::WireFlag;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Capabilities {
    pub edit: bool,
    pub review: bool,
    pub publish: bool,
    #[serde(default)]
    pub audit: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(bound(serialize = "F: Serialize", deserialize = "F: Deserialize<'de>"))]
pub struct Snapshot<F = Configuration> {
    pub schema_version: u32,
    pub environment: String,
    pub revision: u64,
    pub capabilities: Capabilities,
    #[serde(with = "serde_with::rust::maps_duplicate_key_is_error")]
    pub flags: BTreeMap<String, F>,
    pub draft: Option<Draft<F>>,
    pub history: Vec<Publication<F>>,
}

impl Snapshot {
    pub fn decode(bytes: &[u8], environment: &str) -> Result<Self, String> {
        if bytes.len() > 2 * 1024 * 1024 {
            return Err("administration response exceeds 2 MiB".into());
        }
        let wire: Snapshot<WireFlag> =
            serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
        let snapshot = Self::try_from(wire)?;
        if !matches!(snapshot.schema_version, 1 | 2)
            || snapshot.environment != environment
            || snapshot.revision == 0
            || snapshot.history.len() > 128
            || !Self::valid_values(&snapshot.flags)
        {
            return Err("invalid administration snapshot".into());
        }
        if let Some(draft) = &snapshot.draft
            && (draft.id.is_empty()
                || draft.id.len() > 128
                || draft.revision == 0
                || draft.base_revision == 0
                || draft.base_revision > snapshot.revision
                || snapshot
                    .flags
                    .keys()
                    .any(|key| !draft.flags.contains_key(key))
                || !Self::valid_values(&draft.flags)
                || draft
                    .reviewed_revision
                    .is_some_and(|revision| revision != draft.revision))
        {
            return Err("invalid administration draft".into());
        }
        let mut previous = 0;
        for entry in &snapshot.history {
            if entry.revision <= previous
                || entry.revision > snapshot.revision
                || entry.operation_id.is_empty()
                || entry.operation_id.len() > 128
                || entry.draft_id.is_empty()
                || entry.draft_id.len() > 128
                || entry.draft_revision == 0
                || entry.actor.is_empty()
                || entry.actor.len() > 128
                || !Self::valid_values(&entry.flags)
            {
                return Err("invalid administration history".into());
            }
            previous = entry.revision;
        }
        if snapshot.history.last().is_some_and(|entry| {
            entry.revision != snapshot.revision || entry.flags != snapshot.flags
        }) {
            return Err("administration history does not match the published snapshot".into());
        }
        Ok(snapshot)
    }

    fn valid_values(flags: &BTreeMap<String, Configuration>) -> bool {
        if flags.len() > 128 {
            return false;
        }
        for (key, configuration) in flags {
            if key.is_empty()
                || key.len() > 128
                || !key.bytes().all(|byte| {
                    byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._-".contains(&byte)
                })
            {
                return false;
            }
            if configuration.validate().is_err() {
                return false;
            }
        }
        true
    }

    fn decode_values(
        schema: u32,
        values: BTreeMap<String, WireFlag>,
    ) -> Result<BTreeMap<String, Configuration>, String> {
        let mut result = BTreeMap::new();
        for (key, value) in values {
            result.insert(key, value.decode(schema)?);
        }
        Ok(result)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(bound(serialize = "F: Serialize", deserialize = "F: Deserialize<'de>"))]
pub struct Draft<F = Configuration> {
    pub id: String,
    pub revision: u64,
    pub base_revision: u64,
    pub reviewed_revision: Option<u64>,
    #[serde(with = "serde_with::rust::maps_duplicate_key_is_error")]
    pub flags: BTreeMap<String, F>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(bound(serialize = "F: Serialize", deserialize = "F: Deserialize<'de>"))]
pub struct Publication<F = Configuration> {
    pub operation_id: String,
    pub draft_id: String,
    pub draft_revision: u64,
    pub revision: u64,
    pub actor: String,
    pub created_at: u64,
    #[serde(with = "serde_with::rust::maps_duplicate_key_is_error")]
    pub flags: BTreeMap<String, F>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ecs", derive(bevy_ecs::prelude::Component))]
#[cfg_attr(feature = "ecs", component(immutable))]
#[serde(bound(serialize = "F: Serialize", deserialize = "F: Deserialize<'de>"))]
pub struct Save<F: Send + Sync + 'static = Configuration> {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub operation_id: String,
    pub draft_id: String,
    pub expected_revision: u64,
    pub base_revision: u64,
    #[serde(with = "serde_with::rust::maps_duplicate_key_is_error")]
    pub changes: BTreeMap<String, F>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ecs", derive(bevy_ecs::prelude::Component))]
#[cfg_attr(feature = "ecs", component(immutable))]
pub struct Review {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub operation_id: String,
    pub draft_id: String,
    pub expected_revision: u64,
    pub base_revision: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ecs", derive(bevy_ecs::prelude::Component))]
#[cfg_attr(feature = "ecs", component(immutable))]
pub struct Publish {
    pub operation_id: String,
    pub draft_id: String,
    pub expected_revision: u64,
    pub base_revision: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ecs", derive(bevy_ecs::prelude::Component))]
#[cfg_attr(feature = "ecs", component(immutable))]
pub struct Discard {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub operation_id: String,
    pub draft_id: String,
    pub expected_revision: u64,
}

impl TryFrom<Snapshot<WireFlag>> for Snapshot {
    type Error = String;

    fn try_from(wire: Snapshot<WireFlag>) -> Result<Self, Self::Error> {
        let schema = wire.schema_version;
        let flags = Self::decode_values(schema, wire.flags)?;
        let draft = match wire.draft {
            Some(draft) => Some(Draft {
                id: draft.id,
                revision: draft.revision,
                base_revision: draft.base_revision,
                reviewed_revision: draft.reviewed_revision,
                flags: Self::decode_values(schema, draft.flags)?,
            }),
            None => None,
        };
        let mut history = Vec::new();
        for publication in wire.history {
            history.push(Publication {
                operation_id: publication.operation_id,
                draft_id: publication.draft_id,
                draft_revision: publication.draft_revision,
                revision: publication.revision,
                actor: publication.actor,
                created_at: publication.created_at,
                flags: Self::decode_values(schema, publication.flags)?,
            });
        }
        Ok(Self {
            schema_version: schema,
            environment: wire.environment,
            revision: wire.revision,
            capabilities: wire.capabilities,
            flags,
            draft,
            history,
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ecs", derive(bevy_ecs::prelude::Component))]
#[cfg_attr(feature = "ecs", component(immutable))]
pub struct Rollback {
    pub operation_id: String,
    pub base_revision: u64,
    pub target_revision: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListAudit {
    pub before: Option<u64>,
    pub limit: u16,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuditPage {
    pub environment: String,
    pub publications: Vec<Publication>,
    pub next: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    Invalid,
    Unauthenticated,
    Forbidden,
    Conflict,
    NotFound,
    Unavailable,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Refusal {
    pub kind: ErrorKind,
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn administration_rejects_cross_environment_and_malformed_review() {
        let mut value = serde_json::json!({
            "schema_version": 1, "environment": "development", "revision": 3,
            "capabilities": { "edit": true, "review": true, "publish": true },
            "flags": {"account.cloud": false}, "draft": null, "history": []
        });
        let bytes = serde_json::to_vec(&value).unwrap();
        assert!(Snapshot::decode(&bytes, "development").is_ok());
        assert!(Snapshot::decode(&bytes, "production").is_err());
        value["draft"] = serde_json::json!({"id": "draft", "revision": 2, "base_revision": 3, "reviewed_revision": 1, "flags": {"account.cloud":true}});
        assert!(Snapshot::decode(&serde_json::to_vec(&value).unwrap(), "development").is_err());
    }

    #[test]
    fn administration_refuses_history_that_rewinds_revision() {
        let value = serde_json::json!({
            "schema_version": 1, "environment": "development", "revision": 3,
            "capabilities": { "edit": false, "review": false, "publish": false },
            "flags": {}, "draft": null, "history": [
                {"operation_id":"a", "draft_id":"d", "draft_revision":1, "revision":3, "actor":"operator", "created_at":1, "flags":{}},
                {"operation_id":"b", "draft_id":"e", "draft_revision":1, "revision":2, "actor":"operator", "created_at":2, "flags":{}}
            ]
        });
        assert!(Snapshot::decode(&serde_json::to_vec(&value).unwrap(), "development").is_err());
    }
}
