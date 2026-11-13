use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use bevy_ecs::prelude::Name;
use serde::{Deserialize, Serialize};
use vmux_ecs::{Cwd, Revision};

use crate::{Repository, Ticket, WriteTaskRequest};

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Snapshot {
    #[serde(default)]
    pub projects: BTreeMap<String, Record>,
    #[serde(default)]
    pub hidden: BTreeSet<String>,
    #[serde(default)]
    pub archived: BTreeSet<String>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Record {
    pub name: String,
    pub directory: PathBuf,
    pub repository: Option<String>,
    pub revision: u64,
}

impl Snapshot {
    pub(crate) fn write_request(self, ticket: Ticket) -> WriteTaskRequest {
        WriteTaskRequest {
            ticket,
            snapshot: self,
        }
    }
}

impl Record {
    pub(crate) fn capture(
        name: &Name,
        directory: &Cwd,
        repository: &Repository,
        revision: &Revision,
    ) -> Self {
        Self {
            name: name.as_str().to_string(),
            directory: directory.0.clone(),
            repository: repository.0.clone(),
            revision: revision.0,
        }
    }
}
