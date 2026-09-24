use std::marker::PhantomData;

use bevy::prelude::*;
use bevy_cef::prelude::{BinHostEmitEvent, Browsers};
use rkyv::api::high::HighSerializer;
use rkyv::ser::allocator::ArenaHandle;
use rkyv::util::AlignedVec;
use vmux_api::{BatchedUiState, HostEvent};

pub struct UiStatePlugin<S>(PhantomData<fn() -> S>);

impl<S> Default for UiStatePlugin<S> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<S> Plugin for UiStatePlugin<S>
where
    S: BatchedUiState
        + HostEvent
        + for<'a> rkyv::Serialize<HighSerializer<AlignedVec, ArenaHandle<'a>, rkyv::rancor::Error>>,
{
    fn build(&self, app: &mut App) {
        app.add_observer(UiStateUpdates::<S>::collect)
            .add_systems(Last, UiStateUpdates::<S>::emit);
    }
}

#[derive(Component)]
pub struct UiStateUpdates<S: BatchedUiState> {
    sequence: u64,
    patches: Vec<S::Patch>,
    state: PhantomData<fn() -> S>,
}

impl<S: BatchedUiState> Default for UiStateUpdates<S> {
    fn default() -> Self {
        Self {
            sequence: 0,
            patches: Vec::new(),
            state: PhantomData,
        }
    }
}

impl<S: BatchedUiState> UiStateUpdates<S> {
    pub fn write<T>(commands: &mut Commands, webview: Entity, event: &T)
    where
        T: Clone + Into<S::Patch>,
    {
        commands.trigger(UiStateWrite::<S>::from_event(webview, event));
    }

    pub fn deliver<T>(
        pages: &Query<(), With<Self>>,
        commands: &mut Commands,
        webview: Entity,
        event: &T,
    ) where
        T: HostEvent
            + Clone
            + Into<S::Patch>
            + for<'a> rkyv::Serialize<
                HighSerializer<AlignedVec, ArenaHandle<'a>, rkyv::rancor::Error>,
            >,
    {
        if pages.contains(webview) {
            Self::write(commands, webview, event);
        } else {
            commands.trigger(BinHostEmitEvent::from_event(webview, event));
        }
    }

    fn push(&mut self, patch: S::Patch) {
        self.patches.push(patch);
    }

    fn take(&mut self) -> Option<S> {
        if self.patches.is_empty() {
            return None;
        }
        self.sequence = self.sequence.wrapping_add(1).max(1);
        Some(S::from_parts(
            self.sequence,
            std::mem::take(&mut self.patches),
        ))
    }

    fn collect(trigger: On<UiStateWrite<S>>, mut updates: Query<&mut Self>) {
        let Ok(mut updates) = updates.get_mut(trigger.event().webview) else {
            return;
        };
        updates.push(trigger.event().patch.clone());
    }

    fn emit(
        mut updates: Query<(Entity, &mut Self)>,
        browsers: Option<NonSend<Browsers>>,
        mut commands: Commands,
    ) where
        S: HostEvent
            + for<'a> rkyv::Serialize<
                HighSerializer<AlignedVec, ArenaHandle<'a>, rkyv::rancor::Error>,
            >,
    {
        let Some(browsers) = browsers else {
            return;
        };
        for (entity, mut updates) in &mut updates {
            if !browsers.can_emit_to(&entity) {
                continue;
            }
            let Some(event) = updates.take() else {
                continue;
            };
            commands.trigger(BinHostEmitEvent::from_event(entity, &event));
        }
    }
}

#[derive(Clone, EntityEvent)]
pub struct UiStateWrite<S: BatchedUiState> {
    #[event_target]
    webview: Entity,
    patch: S::Patch,
}

impl<S: BatchedUiState> UiStateWrite<S> {
    pub fn from_event<T>(webview: Entity, event: &T) -> Self
    where
        T: Clone + Into<S::Patch>,
    {
        Self {
            webview,
            patch: event.clone().into(),
        }
    }

    pub fn webview(&self) -> Entity {
        self.webview
    }

    pub fn patch(&self) -> &S::Patch {
        &self.patch
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{FileDirtyEvent, FileUiState, FileUiStatePatch};
    use vmux_api::BinEvent;
    use vmux_api::git::GitChangedEvent;

    #[derive(Resource, Default)]
    struct Emitted(Vec<FileUiState>);

    #[derive(Resource)]
    struct Targets {
        file: Entity,
        direct: Entity,
    }

    #[derive(Resource, Default)]
    struct Delivered {
        writes: Vec<Entity>,
        direct: Vec<Entity>,
    }

    impl Emitted {
        fn record(trigger: On<BinHostEmitEvent>, mut emitted: ResMut<Self>) {
            if trigger.event().id() != FileUiState::id() {
                return;
            }
            let event =
                rkyv::from_bytes::<FileUiState, rkyv::rancor::Error>(trigger.event().payload())
                    .unwrap();
            emitted.0.push(event);
        }
    }

    impl Delivered {
        fn write(trigger: On<UiStateWrite<FileUiState>>, mut delivered: ResMut<Self>) {
            delivered.writes.push(trigger.event().webview());
        }

        fn direct(trigger: On<BinHostEmitEvent>, mut delivered: ResMut<Self>) {
            delivered.direct.push(trigger.event().webview());
        }
    }

    fn deliver(
        targets: Res<Targets>,
        pages: Query<(), With<UiStateUpdates<FileUiState>>>,
        mut commands: Commands,
    ) {
        UiStateUpdates::<FileUiState>::deliver(
            &pages,
            &mut commands,
            targets.file,
            &GitChangedEvent {},
        );
        UiStateUpdates::<FileUiState>::deliver(
            &pages,
            &mut commands,
            targets.direct,
            &GitChangedEvent {},
        );
    }

    #[test]
    fn same_frame_updates_emit_one_batch() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, UiStatePlugin::<FileUiState>::default()))
            .init_resource::<Emitted>()
            .add_observer(Emitted::record);
        let entity = app
            .world_mut()
            .spawn(UiStateUpdates::<FileUiState>::default())
            .id();
        let mut browsers = Browsers::default();
        browsers.set_externally_hosted(entity);
        app.world_mut().insert_non_send(browsers);

        app.world_mut()
            .trigger(UiStateWrite::<FileUiState>::from_event(
                entity,
                &FileDirtyEvent { dirty: true },
            ));
        app.world_mut()
            .trigger(UiStateWrite::<FileUiState>::from_event(
                entity,
                &FileDirtyEvent { dirty: false },
            ));
        app.update();

        let emitted = &app.world().resource::<Emitted>().0;
        assert_eq!(emitted.len(), 1);
        assert_eq!(emitted[0].sequence, 1);
        assert!(matches!(
            emitted[0].patches.as_slice(),
            [
                FileUiStatePatch::Dirty(FileDirtyEvent { dirty: true }),
                FileUiStatePatch::Dirty(FileDirtyEvent { dirty: false })
            ]
        ));
    }

    #[test]
    fn page_updates_are_batched_while_other_targets_receive_direct_events() {
        let mut app = App::new();
        let file = app
            .world_mut()
            .spawn(UiStateUpdates::<FileUiState>::default())
            .id();
        let direct = app.world_mut().spawn_empty().id();
        app.insert_resource(Targets { file, direct })
            .init_resource::<Delivered>()
            .add_observer(Delivered::write)
            .add_observer(Delivered::direct)
            .add_systems(Update, deliver);

        app.update();

        let delivered = app.world().resource::<Delivered>();
        assert_eq!(delivered.writes, vec![file]);
        assert_eq!(delivered.direct, vec![direct]);
    }
}
