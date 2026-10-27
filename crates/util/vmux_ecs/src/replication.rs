use std::collections::{HashMap, HashSet};

use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use bevy_ecs::system::SystemParam;
use bevy_reflect::{Reflect, TypePath, std_traits::ReflectDefault};
use moonshine_save::prelude::Save;
use serde::{Deserialize, Serialize};
use vmux_api::Id;

use crate::persistence::PersistenceAppExt;
use crate::{DeletedAt, Operation, OperationTargetRevision, Revision, UnixMillis};

const DEFAULT_TOMBSTONE_RETENTION_MS: u64 = 30 * 24 * 60 * 60 * 1000;

pub struct ReplicationPlugin;

impl Plugin for ReplicationPlugin {
    fn build(&self, app: &mut App) {
        app.register_persisted::<ReplicationRoot>()
            .register_persisted::<TombstoneRetention>()
            .register_persisted::<CommittedOperation>()
            .register_persisted::<Id<Operation>>()
            .register_persisted::<Id<Actor>>()
            .register_persisted::<OperationTargetRevision>()
            .register_persisted::<CommittedRevision>()
            .register_persisted::<OperationSequence>()
            .register_persisted::<CommittedAt>()
            .register_persisted::<ReplicationSnapshot>()
            .register_persisted::<SnapshotRevision>()
            .register_persisted::<SnapshotSequence>()
            .register_persisted::<Tombstone>()
            .register_persisted::<DeletionRevision>()
            .register_persisted::<DeletionSequence>()
            .register_persisted::<ReplicaAck>()
            .register_persisted::<Id<Replica>>()
            .register_persisted::<AcknowledgedSequence>()
            .init_resource::<ReplicationIndex>()
            .init_resource::<IndexedReplicationEntities>()
            .add_systems(Update, (rebuild, compact).chain());
    }
}

#[derive(TypePath)]
#[type_path = "vmux_ecs"]
pub enum Actor {}

#[derive(TypePath)]
#[type_path = "vmux_ecs"]
pub enum Replica {}

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
#[reflect(Component, Default)]
#[require(Save, Revision = Revision::new(0), TombstoneRetention)]
#[type_path = "vmux_ecs"]
pub struct ReplicationRoot;

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Reflect, Serialize, Deserialize)]
#[reflect(Component)]
#[type_path = "vmux_ecs"]
pub struct TombstoneRetention(u64);

impl TombstoneRetention {
    pub fn new(milliseconds: u64) -> Result<Self, String> {
        if milliseconds == 0 {
            return Err("tombstone retention must be positive".to_string());
        }
        Ok(Self(milliseconds))
    }

    pub const fn milliseconds(self) -> u64 {
        self.0
    }
}

impl Default for TombstoneRetention {
    fn default() -> Self {
        Self(DEFAULT_TOMBSTONE_RETENTION_MS)
    }
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Reflect)]
#[reflect(Component)]
#[require(Save)]
#[type_path = "vmux_ecs"]
pub struct CommittedOperation(());

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Reflect, Serialize, Deserialize)]
#[component(immutable)]
#[reflect(Component)]
#[type_path = "vmux_ecs"]
pub struct CommittedRevision(pub Revision);

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Reflect, Serialize, Deserialize)]
#[component(immutable)]
#[reflect(Component)]
#[type_path = "vmux_ecs"]
pub struct OperationSequence(pub u64);

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Reflect, Serialize, Deserialize)]
#[component(immutable)]
#[reflect(Component)]
#[type_path = "vmux_ecs"]
pub struct CommittedAt(pub UnixMillis);

#[derive(Bundle)]
pub struct CommittedOperationBundle<T: Send + Sync + 'static> {
    operation: CommittedOperation,
    id: Id<Operation>,
    actor: Id<Actor>,
    target: Id<T>,
    base_revision: OperationTargetRevision,
    committed_revision: CommittedRevision,
    sequence: OperationSequence,
    committed_at: CommittedAt,
    owner: ChildOf,
}

pub struct CommittedOperationSpec<T> {
    id: Id<Operation>,
    actor: Id<Actor>,
    target: Id<T>,
    base_revision: Revision,
    committed_revision: Revision,
    sequence: u64,
    committed_at: UnixMillis,
}

impl<T> CommittedOperationSpec<T> {
    pub fn new(
        id: Id<Operation>,
        actor: Id<Actor>,
        target: Id<T>,
        base_revision: Revision,
        committed_revision: Revision,
        sequence: u64,
        committed_at: UnixMillis,
    ) -> Result<Self, String> {
        if committed_revision <= base_revision {
            return Err("committed revision must advance the base revision".to_string());
        }
        if sequence == 0 {
            return Err("operation sequence must be positive".to_string());
        }
        Ok(Self {
            id,
            actor,
            target,
            base_revision,
            committed_revision,
            sequence,
            committed_at,
        })
    }
}

