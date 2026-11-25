use std::any::TypeId;
use std::cmp::Ordering;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;

use bevy_app::{App, Plugin, PostUpdate, Update};
use bevy_ecs::prelude::*;

use crate::{Operation, OperationState, OperationTarget};

pub struct GenerationPlugin<Tag: Send + Sync + 'static = ()>(PhantomData<fn() -> Tag>);

impl<Tag: Send + Sync + 'static> Default for GenerationPlugin<Tag> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<Tag: Send + Sync + 'static> Plugin for GenerationPlugin<Tag> {
    fn build(&self, app: &mut App) {
        let set = GenerationSet::of::<Tag>();
        app.add_systems(Update, (guard::<Tag>, ApplyDeferred).chain().in_set(set))
            .add_systems(
                PostUpdate,
                (guard::<Tag>, ApplyDeferred).chain().in_set(set),
            );
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GenerationSet(TypeId);

impl GenerationSet {
    pub fn of<Tag: Send + Sync + 'static>() -> Self {
        Self(TypeId::of::<Tag>())
    }
}

#[derive(Component)]
pub struct SequenceCounter<Tag: Send + Sync + 'static = ()> {
    next: u64,
    marker: PhantomData<fn() -> Tag>,
}

impl<Tag: Send + Sync + 'static> Default for SequenceCounter<Tag> {
    fn default() -> Self {
        Self::starting_at(0)
    }
}

impl<Tag: Send + Sync + 'static> SequenceCounter<Tag> {
    pub const fn starting_at(start: u64) -> Self {
        Self {
            next: start,
            marker: PhantomData,
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> u64 {
        let value = self.next;
        self.next = self.next.saturating_add(1);
        value
    }

    pub const fn peek(&self) -> u64 {
        self.next
    }
}

impl<Tag: Send + Sync + 'static> fmt::Debug for SequenceCounter<Tag> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("SequenceCounter")
            .field(&self.next)
            .finish()
    }
}

#[derive(Resource)]
pub struct GlobalSequence<Tag: Send + Sync + 'static = ()> {
    next: u64,
    marker: PhantomData<fn() -> Tag>,
}

impl<Tag: Send + Sync + 'static> Default for GlobalSequence<Tag> {
    fn default() -> Self {
        Self::starting_at(0)
    }
}

impl<Tag: Send + Sync + 'static> GlobalSequence<Tag> {
    pub const fn starting_at(start: u64) -> Self {
        Self {
            next: start,
            marker: PhantomData,
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> u64 {
        let value = self.next;
        self.next = self.next.saturating_add(1);
        value
    }

    pub const fn peek(&self) -> u64 {
        self.next
    }
}

impl<Tag: Send + Sync + 'static> fmt::Debug for GlobalSequence<Tag> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("GlobalSequence")
            .field(&self.next)
            .finish()
    }
}

#[derive(Component)]
pub struct Generation<Tag: Send + Sync + 'static = ()> {
    value: u64,
    marker: PhantomData<fn() -> Tag>,
}

impl<Tag: Send + Sync + 'static> Generation<Tag> {
    pub const fn new(value: u64) -> Self {
        Self {
            value,
            marker: PhantomData,
        }
    }

    pub const fn value(self) -> u64 {
        self.value
    }

    pub fn advance(&mut self) -> u64 {
        self.value = self.value.saturating_add(1);
        self.value
    }

    pub const fn snapshot(self) -> AtGeneration<Tag> {
        AtGeneration::new(self.value)
    }
}

impl<Tag: Send + Sync + 'static> Default for Generation<Tag> {
    fn default() -> Self {
        Self::new(0)
    }
}

impl<Tag: Send + Sync + 'static> Clone for Generation<Tag> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<Tag: Send + Sync + 'static> Copy for Generation<Tag> {}

impl<Tag: Send + Sync + 'static> PartialEq for Generation<Tag> {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

impl<Tag: Send + Sync + 'static> Eq for Generation<Tag> {}

impl<Tag: Send + Sync + 'static> PartialOrd for Generation<Tag> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<Tag: Send + Sync + 'static> Ord for Generation<Tag> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.value.cmp(&other.value)
    }
}

impl<Tag: Send + Sync + 'static> Hash for Generation<Tag> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.value.hash(state);
    }
}

impl<Tag: Send + Sync + 'static> fmt::Debug for Generation<Tag> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("Generation")
            .field(&self.value)
            .finish()
    }
}

