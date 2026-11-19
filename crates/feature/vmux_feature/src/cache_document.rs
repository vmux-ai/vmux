use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::flag::Assignment;
use crate::snapshot::Document;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Envelope {
    pub(crate) schema_version: u32,
    pub(crate) sequence: u64,
    pub(crate) operation: Option<Uuid>,
    pub(crate) snapshots: Vec<SavedSnapshot>,
    pub(crate) retained: Vec<OffRecord>,
    #[serde(default)]
    pub(crate) assignments: Vec<Assignment>,
    #[serde(default)]
    pub(crate) clear: Option<ClearJournal>,
}

impl Envelope {
    pub(crate) fn empty() -> Self {
        Self {
            schema_version: 2,
            sequence: 0,
            operation: None,
            snapshots: Vec::new(),
            retained: Vec::new(),
            assignments: Vec::new(),
            clear: None,
        }
    }

    pub(crate) fn decode(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > 16 * 1024 * 1024 {
            return Err("feature cache exceeds 16 MiB".into());
        }
        let envelope: Self = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
        envelope.validate()?;
        Ok(envelope)
    }

    pub(crate) fn encode(&self) -> Result<Vec<u8>, String> {
        self.validate()?;
        let bytes = serde_json::to_vec(self).map_err(|error| error.to_string())?;
        if bytes.len() > 16 * 1024 * 1024 {
            return Err("feature cache exceeds 16 MiB".into());
        }
        Ok(bytes)
    }

    fn validate(&self) -> Result<(), String> {
        if !matches!(self.schema_version, 1 | 2)
            || self.snapshots.len() > 256
            || self.retained.len() > 2048
            || self.assignments.len() > 4096
        {
            return Err("unsupported or oversized feature cache".into());
        }
        if self.sequence == 0 {
            if self.operation.is_some()
                || !self.snapshots.is_empty()
                || !self.retained.is_empty()
                || !self.assignments.is_empty()
            {
                return Err("nonempty feature cache needs a storage sequence".into());
            }
        } else if self.operation.is_none() {
            return Err("committed feature cache needs an operation identity".into());
        }
        let mut snapshots = BTreeSet::new();
        if let Some(clear) = &self.clear
            && (clear.operation.is_nil()
                || clear.revision <= clear.previous.revision
                || !self.snapshots.iter().any(|snapshot| {
                    snapshot.document.environment == clear.previous.environment
                        && snapshot.document.revision == clear.revision
                })
                || !clear.admitted
                    && !self.retained.iter().any(|record| {
                        record.environment == clear.previous.environment
                            && record.installation == clear.previous.installation
                            && record.flag == clear.previous.flag
                            && record.revision >= clear.previous.revision
                    }))
        {
            return Err("invalid retained-disable clear journal".into());
        }
        let mut assignments = BTreeSet::new();
        for assignment in &self.assignments {
            assignment.validate()?;
            if !assignments.insert((
                &assignment.environment,
                &assignment.generation,
                &assignment.flag,
                assignment.subject.kind.as_str(),
                &assignment.subject.key,
            )) {
                return Err("duplicate experiment assignment".into());
            }
        }
        for snapshot in &self.snapshots {
            if snapshot.identity.is_empty()
                || snapshot.identity.len() > 256
                || snapshot.identity.chars().any(char::is_control)
                || !snapshot
                    .document
                    .is_valid(&snapshot.document.environment, snapshot.document.issued_at)
                || !snapshots.insert((&snapshot.document.environment, &snapshot.identity))
            {
                return Err("invalid or duplicate cached snapshot partition".into());
            }
        }
        let mut retained = BTreeSet::new();
        for record in &self.retained {
            if record.environment.is_empty()
                || record.environment.len() > 64
                || record.installation.is_empty()
                || record.installation.len() > 256
                || record.flag.is_empty()
                || record.flag.len() > 128
                || !record.flag.bytes().all(|byte| {
                    byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._-".contains(&byte)
                })
                || record.revision == 0
                || !retained.insert((&record.environment, &record.installation, &record.flag))
            {
                return Err("invalid or duplicate retained disable".into());
            }
        }
        Ok(())
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ClearJournal {
    pub(crate) operation: Uuid,
    pub(crate) previous: OffRecord,
    pub(crate) revision: u64,
    pub(crate) admitted: bool,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SavedSnapshot {
    pub(crate) identity: String,
    pub(crate) document: Document,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OffRecord {
    pub(crate) environment: String,
    pub(crate) installation: String,
    pub(crate) flag: String,
    pub(crate) revision: u64,
}
