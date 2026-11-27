use std::time::Duration;

use bevy_app::{App, Plugin};
use bevy_ecs::prelude::{ChildOf, Commands, Component, Entity, On, Query, With, World};
use rkyv::api::high::{HighDeserializer, HighSerializer, HighValidator};
use rkyv::bytecheck::CheckBytes;
use rkyv::ser::allocator::ArenaHandle;
use rkyv::util::AlignedVec;
use vmux_api::{UiEvent, UiEventPermissions, UiState as UiStateContract, UiStateFrame};
use vmux_ecs::{
    Operation, OperationPlugin, OperationState, PageMetadata, PrimitivesPlugin, UiEmitEvent,
    UiIpcBlocked, UiIpcEpoch, UiIpcGeneration, UiIpcNavigationRevision, UiIpcPlugin, UiIpcReady,
    UiPageReady, UiReceiveRaw,
};
use vmux_time::Clock;

pub struct TestPagePlugin;

impl Plugin for TestPagePlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<UiIpcPlugin>() {
            app.add_plugins(UiIpcPlugin);
        }
        app.add_observer(capture_emit);
    }
}

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[require(
    UiIpcGeneration,
    UiIpcNavigationRevision,
    UiIpcEpoch,
    UiIpcReady,
    TestPageSequence
)]
pub struct TestPage;

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
struct TestPageSequence(u64);

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct RecordedEmit {
    pub sequence: u64,
    pub id: &'static str,
    pub payload: Vec<u8>,
}

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TestPageReject;

type TestPageTargets<'w, 's> = Query<
    'w,
    's,
    (
        &'static UiIpcGeneration,
        &'static UiIpcNavigationRevision,
        &'static UiIpcEpoch,
        &'static PageMetadata,
        &'static mut TestPageSequence,
    ),
    (With<TestPage>, With<UiIpcReady>),
>;

fn capture_emit(
    trigger: On<UiEmitEvent>,
    mut pages: TestPageTargets,
    blocked: Query<(), With<UiIpcBlocked>>,
    rejected: Query<(), With<TestPageReject>>,
    permissions: Query<&UiEventPermissions>,
    mut commands: Commands,
) {
    let event = trigger.event();
    let entity = event.target;
    if blocked.contains(entity) {
        return;
    }
    let Ok((generation, revision, epoch, metadata, mut seq)) = pages.get_mut(entity) else {
        return;
    };
    if !event.matches_authority(generation.0, revision.0, epoch.0) {
        return;
    }
    if !UiEventPermissions::allows_page(permissions.iter(), &metadata.url, event.permission) {
        return;
    }
    seq.0 = seq.0.saturating_add(1);
    commands.spawn((
        RecordedEmit {
            sequence: seq.0,
            id: event.id,
            payload: event.payload.to_vec(),
        },
        ChildOf(entity),
    ));
    if !rejected.contains(entity) {
        event.receipt.mark_delivered();
    }
}

pub trait TestAppExt {
    fn init_test_harness(&mut self, initial_unix_millis: i64) -> Clock;

    fn try_finish_composition(&mut self) -> Result<(), String>;

    fn tick(&mut self);

    fn tick_n(&mut self, count: usize);

    fn advance(&mut self, duration: Duration);

    fn tick_until(
        &mut self,
        max_ticks: usize,
        predicate: impl FnMut(&World) -> bool,
    ) -> Result<usize, String>;

    fn spawn_test_page(
        &mut self,
        url: &'static str,
        permissions: &'static [&'static str],
    ) -> Entity;

    fn send_test_ui_event<E>(&mut self, page: Entity, event: &E)
    where
        E: UiEvent
            + for<'a> rkyv::Serialize<
                HighSerializer<AlignedVec, ArenaHandle<'a>, rkyv::rancor::Error>,
            >;

    fn trigger_test_page_ready(&mut self, page: Entity);

    fn ui_state_frames<S>(&mut self, page: Entity) -> Vec<UiStateFrame<S, S::Update>>
    where
        S: UiStateContract,
        UiStateFrame<S, S::Update>: rkyv::Archive,
        <UiStateFrame<S, S::Update> as rkyv::Archive>::Archived: rkyv::Deserialize<UiStateFrame<S, S::Update>, HighDeserializer<rkyv::rancor::Error>>
            + for<'a> CheckBytes<HighValidator<'a, rkyv::rancor::Error>>;

    fn last_ui_state<S>(&mut self, page: Entity) -> Option<S>
    where
        S: UiStateContract,
        UiStateFrame<S, S::Update>: rkyv::Archive,
        <UiStateFrame<S, S::Update> as rkyv::Archive>::Archived: rkyv::Deserialize<UiStateFrame<S, S::Update>, HighDeserializer<rkyv::rancor::Error>>
            + for<'a> CheckBytes<HighValidator<'a, rkyv::rancor::Error>>;

    fn spawn_test_operation(&mut self, kind: &'static str) -> Entity;

    fn operation_state(&self, operation: Entity) -> Option<OperationState>;

    fn assert_operation_succeeded(&self, operation: Entity);

    fn assert_operation_failed(&self, operation: Entity);

    fn assert_operation_canceled(&self, operation: Entity);
}

