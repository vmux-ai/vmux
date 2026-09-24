use std::marker::PhantomData;

use bevy::prelude::*;
use bevy_cef::prelude::{BinHostEmitEvent, Browsers};
use rkyv::api::high::HighSerializer;
use rkyv::ser::allocator::ArenaHandle;
use rkyv::util::AlignedVec;
use vmux_api::{BatchedUiState, UiState as UiStateContract};

pub struct UiStatePlugin<S>(PhantomData<fn() -> S>);

impl<S> Default for UiStatePlugin<S> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<S> Plugin for UiStatePlugin<S>
where
    S: UiStateContract
        + for<'a> rkyv::Serialize<HighSerializer<AlignedVec, ArenaHandle<'a>, rkyv::rancor::Error>>,
{
    fn build(&self, app: &mut App) {
        app.add_observer(UiState::<S>::collect)
            .add_observer(UiState::<S>::replay)
            .add_systems(Last, UiState::<S>::emit);
    }
}

#[derive(Component)]
pub struct UiState<S: UiStateContract> {
    sequence: u64,
    updates: Vec<S::Update>,
    retained: Option<S>,
    replay: bool,
    state: PhantomData<fn() -> S>,
}

impl<S: UiStateContract> Default for UiState<S> {
    fn default() -> Self {
        Self {
            sequence: 0,
            updates: Vec::new(),
            retained: None,
            replay: false,
            state: PhantomData,
        }
    }
}

impl<S: UiStateContract> UiState<S> {
    pub fn current(&self) -> Option<&S> {
        self.retained.as_ref()
    }

    pub fn write<T>(commands: &mut Commands, webview: Entity, event: &T)
    where
        T: Clone + Into<S::Update>,
    {
        commands.trigger(UiStateWrite::<S>::from_event(webview, event));
    }

    pub fn deliver<T>(
        pages: &Query<(), With<Self>>,
        commands: &mut Commands,
        webview: Entity,
        event: &T,
    ) where
        T: Clone + Into<S::Update>,
    {
        if pages.contains(webview) {
            Self::write(commands, webview, event);
        }
    }

    fn push(&mut self, update: S::Update) {
        self.updates.push(update);
    }

    fn take(&mut self) -> Option<S> {
        if !self.updates.is_empty() {
            self.sequence = self.sequence.wrapping_add(1).max(1);
            let state = S::from_updates(self.sequence, std::mem::take(&mut self.updates));
            self.retained = state.retained();
            self.replay = false;
            return Some(state);
        }
        if !self.replay {
            return None;
        }
        self.replay = false;
        self.retained.clone()
    }

    fn collect(
        trigger: On<UiStateWrite<S>>,
        mut updates: Query<&mut Self>,
        mut commands: Commands,
    ) {
        match updates.get_mut(trigger.event().webview) {
            Ok(mut updates) => updates.push(trigger.event().update.clone()),
            Err(_) => {
                let mut updates = Self::default();
                updates.push(trigger.event().update.clone());
                commands.entity(trigger.event().webview).insert(updates);
            }
        }
    }

    fn replay(
        trigger: On<bevy_cef::prelude::BinReceive<vmux_api::PageReady>>,
        mut updates: Query<&mut Self>,
    ) {
        let Ok(mut updates) = updates.get_mut(trigger.event().webview) else {
            return;
        };
        updates.replay = true;
    }

    fn emit(
        mut updates: Query<(Entity, &mut Self)>,
        browsers: Option<NonSend<Browsers>>,
        mut commands: Commands,
    ) where
        S: for<'a> rkyv::Serialize<
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
pub struct UiStateWrite<S: UiStateContract> {
    #[event_target]
    webview: Entity,
    update: S::Update,
}

impl<S: UiStateContract> UiStateWrite<S> {
    pub fn from_event<T>(webview: Entity, event: &T) -> Self
    where
        T: Clone + Into<S::Update>,
    {
        Self {
            webview,
            update: event.clone().into(),
        }
    }

    pub fn webview(&self) -> Entity {
        self.webview
    }

    pub fn update(&self) -> &S::Update {
        &self.update
    }
}

impl<S: BatchedUiState> UiStateWrite<S> {
    pub fn patch(&self) -> &S::Patch {
        &self.update
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{FileDirtyEvent, FileUiState, FileUiStatePatch};
    use bevy_cef::prelude::BinReceive;
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
    }

    #[vmux_api::ui_state(Default, Eq, target = any)]
    struct SnapshotState {
        value: u32,
    }

    #[derive(Resource, Default)]
    struct SnapshotEmitted(Vec<SnapshotState>);

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
    }

    impl SnapshotEmitted {
        fn record(trigger: On<BinHostEmitEvent>, mut emitted: ResMut<Self>) {
            if trigger.event().id() != SnapshotState::id() {
                return;
            }
            let state =
                rkyv::from_bytes::<SnapshotState, rkyv::rancor::Error>(trigger.event().payload())
                    .unwrap();
            emitted.0.push(state);
        }
    }

    fn deliver(
        targets: Res<Targets>,
        pages: Query<(), With<UiState<FileUiState>>>,
        mut commands: Commands,
    ) {
        UiState::<FileUiState>::deliver(&pages, &mut commands, targets.file, &GitChangedEvent {});
        UiState::<FileUiState>::deliver(&pages, &mut commands, targets.direct, &GitChangedEvent {});
    }

    #[test]
    fn same_frame_updates_emit_one_batch() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, UiStatePlugin::<FileUiState>::default()))
            .init_resource::<Emitted>()
            .add_observer(Emitted::record);
        let entity = app
            .world_mut()
            .spawn(UiState::<FileUiState>::default())
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
    fn snapshot_updates_retry_and_replay_the_latest_value() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, UiStatePlugin::<SnapshotState>::default()))
            .init_resource::<SnapshotEmitted>()
            .add_observer(SnapshotEmitted::record);
        let entity = app.world_mut().spawn_empty().id();

        app.world_mut()
            .trigger(UiStateWrite::<SnapshotState>::from_event(
                entity,
                &SnapshotState { value: 7 },
            ));
        app.update();
        assert!(app.world().resource::<SnapshotEmitted>().0.is_empty());

        let mut browsers = Browsers::default();
        browsers.set_externally_hosted(entity);
        app.world_mut().insert_non_send(browsers);
        app.update();

        app.world_mut().trigger(BinReceive {
            webview: entity,
            payload: vmux_api::PageReady {},
        });
        app.update();

        let emitted = &app.world().resource::<SnapshotEmitted>().0;
        assert_eq!(emitted.len(), 2);
        assert!(emitted.iter().all(|state| state.value == 7));
    }

    #[test]
    fn delivery_only_targets_pages_with_the_requested_ui_state() {
        let mut app = App::new();
        let file = app
            .world_mut()
            .spawn(UiState::<FileUiState>::default())
            .id();
        let direct = app.world_mut().spawn_empty().id();
        app.insert_resource(Targets { file, direct })
            .init_resource::<Delivered>()
            .add_observer(Delivered::write)
            .add_systems(Update, deliver);

        app.update();

        let delivered = app.world().resource::<Delivered>();
        assert_eq!(delivered.writes, vec![file]);
    }
}
