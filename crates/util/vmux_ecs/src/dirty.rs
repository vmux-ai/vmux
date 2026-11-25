use std::any::TypeId;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;

use bevy_app::{App, Plugin, PostUpdate};
use bevy_ecs::prelude::*;

pub struct DirtyPlugin<Tag: Send + Sync + 'static = ()>(PhantomData<fn() -> Tag>);

impl<Tag: Send + Sync + 'static> Default for DirtyPlugin<Tag> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<Tag: Send + Sync + 'static> Plugin for DirtyPlugin<Tag> {
    fn build(&self, app: &mut App) {
        let set = DirtySet::of::<Tag>();
        app.add_systems(
            PostUpdate,
            (clear::<Tag>, ApplyDeferred).chain().in_set(set),
        );
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DirtySet(TypeId);

impl DirtySet {
    pub fn of<Tag: Send + Sync + 'static>() -> Self {
        Self(TypeId::of::<Tag>())
    }
}

#[derive(Component)]
pub struct Dirty<Tag: Send + Sync + 'static = ()>(PhantomData<fn() -> Tag>);

impl<Tag: Send + Sync + 'static> Dirty<Tag> {
    pub const fn new() -> Self {
        Self(PhantomData)
    }
}

impl<Tag: Send + Sync + 'static> Default for Dirty<Tag> {
    fn default() -> Self {
        Self::new()
    }
}

impl<Tag: Send + Sync + 'static> Clone for Dirty<Tag> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<Tag: Send + Sync + 'static> Copy for Dirty<Tag> {}

impl<Tag: Send + Sync + 'static> PartialEq for Dirty<Tag> {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl<Tag: Send + Sync + 'static> Eq for Dirty<Tag> {}

impl<Tag: Send + Sync + 'static> Hash for Dirty<Tag> {
    fn hash<H: Hasher>(&self, _: &mut H) {}
}

impl<Tag: Send + Sync + 'static> fmt::Debug for Dirty<Tag> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Dirty")
    }
}

fn clear<Tag: Send + Sync + 'static>(
    marked: Query<Entity, With<Dirty<Tag>>>,
    mut commands: Commands,
) {
    for entity in &marked {
        commands.entity(entity).try_remove::<Dirty<Tag>>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct SummaryView;
    struct DetailView;

    #[derive(Component)]
    struct Counter(u32);

    #[derive(Component, Default)]
    struct SummaryProjection(u32);

    #[derive(Resource, Default)]
    struct RebuildCount(u32);

    fn project_summary(
        mut entities: Query<(&Counter, &mut SummaryProjection), With<Dirty<SummaryView>>>,
        mut rebuilds: ResMut<RebuildCount>,
    ) {
        for (counter, mut projection) in &mut entities {
            projection.0 = counter.0;
            rebuilds.0 += 1;
        }
    }

    #[test]
    fn dirty_markers_drive_one_projection_pass_and_clear_independently_by_tag() {
        let mut app = App::new();
        app.add_plugins((
            DirtyPlugin::<SummaryView>::default(),
            DirtyPlugin::<DetailView>::default(),
        ))
        .init_resource::<RebuildCount>()
        .add_systems(
            PostUpdate,
            project_summary.before(DirtySet::of::<SummaryView>()),
        );

        let entity = app
            .world_mut()
            .spawn((
                Counter(7),
                SummaryProjection::default(),
                Dirty::<SummaryView>::new(),
                Dirty::<DetailView>::new(),
            ))
            .id();

        app.update();
        assert_eq!(app.world().get::<SummaryProjection>(entity).unwrap().0, 7);
        assert_eq!(app.world().resource::<RebuildCount>().0, 1);
        assert!(app.world().get::<Dirty<SummaryView>>(entity).is_none());
        assert!(app.world().get::<Dirty<DetailView>>(entity).is_none());

        app.update();
        assert_eq!(app.world().resource::<RebuildCount>().0, 1);

        app.world_mut().get_mut::<Counter>(entity).unwrap().0 = 12;
        app.world_mut()
            .entity_mut(entity)
            .insert(Dirty::<SummaryView>::new());
        app.update();
        assert_eq!(app.world().get::<SummaryProjection>(entity).unwrap().0, 12);
        assert_eq!(app.world().resource::<RebuildCount>().0, 2);
        assert!(app.world().get::<Dirty<SummaryView>>(entity).is_none());
    }
}