impl TestAppExt for App {
    fn init_test_harness(&mut self, initial_unix_millis: i64) -> Clock {
        let clock = Clock::fake(initial_unix_millis);
        self.insert_resource(clock.clone());
        if !self.is_plugin_added::<PrimitivesPlugin>() {
            self.add_plugins(PrimitivesPlugin);
        }
        if !self.is_plugin_added::<OperationPlugin>() {
            self.add_plugins(OperationPlugin);
        }
        if !self.is_plugin_added::<TestPagePlugin>() {
            self.add_plugins(TestPagePlugin);
        }
        clock
    }

    fn try_finish_composition(&mut self) -> Result<(), String> {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.finish()));
        match result {
            Ok(()) => {
                self.cleanup();
                Ok(())
            }
            Err(error) => {
                if let Some(message) = error.downcast_ref::<String>() {
                    return Err(message.clone());
                }
                if let Some(message) = error.downcast_ref::<&str>() {
                    return Err((*message).to_string());
                }
                Err(String::from("non-string composition failure"))
            }
        }
    }

    fn tick(&mut self) {
        self.update();
    }

    fn tick_n(&mut self, count: usize) {
        for _ in 0..count {
            self.update();
        }
    }

    fn advance(&mut self, duration: Duration) {
        if let Some(clock) = self.world().get_resource::<Clock>().cloned() {
            clock.advance(duration);
        }
        self.update();
    }

    fn tick_until(
        &mut self,
        max_ticks: usize,
        mut predicate: impl FnMut(&World) -> bool,
    ) -> Result<usize, String> {
        for tick in 1..=max_ticks {
            self.update();
            if predicate(self.world()) {
                return Ok(tick);
            }
        }
        Err(format!("condition not satisfied within {max_ticks} ticks"))
    }

    fn spawn_test_page(
        &mut self,
        url: &'static str,
        permissions: &'static [&'static str],
    ) -> Entity {
        self.world_mut().spawn(UiEventPermissions {
            url,
            owns_subtree: false,
            permissions,
        });
        self.world_mut()
            .spawn((
                TestPage,
                UiIpcGeneration(1),
                UiIpcNavigationRevision(1),
                UiIpcEpoch(1),
                PageMetadata {
                    url: url.to_string(),
                    ..Default::default()
                },
            ))
            .id()
    }

    fn send_test_ui_event<E>(&mut self, page: Entity, event: &E)
    where
        E: UiEvent
            + for<'a> rkyv::Serialize<
                HighSerializer<AlignedVec, ArenaHandle<'a>, rkyv::rancor::Error>,
            >,
    {
        let authority = (
            self.world()
                .get::<UiIpcGeneration>(page)
                .map_or(1, |value| value.0),
            self.world()
                .get::<UiIpcNavigationRevision>(page)
                .map_or(1, |value| value.0),
            self.world()
                .get::<UiIpcEpoch>(page)
                .map_or(1, |value| value.0),
        );
        let url = self
            .world()
            .get::<PageMetadata>(page)
            .map(|meta| meta.url.clone())
            .unwrap_or_default();
        self.world_mut()
            .write_message(UiReceiveRaw::from_event(page, url, authority, event));
    }

    fn trigger_test_page_ready(&mut self, page: Entity) {
        self.world_mut().trigger(UiPageReady { page });
    }

    fn ui_state_frames<S>(&mut self, page: Entity) -> Vec<UiStateFrame<S, S::Update>>
    where
        S: UiStateContract,
        UiStateFrame<S, S::Update>: rkyv::Archive,
        <UiStateFrame<S, S::Update> as rkyv::Archive>::Archived: rkyv::Deserialize<UiStateFrame<S, S::Update>, HighDeserializer<rkyv::rancor::Error>>
            + for<'a> CheckBytes<HighValidator<'a, rkyv::rancor::Error>>,
    {
        let world = self.world_mut();
        let mut records: Vec<(u64, Vec<u8>)> = world
            .query::<(&RecordedEmit, &ChildOf)>()
            .iter(world)
            .filter(|(emit, parent)| parent.parent() == page && emit.id == S::id())
            .map(|(emit, _)| (emit.sequence, emit.payload.clone()))
            .collect();
        records.sort_by_key(|(sequence, _)| *sequence);
        let mut decoded = Vec::new();
        for (_, payload) in records {
            if let Ok(frame) =
                rkyv::from_bytes::<UiStateFrame<S, S::Update>, rkyv::rancor::Error>(&payload)
            {
                decoded.push(frame);
            }
        }
        decoded
    }

    fn last_ui_state<S>(&mut self, page: Entity) -> Option<S>
    where
        S: UiStateContract,
        UiStateFrame<S, S::Update>: rkyv::Archive,
        <UiStateFrame<S, S::Update> as rkyv::Archive>::Archived: rkyv::Deserialize<UiStateFrame<S, S::Update>, HighDeserializer<rkyv::rancor::Error>>
            + for<'a> CheckBytes<HighValidator<'a, rkyv::rancor::Error>>,
    {
        let mut current = None;
        for frame in self.ui_state_frames::<S>(page) {
            match frame {
                UiStateFrame::Snapshot(state) => current = Some(state),
                UiStateFrame::Updates(updates) => {
                    current = Some(S::from_updates(current.take(), updates));
                }
            }
        }
        current
    }

    fn spawn_test_operation(&mut self, kind: &'static str) -> Entity {
        self.world_mut()
            .spawn(Operation::bundle(kind).expect("valid operation kind"))
            .id()
    }

    fn operation_state(&self, operation: Entity) -> Option<OperationState> {
        self.world().get::<OperationState>(operation).cloned()
    }

    fn assert_operation_succeeded(&self, operation: Entity) {
        assert_eq!(
            self.operation_state(operation),
            Some(OperationState::Succeeded)
        );
    }

    fn assert_operation_failed(&self, operation: Entity) {
        assert_eq!(
            self.operation_state(operation),
            Some(OperationState::Failed)
        );
    }

    fn assert_operation_canceled(&self, operation: Entity) {
        assert_eq!(
            self.operation_state(operation),
            Some(OperationState::Canceled)
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_ecs::prelude::Query;
    use vmux_api::BinEvent;
    use vmux_ecs::{
        OperationCompleted, UiIpcEventPlugin, UiIpcInput, UiStatePlugin, UiStateWriter,
    };

    #[vmux_api::ui_state(Default, Eq)]
    struct DemoUiState {
        value: i32,
    }

    #[vmux_api::ui_event(Eq)]
    struct DemoIncrement {
        delta: i32,
    }

    #[derive(Component, Default)]
    struct Counter(i32);

    fn handle_increment(
        trigger: On<UiIpcInput<DemoIncrement>>,
        mut counters: Query<&mut Counter>,
        mut writer: UiStateWriter<DemoUiState>,
    ) {
        let page = trigger.event().page;
        if let Ok(mut counter) = counters.get_mut(page) {
            counter.0 += trigger.event().delta;
            writer.push(page, DemoUiState { value: counter.0 });
        }
    }

    #[test]
    fn test_app_ext_drives_ui_events_ui_state_clock_and_operations() {
        let mut app = App::new();
        let clock = app.init_test_harness(1_700_000_000_000);
        app.add_plugins((
            UiStatePlugin::<DemoUiState>::default(),
            UiIpcEventPlugin::<DemoIncrement>::default(),
        ))
        .add_observer(handle_increment);

        let page = app.spawn_test_page(
            "app://demo/",
            &[DemoUiState::PERMISSION, DemoIncrement::PERMISSION],
        );
        app.world_mut().entity_mut(page).insert(Counter(0));

        app.send_test_ui_event(page, &DemoIncrement { delta: 5 });
        let ticks = app
            .tick_until(5, |world| {
                world.get::<Counter>(page).is_some_and(|c| c.0 == 5)
            })
            .unwrap();
        assert_eq!(ticks, 1);
        assert_eq!(
            app.last_ui_state::<DemoUiState>(page),
            Some(DemoUiState { value: 5 })
        );

        app.advance(Duration::from_secs(2));
        assert_eq!(clock.unix_millis(), 1_700_000_002_000);

        let op = app.spawn_test_operation("demo.work");
        app.world_mut().trigger(OperationCompleted {
            entity: op,
            result: Ok(()),
        });
        app.assert_operation_succeeded(op);
    }

    #[test]
    fn test_page_reject_defers_delivery_for_retry_testing() {
        let mut app = App::new();
        app.init_test_harness(1_700_000_000_000);
        app.add_plugins(UiStatePlugin::<DemoUiState>::default());
        let page = app.spawn_test_page("app://retry/", &[DemoUiState::PERMISSION]);
        app.world_mut().entity_mut(page).insert(TestPageReject);
        app.world_mut()
            .trigger(vmux_ecs::UiStateWrite::<DemoUiState>::from_event(
                page,
                &DemoUiState { value: 9 },
            ));
        app.tick();
        assert_eq!(app.ui_state_frames::<DemoUiState>(page).len(), 1);

        app.world_mut().entity_mut(page).remove::<TestPageReject>();
        app.tick();
        assert_eq!(app.ui_state_frames::<DemoUiState>(page).len(), 2);
        assert_eq!(
            app.last_ui_state::<DemoUiState>(page),
            Some(DemoUiState { value: 9 })
        );
    }

    #[test]
    fn try_finish_composition_reports_panics_as_err() {
        struct FailingPlugin;
        impl Plugin for FailingPlugin {
            fn build(&self, _app: &mut App) {}
            fn finish(&self, _app: &mut App) {
                panic!("incompatible composition");
            }
        }
        let mut app = App::new();
        app.add_plugins(FailingPlugin);
        let error = app.try_finish_composition().unwrap_err();
        assert!(error.contains("incompatible composition"));
    }
}