impl CommittedOperation {
    pub fn bundle<T: Send + Sync + 'static>(
        root: Entity,
        spec: CommittedOperationSpec<T>,
    ) -> CommittedOperationBundle<T> {
        CommittedOperationBundle {
            operation: Self(()),
            id: spec.id,
            actor: spec.actor,
            target: spec.target,
            base_revision: OperationTargetRevision(spec.base_revision),
            committed_revision: CommittedRevision(spec.committed_revision),
            sequence: OperationSequence(spec.sequence),
            committed_at: CommittedAt(spec.committed_at),
            owner: ChildOf(root),
        }
    }
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Reflect)]
#[reflect(Component)]
#[require(Save)]
#[type_path = "vmux_ecs"]
pub struct ReplicationSnapshot(());

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Reflect, Serialize, Deserialize)]
#[component(immutable)]
#[reflect(Component)]
#[type_path = "vmux_ecs"]
pub struct SnapshotRevision(pub Revision);

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Reflect, Serialize, Deserialize)]
#[component(immutable)]
#[reflect(Component)]
#[type_path = "vmux_ecs"]
pub struct SnapshotSequence(pub u64);

#[derive(Bundle)]
pub struct ReplicationSnapshotBundle {
    snapshot: ReplicationSnapshot,
    revision: SnapshotRevision,
    sequence: SnapshotSequence,
    owner: ChildOf,
}

impl ReplicationSnapshot {
    pub fn bundle(root: Entity, revision: Revision, sequence: u64) -> ReplicationSnapshotBundle {
        ReplicationSnapshotBundle {
            snapshot: Self(()),
            revision: SnapshotRevision(revision),
            sequence: SnapshotSequence(sequence),
            owner: ChildOf(root),
        }
    }
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Reflect)]
#[reflect(Component)]
#[require(Save)]
#[type_path = "vmux_ecs"]
pub struct Tombstone(());

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Reflect, Serialize, Deserialize)]
#[component(immutable)]
#[reflect(Component)]
#[type_path = "vmux_ecs"]
pub struct DeletionRevision(pub Revision);

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Reflect, Serialize, Deserialize)]
#[component(immutable)]
#[reflect(Component)]
#[type_path = "vmux_ecs"]
pub struct DeletionSequence(pub u64);

#[derive(Bundle)]
pub struct TombstoneBundle<T: Send + Sync + 'static> {
    tombstone: Tombstone,
    target: Id<T>,
    actor: Id<Actor>,
    revision: DeletionRevision,
    sequence: DeletionSequence,
    deleted_at: DeletedAt,
    owner: ChildOf,
}

impl Tombstone {
    pub fn bundle<T: Send + Sync + 'static>(
        root: Entity,
        target: Id<T>,
        actor: Id<Actor>,
        revision: Revision,
        sequence: u64,
        deleted_at: UnixMillis,
    ) -> Result<TombstoneBundle<T>, String> {
        if sequence == 0 {
            return Err("deletion sequence must be positive".to_string());
        }
        Ok(TombstoneBundle {
            tombstone: Self(()),
            target,
            actor,
            revision: DeletionRevision(revision),
            sequence: DeletionSequence(sequence),
            deleted_at: DeletedAt(deleted_at),
            owner: ChildOf(root),
        })
    }
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Reflect)]
#[reflect(Component)]
#[require(Save)]
#[type_path = "vmux_ecs"]
pub struct ReplicaAck(());

#[derive(
    Component, Clone, Copy, Debug, Default, PartialEq, Eq, Reflect, Serialize, Deserialize,
)]
#[reflect(Component, Default)]
#[type_path = "vmux_ecs"]
pub struct AcknowledgedSequence(pub u64);

#[derive(Bundle)]
pub struct ReplicaAckBundle {
    ack: ReplicaAck,
    replica: Id<Replica>,
    sequence: AcknowledgedSequence,
    owner: ChildOf,
}

impl ReplicaAck {
    pub fn bundle(root: Entity, replica: Id<Replica>, sequence: u64) -> ReplicaAckBundle {
        ReplicaAckBundle {
            ack: Self(()),
            replica,
            sequence: AcknowledgedSequence(sequence),
            owner: ChildOf(root),
        }
    }
}

