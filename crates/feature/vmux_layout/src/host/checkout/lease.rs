use bevy::prelude::*;
use std::fs::Metadata;
use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;
use vmux_ecs::{Cwd, OperationTarget};
use vmux_git::state::CheckedOutBranch;

#[derive(Component)]
pub(super) struct Checkout;

#[derive(Component)]
pub(super) struct Repository;

#[derive(Component)]
pub(super) struct CommonDirectory(pub PathBuf);

#[derive(Component)]
pub(super) struct InitialRevision(pub String);

#[derive(Component)]
pub(super) struct DeleteBranch;

#[derive(Component)]
pub(super) struct DirectoryGuard {
    pub(super) _directory: std::sync::Arc<std::fs::File>,
}

#[derive(Component)]
pub(super) struct Durable;

#[derive(Component)]
pub(super) struct JournalPath(pub PathBuf);

#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub(super) struct Lease(pub Phase);

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Phase {
    Reserved,
    Prepared,
    Cleaning,
    Committed,
    Retained,
    Released,
}

#[derive(Component, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(super) struct Identity {
    device: u64,
    inode: u64,
}

impl Identity {
    pub(super) fn from_metadata(metadata: &Metadata) -> Self {
        Self {
            device: metadata.dev(),
            inode: metadata.ino(),
        }
    }

    pub(super) fn matches(&self, metadata: &Metadata) -> bool {
        metadata.is_dir() && self.device == metadata.dev() && self.inode == metadata.ino()
    }
}

#[derive(bevy::ecs::system::SystemParam)]
pub(super) struct Leases<'w, 's> {
    pub(super) checkouts: Query<
        'w,
        's,
        (
            Entity,
            &'static Cwd,
            &'static CheckedOutBranch,
            &'static Lease,
            Option<&'static OperationTarget>,
        ),
        With<Checkout>,
    >,
}

impl Leases<'_, '_> {
    pub(super) fn find(&self, path: &std::path::Path) -> Option<(Entity, Phase, Option<Entity>)> {
        for (entity, cwd, _, lease, owner) in &self.checkouts {
            if cwd.0 == path {
                return Some((entity, lease.0, owner.map(OperationTarget::entity)));
            }
        }
        None
    }

    pub(super) fn owns(&self, checkout: Entity, operation: Entity) -> bool {
        match self.checkouts.get(checkout) {
            Ok((_, _, _, _, Some(owner))) => owner.entity() == operation,
            _ => false,
        }
    }
}
