use std::any::TypeId;
use std::collections::HashMap;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;

use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;

pub struct SelectionPlugin<Scope: Send + Sync + 'static = ()>(PhantomData<fn() -> Scope>);

impl<Scope: Send + Sync + 'static> Default for SelectionPlugin<Scope> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<Scope: Send + Sync + 'static> Plugin for SelectionPlugin<Scope> {
    fn build(&self, app: &mut App) {
        let set = SelectionSet::of::<Scope>();
        app.add_message::<SelectRequest<Scope>>()
            .add_observer(enforce::<Scope>)
            .add_observer(reparent::<Scope>)
            .add_observer(detach::<Scope>)
            .add_observer(clear_parent::<Scope>)
            .add_systems(Update, (apply::<Scope>, ApplyDeferred).chain().in_set(set));
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SelectionSet(TypeId);

impl SelectionSet {
    pub fn of<Scope: Send + Sync + 'static>() -> Self {
        Self(TypeId::of::<Scope>())
    }
}

#[derive(Component)]
pub struct Selected<Scope: Send + Sync + 'static = ()>(PhantomData<fn() -> Scope>);

impl<Scope: Send + Sync + 'static> Selected<Scope> {
    pub const fn new() -> Self {
        Self(PhantomData)
    }
}

impl<Scope: Send + Sync + 'static> Default for Selected<Scope> {
    fn default() -> Self {
        Self::new()
    }
}

impl<Scope: Send + Sync + 'static> Clone for Selected<Scope> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<Scope: Send + Sync + 'static> Copy for Selected<Scope> {}

impl<Scope: Send + Sync + 'static> PartialEq for Selected<Scope> {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl<Scope: Send + Sync + 'static> Eq for Selected<Scope> {}

impl<Scope: Send + Sync + 'static> Hash for Selected<Scope> {
    fn hash<H: Hasher>(&self, _: &mut H) {}
}

impl<Scope: Send + Sync + 'static> fmt::Debug for Selected<Scope> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Selected")
    }
}

#[derive(Message)]
pub struct SelectRequest<Scope: Send + Sync + 'static = ()> {
    target: Option<Entity>,
    scope: Option<Entity>,
    marker: PhantomData<fn() -> Scope>,
}

impl<Scope: Send + Sync + 'static> SelectRequest<Scope> {
    pub const fn select(entity: Entity) -> Self {
        Self {
            target: Some(entity),
            scope: None,
            marker: PhantomData,
        }
    }

    pub const fn clear(scope: Option<Entity>) -> Self {
        Self {
            target: None,
            scope,
            marker: PhantomData,
        }
    }

    pub const fn target(&self) -> Option<Entity> {
        self.target
    }

    pub const fn scope(&self) -> Option<Entity> {
        self.scope
    }
}

impl<Scope: Send + Sync + 'static> Clone for SelectRequest<Scope> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<Scope: Send + Sync + 'static> Copy for SelectRequest<Scope> {}

impl<Scope: Send + Sync + 'static> PartialEq for SelectRequest<Scope> {
    fn eq(&self, other: &Self) -> bool {
        self.target == other.target && self.scope == other.scope
    }
}

impl<Scope: Send + Sync + 'static> Eq for SelectRequest<Scope> {}

impl<Scope: Send + Sync + 'static> fmt::Debug for SelectRequest<Scope> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SelectRequest")
            .field("target", &self.target)
            .field("scope", &self.scope)
            .finish()
    }
}

#[derive(Component)]
struct SelectionParent<Scope: Send + Sync + 'static> {
    parent: Option<Entity>,
    marker: PhantomData<fn() -> Scope>,
}

impl<Scope: Send + Sync + 'static> SelectionParent<Scope> {
    const fn of(parent: Option<Entity>) -> Self {
        Self {
            parent,
            marker: PhantomData,
        }
    }
}