impl AcknowledgedSequence {
    pub fn acknowledge(&mut self, sequence: u64) {
        self.0 = self.0.max(sequence);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplicationConflict {
    DuplicateOperationId,
    DuplicateSequence,
    InvalidRevision,
    NonMonotonicRevision,
}

#[derive(Resource, Default)]
pub struct ReplicationIndex {
    operations: HashMap<Entity, Vec<(u64, Revision, Entity)>>,
    conflicts: HashMap<Entity, ReplicationConflict>,
}

impl ReplicationIndex {
    pub fn replay(
        &self,
        root: Entity,
        after_sequence: u64,
    ) -> Result<Vec<Entity>, ReplicationConflict> {
        if let Some(conflict) = self.conflicts.get(&root) {
            return Err(*conflict);
        }
        Ok(self
            .operations
            .get(&root)
            .into_iter()
            .flatten()
            .filter(|(sequence, _, _)| *sequence > after_sequence)
            .map(|(_, _, entity)| *entity)
            .collect())
    }

    pub fn conflict(&self, root: Entity) -> Option<ReplicationConflict> {
        self.conflicts.get(&root).copied()
    }
}

#[derive(Resource, Default)]
struct IndexedReplicationEntities(HashSet<Entity>);

type Operations<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Id<Operation>,
        &'static OperationTargetRevision,
        &'static CommittedRevision,
        &'static OperationSequence,
        &'static ChildOf,
    ),
    With<CommittedOperation>,
>;

type ChangedOperations<'w, 's> = Query<
    'w,
    's,
    (),
    (
        With<CommittedOperation>,
        Or<(
            Added<CommittedOperation>,
            Added<Id<Operation>>,
            Added<OperationTargetRevision>,
            Added<CommittedRevision>,
            Added<OperationSequence>,
            Changed<ChildOf>,
        )>,
    ),
>;

#[derive(SystemParam)]
struct ReplicationChanges<'w, 's> {
    changed: ChangedOperations<'w, 's>,
    operations: RemovedComponents<'w, 's, CommittedOperation>,
    ids: RemovedComponents<'w, 's, Id<Operation>>,
    bases: RemovedComponents<'w, 's, OperationTargetRevision>,
    revisions: RemovedComponents<'w, 's, CommittedRevision>,
    sequences: RemovedComponents<'w, 's, OperationSequence>,
    owners: RemovedComponents<'w, 's, ChildOf>,
}

impl ReplicationChanges<'_, '_> {
    fn dirty(&mut self, indexed: &IndexedReplicationEntities) -> bool {
        if !self.changed.is_empty() {
            return true;
        }
        self.operations
            .read()
            .chain(self.ids.read())
            .chain(self.bases.read())
            .chain(self.revisions.read())
            .chain(self.sequences.read())
            .chain(self.owners.read())
            .any(|entity| indexed.0.contains(&entity))
    }
}

fn rebuild(
    operations: Operations,
    mut changes: ReplicationChanges,
    mut current: ResMut<ReplicationIndex>,
    mut indexed: ResMut<IndexedReplicationEntities>,
) {
    if !changes.dirty(&indexed) {
        return;
    }

    let mut next = ReplicationIndex::default();
    let mut next_indexed = HashSet::new();
    let mut ids = HashMap::<(Entity, Id<Operation>), Entity>::new();
    let mut sequences = HashMap::<(Entity, u64), Entity>::new();
    for (entity, id, base, revision, sequence, owner) in &operations {
        let root = owner.parent();
        next_indexed.insert(entity);
        if revision.0 <= base.0 {
            next.conflicts
                .insert(root, ReplicationConflict::InvalidRevision);
        }
        if ids.insert((root, id.clone()), entity).is_some() {
            next.conflicts
                .insert(root, ReplicationConflict::DuplicateOperationId);
        }
        if sequences.insert((root, sequence.0), entity).is_some() {
            next.conflicts
                .insert(root, ReplicationConflict::DuplicateSequence);
        }
        next.operations
            .entry(root)
            .or_default()
            .push((sequence.0, revision.0, entity));
    }
    for (root, operations) in &mut next.operations {
        operations.sort_by_key(|(sequence, _, _)| *sequence);
        if operations.windows(2).any(|pair| pair[0].1 >= pair[1].1) {
            next.conflicts
                .insert(*root, ReplicationConflict::NonMonotonicRevision);
        }
    }
    *current = next;
    indexed.0 = next_indexed;
}

fn compact(
    roots: Query<(Entity, &TombstoneRetention), With<ReplicationRoot>>,
    snapshots: Query<(&SnapshotSequence, &ChildOf), With<ReplicationSnapshot>>,
    acknowledgements: Query<(&AcknowledgedSequence, &ChildOf), With<ReplicaAck>>,
    operations: Query<(Entity, &OperationSequence, &ChildOf), With<CommittedOperation>>,
    tombstones: Query<(Entity, &DeletionSequence, &DeletedAt, &ChildOf), With<Tombstone>>,
    index: Res<ReplicationIndex>,
    mut commands: Commands,
) {
    let now = UnixMillis::now().0;
    for (root, retention) in &roots {
        if index.conflict(root).is_some() {
            continue;
        }
        let snapshot = snapshots
            .iter()
            .filter(|(_, owner)| owner.parent() == root)
            .map(|(sequence, _)| sequence.0)
            .max();
        let Some(snapshot) = snapshot else {
            continue;
        };
        let mut replicas = acknowledgements
            .iter()
            .filter(|(_, owner)| owner.parent() == root)
            .map(|(sequence, _)| sequence.0);
        let Some(mut acknowledged) = replicas.next() else {
            continue;
        };
        for sequence in replicas {
            acknowledged = acknowledged.min(sequence);
        }
        let compacted = snapshot.min(acknowledged);
        for (entity, sequence, owner) in &operations {
            if owner.parent() == root && sequence.0 <= compacted {
                commands.entity(entity).despawn();
            }
        }
        let retention = i64::try_from(retention.milliseconds()).unwrap_or(i64::MAX);
        for (entity, sequence, deleted_at, owner) in &tombstones {
            if owner.parent() != root || sequence.0 > compacted {
                continue;
            }
            if deleted_at.0.0.saturating_add(retention) <= now {
                commands.entity(entity).despawn();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    enum Issue {}

    struct Fixture {
        app: App,
        root: Entity,
        actor: Id<Actor>,
    }

    impl Fixture {
        fn new(retention: u64) -> Self {
            let mut app = App::new();
            app.add_plugins(ReplicationPlugin);
            let root = app
                .world_mut()
                .spawn((ReplicationRoot, TombstoneRetention::new(retention).unwrap()))
                .id();
            Self {
                app,
                root,
                actor: Id::new("member-1"),
            }
        }

        fn operation(&mut self, id: &str, revision: u64, sequence: u64) -> Entity {
            self.app
                .world_mut()
                .spawn(CommittedOperation::bundle(
                    self.root,
                    CommittedOperationSpec::new(
                        Id::new(id),
                        self.actor.clone(),
                        Id::<Issue>::new("VMX-233"),
                        Revision::new(revision - 1),
                        Revision::new(revision),
                        sequence,
                        UnixMillis(10),
                    )
                    .unwrap(),
                ))
                .id()
        }
    }

    #[test]
    fn replay_is_sequence_ordered_and_duplicate_safe() {
        let mut fixture = Fixture::new(1);
        let second = fixture.operation("operation-2", 2, 2);
        let first = fixture.operation("operation-1", 1, 1);

        fixture.app.update();

        let replay = fixture
            .app
            .world()
            .resource::<ReplicationIndex>()
            .replay(fixture.root, 0)
            .unwrap();
        assert_eq!(replay, vec![first, second]);
        assert_eq!(
            fixture
                .app
                .world()
                .resource::<ReplicationIndex>()
                .replay(fixture.root, 1)
                .unwrap(),
            vec![second]
        );
    }

    #[test]
    fn duplicate_operation_identity_blocks_replay_and_compaction() {
        let mut fixture = Fixture::new(1);
        let first = fixture.operation("operation-1", 1, 1);
        fixture.operation("operation-1", 2, 2);
        fixture.app.world_mut().spawn(ReplicationSnapshot::bundle(
            fixture.root,
            Revision::new(2),
            2,
        ));
        fixture
            .app
            .world_mut()
            .spawn(ReplicaAck::bundle(fixture.root, Id::new("replica-1"), 2));

        fixture.app.update();

        assert_eq!(
            fixture
                .app
                .world()
                .resource::<ReplicationIndex>()
                .replay(fixture.root, 0),
            Err(ReplicationConflict::DuplicateOperationId)
        );
        assert!(fixture.app.world().get_entity(first).is_ok());
    }

    #[test]
    fn compaction_waits_for_snapshot_ack_and_tombstone_retention() {
        let mut fixture = Fixture::new(10);
        let operation = fixture.operation("operation-1", 1, 1);
        let tombstone = fixture
            .app
            .world_mut()
            .spawn(
                Tombstone::bundle(
                    fixture.root,
                    Id::<Issue>::new("VMX-233"),
                    fixture.actor.clone(),
                    Revision::new(2),
                    2,
                    UnixMillis(UnixMillis::now().0 - 11),
                )
                .unwrap(),
            )
            .id();
        fixture.app.world_mut().spawn(ReplicationSnapshot::bundle(
            fixture.root,
            Revision::new(2),
            2,
        ));
        let ack = fixture
            .app
            .world_mut()
            .spawn(ReplicaAck::bundle(fixture.root, Id::new("replica-1"), 1))
            .id();

        fixture.app.update();

        assert!(fixture.app.world().get_entity(operation).is_err());
        assert!(fixture.app.world().get_entity(tombstone).is_ok());

        fixture
            .app
            .world_mut()
            .get_mut::<AcknowledgedSequence>(ack)
            .unwrap()
            .acknowledge(2);
        fixture.app.update();

        assert!(fixture.app.world().get_entity(tombstone).is_err());
    }
}