#[derive(Component)]
#[component(immutable)]
pub struct AtGeneration<Tag: Send + Sync + 'static = ()> {
    value: u64,
    marker: PhantomData<fn() -> Tag>,
}

impl<Tag: Send + Sync + 'static> AtGeneration<Tag> {
    pub const fn new(value: u64) -> Self {
        Self {
            value,
            marker: PhantomData,
        }
    }

    pub const fn value(self) -> u64 {
        self.value
    }

    pub const fn matches(self, current: Generation<Tag>) -> bool {
        self.value == current.value
    }

    pub const fn is_stale(self, current: Generation<Tag>) -> bool {
        self.value != current.value
    }
}

impl<Tag: Send + Sync + 'static> Clone for AtGeneration<Tag> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<Tag: Send + Sync + 'static> Copy for AtGeneration<Tag> {}

impl<Tag: Send + Sync + 'static> PartialEq for AtGeneration<Tag> {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

impl<Tag: Send + Sync + 'static> Eq for AtGeneration<Tag> {}

impl<Tag: Send + Sync + 'static> PartialOrd for AtGeneration<Tag> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<Tag: Send + Sync + 'static> Ord for AtGeneration<Tag> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.value.cmp(&other.value)
    }
}

impl<Tag: Send + Sync + 'static> Hash for AtGeneration<Tag> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.value.hash(state);
    }
}

impl<Tag: Send + Sync + 'static> fmt::Debug for AtGeneration<Tag> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("AtGeneration")
            .field(&self.value)
            .finish()
    }
}

#[derive(Component)]
pub struct Stale<Tag: Send + Sync + 'static = ()>(PhantomData<fn() -> Tag>);

impl<Tag: Send + Sync + 'static> Stale<Tag> {
    pub const fn new() -> Self {
        Self(PhantomData)
    }
}

impl<Tag: Send + Sync + 'static> Default for Stale<Tag> {
    fn default() -> Self {
        Self::new()
    }
}

impl<Tag: Send + Sync + 'static> Clone for Stale<Tag> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<Tag: Send + Sync + 'static> Copy for Stale<Tag> {}

impl<Tag: Send + Sync + 'static> PartialEq for Stale<Tag> {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl<Tag: Send + Sync + 'static> Eq for Stale<Tag> {}

impl<Tag: Send + Sync + 'static> Hash for Stale<Tag> {
    fn hash<H: Hasher>(&self, _: &mut H) {}
}

impl<Tag: Send + Sync + 'static> fmt::Debug for Stale<Tag> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Stale")
    }
}

type ChangedWork<'w, 's, Tag> = Query<
    'w,
    's,
    (),
    (
        With<AtGeneration<Tag>>,
        Without<Stale<Tag>>,
        Or<(
            Added<AtGeneration<Tag>>,
            Changed<ChildOf>,
            Changed<OperationTarget>,
        )>,
    ),
>;

type GuardedWork<'w, 's, Tag> = Query<
    'w,
    's,
    (
        Entity,
        &'static AtGeneration<Tag>,
        Option<&'static ChildOf>,
        Option<&'static OperationTarget>,
        Option<&'static mut OperationState>,
        Has<Operation>,
    ),
    Without<Stale<Tag>>,
>;