fn apply<Scope: Send + Sync + 'static>(
    mut requests: MessageReader<SelectRequest<Scope>>,
    existing: Query<()>,
    parents: Query<&ChildOf>,
    selected: Query<(Entity, Option<&ChildOf>), With<Selected<Scope>>>,
    mut commands: Commands,
) {
    let mut desired = HashMap::<Option<Entity>, Option<Entity>>::new();
    for request in requests.read() {
        match request.target {
            Some(target) => {
                if !existing.contains(target) {
                    continue;
                }
                let scope = parents.get(target).ok().map(ChildOf::parent);
                desired.insert(scope, Some(target));
            }
            None => {
                desired.insert(request.scope, None);
            }
        }
    }
    if desired.is_empty() {
        return;
    }
    for (entity, parent) in &selected {
        let scope = parent.map(ChildOf::parent);
        if let Some(winner) = desired.get(&scope)
            && *winner != Some(entity)
        {
            commands
                .entity(entity)
                .try_remove::<(Selected<Scope>, SelectionParent<Scope>)>();
        }
    }
    for (scope, winner) in desired {
        if let Some(entity) = winner {
            commands.entity(entity).try_insert((
                Selected::<Scope>::new(),
                SelectionParent::<Scope>::of(scope),
            ));
        }
    }
}

fn enforce<Scope: Send + Sync + 'static>(
    trigger: On<Insert, Selected<Scope>>,
    parents: Query<&ChildOf>,
    selected: Query<(Entity, Option<&ChildOf>), With<Selected<Scope>>>,
    mut commands: Commands,
) {
    let target = trigger.event_target();
    let scope = parents.get(target).ok().map(ChildOf::parent);
    commands
        .entity(target)
        .try_insert(SelectionParent::<Scope>::of(scope));
    for (entity, parent) in &selected {
        if entity != target && parent.map(ChildOf::parent) == scope {
            commands
                .entity(entity)
                .try_remove::<(Selected<Scope>, SelectionParent<Scope>)>();
        }
    }
}

fn reparent<Scope: Send + Sync + 'static>(
    trigger: On<Insert, ChildOf>,
    parents: Query<(&ChildOf, &SelectionParent<Scope>), With<Selected<Scope>>>,
    mut commands: Commands,
) {
    let entity = trigger.event_target();
    let Ok((current, recorded)) = parents.get(entity) else {
        return;
    };
    if recorded.parent != Some(current.parent()) {
        commands
            .entity(entity)
            .try_remove::<(Selected<Scope>, SelectionParent<Scope>)>();
    }
}

fn detach<Scope: Send + Sync + 'static>(
    trigger: On<Remove, ChildOf>,
    selected: Query<&SelectionParent<Scope>, With<Selected<Scope>>>,
    mut commands: Commands,
) {
    let entity = trigger.event_target();
    let Ok(recorded) = selected.get(entity) else {
        return;
    };
    if recorded.parent.is_some() {
        commands
            .entity(entity)
            .try_remove::<(Selected<Scope>, SelectionParent<Scope>)>();
    }
}

