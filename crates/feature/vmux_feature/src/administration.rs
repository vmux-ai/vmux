use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Capabilities {
    pub edit: bool,
    pub review: bool,
    pub publish: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub schema_version: u32,
    pub environment: String,
    pub revision: u64,
    pub capabilities: Capabilities,
    #[serde(with = "serde_with::rust::maps_duplicate_key_is_error")]
    pub flags: BTreeMap<String, bool>,
    pub draft: Option<Draft>,
    pub history: Vec<Publication>,
}

impl Snapshot {
    pub fn decode(bytes: &[u8], environment: &str) -> Result<Self, String> {
        if bytes.len() > 2 * 1024 * 1024 {
            return Err("administration response exceeds 2 MiB".into());
        }
        let snapshot: Self = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
        if snapshot.schema_version != 1
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
                || entry.created_at == 0
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

    fn valid_values(flags: &BTreeMap<String, bool>) -> bool {
        if flags.len() > 128 {
            return false;
        }
        for key in flags.keys() {
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Draft {
    pub id: String,
    pub revision: u64,
    pub base_revision: u64,
    pub reviewed_revision: Option<u64>,
    #[serde(with = "serde_with::rust::maps_duplicate_key_is_error")]
    pub flags: BTreeMap<String, bool>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Publication {
    pub operation_id: String,
    pub draft_id: String,
    pub draft_revision: u64,
    pub revision: u64,
    pub actor: String,
    pub created_at: u64,
    #[serde(with = "serde_with::rust::maps_duplicate_key_is_error")]
    pub flags: BTreeMap<String, bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ecs", derive(bevy_ecs::prelude::Component))]
#[cfg_attr(feature = "ecs", component(immutable))]
pub struct Save {
    pub draft_id: String,
    pub expected_revision: u64,
    pub base_revision: u64,
    #[serde(with = "serde_with::rust::maps_duplicate_key_is_error")]
    pub changes: BTreeMap<String, bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ecs", derive(bevy_ecs::prelude::Component))]
#[cfg_attr(feature = "ecs", component(immutable))]
pub struct Review {
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
    pub draft_id: String,
    pub expected_revision: u64,
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