fn guard<Tag: Send + Sync + 'static>(
    owners: Query<&Generation<Tag>>,
    changed_owners: Query<(), Changed<Generation<Tag>>>,
    changed_work: ChangedWork<Tag>,
    mut work: GuardedWork<Tag>,
    mut commands: Commands,
) {
    if changed_owners.is_empty() && changed_work.is_empty() {
        return;
    }
    for (entity, at, parent, target, state, is_operation) in &mut work {
        let mut stale = false;
        if let Some(target) = target
            && let Ok(current) = owners.get(target.entity())
            && at.is_stale(*current)
        {
            stale = true;
        }
        if !stale
            && let Some(parent) = parent
            && let Ok(current) = owners.get(parent.parent())
            && at.is_stale(*current)
        {
            stale = true;
        }
        if !stale {
            continue;
        }
        commands.entity(entity).try_insert(Stale::<Tag>::new());
        if is_operation
            && let Some(mut state) = state
            && matches!(*state, OperationState::Pending | OperationState::Running)
        {
            *state = OperationState::Canceled;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct DocumentTag;
    struct OtherTag;

    #[test]
    fn sequence_counters_advance_monotonically_without_wrapping_backward() {
        let mut counter = SequenceCounter::<DocumentTag>::default();
        assert_eq!(counter.peek(), 0);
        assert_eq!(counter.next(), 0);
        assert_eq!(counter.next(), 1);
        assert_eq!(counter.peek(), 2);

        let mut custom = SequenceCounter::<DocumentTag>::starting_at(10);
        assert_eq!(custom.next(), 10);
        assert_eq!(custom.next(), 11);

        let mut global = GlobalSequence::<DocumentTag>::starting_at(u64::MAX);
        assert_eq!(global.next(), u64::MAX);
        assert_eq!(global.next(), u64::MAX);
    }

    #[test]
    fn advancing_generation_marks_stale_and_cancels_in_flight_child_and_targeted_operations() {
        let mut app = App::new();
        app.add_plugins((
            GenerationPlugin::<DocumentTag>::default(),
            GenerationPlugin::<OtherTag>::default(),
        ));

        let owner = app
            .world_mut()
            .spawn((
                Generation::<DocumentTag>::new(1),
                Generation::<OtherTag>::new(1),
            ))
            .id();
        let child_op = app
            .world_mut()
            .spawn((
                Operation::bundle("doc.child").unwrap().running(),
                ChildOf(owner),
                AtGeneration::<DocumentTag>::new(1),
            ))
            .id();
        let targeted_op = app
            .world_mut()
            .spawn((
                Operation::bundle("doc.targeted").unwrap(),
                OperationTarget::new(owner),
                AtGeneration::<DocumentTag>::new(1),
            ))
            .id();
        let finished_op = app
            .world_mut()
            .spawn((
                Operation,
                OperationState::Succeeded,
                OperationTarget::new(owner),
                AtGeneration::<DocumentTag>::new(1),
            ))
            .id();
        let plain_work = app
            .world_mut()
            .spawn((
                OperationTarget::new(owner),
                AtGeneration::<DocumentTag>::new(1),
            ))
            .id();
        let other_tag_op = app
            .world_mut()
            .spawn((
                Operation::bundle("other.op").unwrap().running(),
                ChildOf(owner),
                AtGeneration::<OtherTag>::new(1),
            ))
            .id();

        app.update();
        assert!(app.world().get::<Stale<DocumentTag>>(child_op).is_none());
        assert_eq!(
            app.world().get::<OperationState>(child_op),
            Some(&OperationState::Running)
        );
        assert_eq!(
            app.world().get::<OperationState>(targeted_op),
            Some(&OperationState::Pending)
        );

        let next = app
            .world_mut()
            .get_mut::<Generation<DocumentTag>>(owner)
            .unwrap()
            .advance();
        assert_eq!(next, 2);

        app.update();

        assert!(app.world().get::<Stale<DocumentTag>>(child_op).is_some());
        assert_eq!(
            app.world().get::<OperationState>(child_op),
            Some(&OperationState::Canceled)
        );
        assert!(app.world().get::<Stale<DocumentTag>>(targeted_op).is_some());
        assert_eq!(
            app.world().get::<OperationState>(targeted_op),
            Some(&OperationState::Canceled)
        );
        assert!(app.world().get::<Stale<DocumentTag>>(finished_op).is_some());
        assert_eq!(
            app.world().get::<OperationState>(finished_op),
            Some(&OperationState::Succeeded)
        );
        assert!(app.world().get::<Stale<DocumentTag>>(plain_work).is_some());
        assert!(app.world().get::<Stale<OtherTag>>(other_tag_op).is_none());
        assert_eq!(
            app.world().get::<OperationState>(other_tag_op),
            Some(&OperationState::Running)
        );
    }

    #[test]
    fn late_work_spawned_at_an_older_generation_is_immediately_marked_stale_and_canceled() {
        let mut app = App::new();
        app.add_plugins(GenerationPlugin::<DocumentTag>::default());

        let owner = app
            .world_mut()
            .spawn(Generation::<DocumentTag>::new(5))
            .id();
        app.update();

        let late = app
            .world_mut()
            .spawn((
                Operation::bundle("doc.late").unwrap().running(),
                OperationTarget::new(owner),
                AtGeneration::<DocumentTag>::new(4),
            ))
            .id();

        app.update();

        assert!(app.world().get::<Stale<DocumentTag>>(late).is_some());
        assert_eq!(
            app.world().get::<OperationState>(late),
            Some(&OperationState::Canceled)
        );
    }
}