fn clear_parent<Scope: Send + Sync + 'static>(
    trigger: On<Remove, Selected<Scope>>,
    mut commands: Commands,
) {
    commands
        .entity(trigger.event_target())
        .try_remove::<SelectionParent<Scope>>();
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TabScope;
    struct PaneScope;

    #[test]
    fn selection_enforces_at_most_one_selected_entity_per_parent_and_root_scope() {
        let mut app = App::new();
        app.add_plugins((
            SelectionPlugin::<TabScope>::default(),
            SelectionPlugin::<PaneScope>::default(),
        ));

        let parent_a = app.world_mut().spawn_empty().id();
        let parent_b = app.world_mut().spawn_empty().id();
        let a1 = app.world_mut().spawn(ChildOf(parent_a)).id();
        let a2 = app.world_mut().spawn(ChildOf(parent_a)).id();
        let b1 = app.world_mut().spawn(ChildOf(parent_b)).id();
        let root_a = app.world_mut().spawn_empty().id();
        let root_b = app.world_mut().spawn_empty().id();

        app.world_mut()
            .write_message(SelectRequest::<TabScope>::select(a1));
        app.world_mut()
            .write_message(SelectRequest::<TabScope>::select(b1));
        app.world_mut()
            .write_message(SelectRequest::<TabScope>::select(root_a));
        app.world_mut()
            .write_message(SelectRequest::<PaneScope>::select(a1));
        app.update();

        assert!(app.world().get::<Selected<TabScope>>(a1).is_some());
        assert!(app.world().get::<Selected<TabScope>>(a2).is_none());
        assert!(app.world().get::<Selected<TabScope>>(b1).is_some());
        assert!(app.world().get::<Selected<TabScope>>(root_a).is_some());
        assert!(app.world().get::<Selected<PaneScope>>(a1).is_some());

        app.world_mut()
            .write_message(SelectRequest::<TabScope>::select(a2));
        app.world_mut()
            .write_message(SelectRequest::<TabScope>::select(root_b));
        app.update();

        assert!(app.world().get::<Selected<TabScope>>(a1).is_none());
        assert!(app.world().get::<Selected<TabScope>>(a2).is_some());
        assert!(app.world().get::<Selected<TabScope>>(b1).is_some());
        assert!(app.world().get::<Selected<TabScope>>(root_a).is_none());
        assert!(app.world().get::<Selected<TabScope>>(root_b).is_some());
        assert!(app.world().get::<Selected<PaneScope>>(a1).is_some());

        app.world_mut()
            .write_message(SelectRequest::<TabScope>::clear(Some(parent_a)));
        app.world_mut()
            .write_message(SelectRequest::<TabScope>::clear(None));
        app.update();

        assert!(app.world().get::<Selected<TabScope>>(a2).is_none());
        assert!(app.world().get::<Selected<TabScope>>(root_b).is_none());
        assert!(app.world().get::<Selected<TabScope>>(b1).is_some());
    }

    #[test]
    fn reparenting_detaching_and_despawning_clean_up_selection_automatically() {
        let mut app = App::new();
        app.add_plugins(SelectionPlugin::<TabScope>::default());

        let parent_a = app.world_mut().spawn_empty().id();
        let parent_b = app.world_mut().spawn_empty().id();
        let a1 = app
            .world_mut()
            .spawn((ChildOf(parent_a), Selected::<TabScope>::new()))
            .id();
        let b1 = app
            .world_mut()
            .spawn((ChildOf(parent_b), Selected::<TabScope>::new()))
            .id();

        assert!(app.world().get::<Selected<TabScope>>(a1).is_some());
        assert!(app.world().get::<Selected<TabScope>>(b1).is_some());

        app.world_mut().entity_mut(a1).insert(ChildOf(parent_b));
        assert!(app.world().get::<Selected<TabScope>>(a1).is_none());
        assert!(app.world().get::<Selected<TabScope>>(b1).is_some());

        app.world_mut().entity_mut(b1).remove::<ChildOf>();
        assert!(app.world().get::<Selected<TabScope>>(b1).is_none());

        let root = app.world_mut().spawn(Selected::<TabScope>::new()).id();
        assert!(app.world().get::<Selected<TabScope>>(root).is_some());
        app.world_mut().entity_mut(root).insert(ChildOf(parent_a));
        assert!(app.world().get::<Selected<TabScope>>(root).is_none());

        app.world_mut()
            .entity_mut(a1)
            .insert(Selected::<TabScope>::new());
        assert!(app.world().get::<Selected<TabScope>>(a1).is_some());
        app.world_mut().despawn(a1);
        app.world_mut()
            .write_message(SelectRequest::<TabScope>::select(a1));
        app.update();
        assert!(app.world().get_entity(a1).is_err());
    }
}
