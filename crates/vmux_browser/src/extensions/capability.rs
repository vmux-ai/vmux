use serde::{Deserialize, Serialize};
use std::collections::HashSet;

const EMBEDDED: &str = include_str!("capabilities.ron");

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CapabilityKind {
    Method,
    Event,
    Property,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CapabilityStatus {
    Native,
    Bridged,
    Unsupported { reason: String },
    Untested,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityEntry {
    pub platform: String,
    pub namespace: String,
    pub member: String,
    pub kind: CapabilityKind,
    pub status: CapabilityStatus,
    pub owner: Option<String>,
    pub scenario: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityMatrix {
    pub chromium_major: u32,
    pub entries: Vec<CapabilityEntry>,
}

impl CapabilityMatrix {
    pub fn embedded() -> Result<Self, String> {
        ron::from_str(EMBEDDED).map_err(|error| error.to_string())
    }

    pub fn lookup(
        &self,
        platform: &str,
        namespace: &str,
        member: &str,
        kind: CapabilityKind,
    ) -> Option<&CapabilityEntry> {
        self.entries.iter().find(|entry| {
            entry.platform == platform
                && entry.namespace == namespace
                && entry.member == member
                && entry.kind == kind
        })
    }

    pub fn validate(&self) -> Result<(), String> {
        let mut keys = HashSet::new();
        for entry in &self.entries {
            let key = (
                entry.platform.as_str(),
                entry.namespace.as_str(),
                entry.member.as_str(),
                entry.kind,
            );
            if !keys.insert(key) {
                return Err(format!(
                    "duplicate capability {}.{} on {}",
                    entry.namespace, entry.member, entry.platform
                ));
            }
            if matches!(
                entry.status,
                CapabilityStatus::Native | CapabilityStatus::Bridged
            ) && entry.scenario.as_deref().is_none_or(str::is_empty)
            {
                return Err(format!(
                    "{}.{} on {} is {:?} without a scenario",
                    entry.namespace, entry.member, entry.platform, entry.status
                ));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_matrix_has_unique_entries_for_chromium_148() {
        let matrix = CapabilityMatrix::embedded().unwrap();
        assert_eq!(matrix.chromium_major, 148);
        assert_eq!(
            matrix
                .lookup("macos", "tabs", "query", CapabilityKind::Method)
                .unwrap()
                .status,
            CapabilityStatus::Untested
        );
        matrix.validate().unwrap();
    }

    #[test]
    fn advertised_entries_require_scenarios() {
        let matrix = CapabilityMatrix {
            chromium_major: 148,
            entries: vec![CapabilityEntry {
                platform: "macos".into(),
                namespace: "runtime".into(),
                member: "sendMessage".into(),
                kind: CapabilityKind::Method,
                status: CapabilityStatus::Native,
                owner: Some("cef".into()),
                scenario: None,
            }],
        };
        assert_eq!(
            matrix.validate().unwrap_err(),
            "runtime.sendMessage on macos is Native without a scenario"
        );
    }
}
