use bevy_ecs::prelude::*;
#[cfg(feature = "profile-sync")]
use serde::{Deserialize, Serialize};
use uuid::Uuid;
#[cfg(feature = "profile-sync")]
use vmux_ecs::record::Record;

use crate::wire::ProfileChange;

#[derive(Message)]
pub(crate) struct ListRequest {
    pub entity: Entity,
    pub connection: Entity,
    pub after: Option<Uuid>,
}

#[derive(Message)]
pub(crate) struct UploadRequest {
    pub entity: Entity,
    pub connection: Entity,
    pub change: ProfileChange,
}

#[cfg(feature = "profile-sync")]
#[derive(Component, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Origin {
    pub server: String,
    pub account: Uuid,
}

#[cfg(feature = "profile-sync")]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Pending {
    pub operation_id: Uuid,
    pub profile_id: Uuid,
    pub origin: Origin,
    pub local_revision: u64,
    pub name: Option<String>,
    pub base_revision: Option<u64>,
    pub conflict: Option<Record>,
    #[serde(default)]
    pub conflicted: bool,
}

#[cfg(feature = "profile-sync")]
#[derive(Clone, Debug)]
pub(crate) struct Checkpoint {
    pub profile_id: Uuid,
    pub remote_revision: u64,
    pub local_revision: u64,
    pub deleted: bool,
}

#[cfg(feature = "profile-sync")]
#[derive(Component, Clone, Debug, Serialize, Deserialize)]
pub struct CheckpointChange {
    pub remote_revision: u64,
    pub through_local_revision: u64,
    pub deleted: bool,
    #[serde(default)]
    pub resolves: Option<Uuid>,
}

#[cfg(feature = "profile-sync")]
#[derive(Component, Clone, Debug, Serialize, Deserialize)]
pub struct ExpectedScope {
    pub account_id: Uuid,
    pub binding_revision: u64,
    pub origin: Origin,
}

#[cfg(feature = "profile-sync")]
impl ExpectedScope {
    pub(crate) fn matches(
        &self,
        account: &vmux_ecs::Uuid,
        binding: Option<&crate::Binding>,
        revision: u64,
    ) -> bool {
        self.account_id.to_string() == account.0
            && self.binding_revision == revision
            && binding.is_some_and(|binding| {
                binding.is_linked()
                    && binding.server == self.origin.server
                    && binding.account_id.as_deref()
                        == Some(self.origin.account.to_string().as_str())
            })
    }
}
