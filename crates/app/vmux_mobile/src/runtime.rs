use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use bevy_app::{App, Plugin as BevyPlugin, PluginsState};
use bevy_ecs::message::{Message, MessageReader};
use bevy_ecs::resource::Resource;
use bevy_ecs::schedule::ScheduleLabel;
use bevy_ecs::system::ResMut;
use bevy_window::AppLifecycle;
use vmux_api::page::UiStateEmit;
use vmux_ui::hooks::transport::BytesListener;

pub(crate) struct RuntimePlugin;

impl BevyPlugin for RuntimePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<AppLifecycle>()
            .add_message::<UiStateEmit>()
            .init_resource::<PendingUiEmits>()
            .init_schedule(CollectUiStateEmits)
            .add_systems(CollectUiStateEmits, collect_ui_state);
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct MountId(pub(crate) u64);

#[derive(Clone)]
pub(crate) struct RuntimeHandle(Rc<RuntimeInner>);

#[allow(clippy::type_complexity)]
struct RuntimeInner {
    state: RefCell<MobileRuntime>,
    listeners: RefCell<ListenerRegistry>,
    deferred: RefCell<Vec<Box<dyn FnOnce(&mut bevy_ecs::world::World)>>>,
    next_mount: Cell<u64>,
}

struct MobileRuntime {
    app: App,
    lifecycle: AppLifecycle,
    finished: bool,
}

#[derive(Default)]
struct ListenerRegistry {
    live_mounts: HashSet<MountId>,
    slots: HashMap<String, Vec<(MountId, BytesListener)>>,
}

impl ListenerRegistry {
    fn register(&mut self, mount: MountId, id: String, listener: BytesListener) {
        self.live_mounts.insert(mount);
        let entries = self.slots.entry(id).or_default();
        for (existing_mount, existing_listener) in entries.iter_mut() {
            if *existing_mount == mount {
                *existing_listener = listener;
                return;
            }
        }
        entries.push((mount, listener));
    }

    fn unmount(&mut self, mount: MountId) {
        self.live_mounts.remove(&mount);
        self.slots.retain(|_, entries| {
            entries.retain(|(entry_mount, _)| *entry_mount != mount);
            !entries.is_empty()
        });
    }

    fn take_for(&mut self, id: &str) -> Vec<(MountId, BytesListener)> {
        self.slots.remove(id).unwrap_or_default()
    }

    fn restore(&mut self, id: String, surviving: Vec<(MountId, BytesListener)>) {
        if surviving.is_empty() {
            return;
        }
        let entries = self.slots.entry(id).or_default();
        for (mount, listener) in surviving {
            if !self.live_mounts.contains(&mount) {
                continue;
            }
            if entries.iter().any(|(existing, _)| *existing == mount) {
                continue;
            }
            entries.push((mount, listener));
        }
    }

    #[cfg(test)]
    fn count(&self, id: &str) -> usize {
        self.slots.get(id).map_or(0, Vec::len)
    }
}

#[derive(Resource, Default)]
struct PendingUiEmits(Vec<UiStateEmit>);

#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash)]
struct CollectUiStateEmits;

fn collect_ui_state(mut emitted: MessageReader<UiStateEmit>, mut pending: ResMut<PendingUiEmits>) {
    for emit in emitted.read() {
        pending.0.push(UiStateEmit {
            id: emit.id.clone(),
            bytes: emit.bytes.clone(),
        });
    }
}

impl RuntimeHandle {
    pub(crate) fn from_app(mut app: App) -> Self {
        while app.plugins_state() == PluginsState::Adding {
            bevy_tasks::tick_global_task_pools_on_main_thread();
        }
        app.finish();
        app.cleanup();
        let runtime = Self(Rc::new(RuntimeInner {
            state: RefCell::new(MobileRuntime {
                app,
                lifecycle: AppLifecycle::Idle,
                finished: false,
            }),
            listeners: RefCell::new(ListenerRegistry::default()),
            deferred: RefCell::new(Vec::new()),
            next_mount: Cell::new(0),
        }));
        Self::with_ui_slot(|installed| *installed = Some(runtime.clone()));
        runtime
    }

    pub(crate) fn ui() -> Self {
        Self::with_ui_slot(|installed| {
            installed
                .as_ref()
                .expect("mobile runtime must be installed before Dioxus starts")
                .clone()
        })
    }

    fn with_ui_slot<R>(f: impl FnOnce(&mut Option<RuntimeHandle>) -> R) -> R {
        thread_local! {
            static UI_RUNTIME: RefCell<Option<RuntimeHandle>> = const { RefCell::new(None) };
        }
        UI_RUNTIME.with_borrow_mut(f)
    }

    pub(crate) fn allocate_mount(&self) -> MountId {
        let next = self.0.next_mount.get().wrapping_add(1).max(1);
        self.0.next_mount.set(next);
        let mount = MountId(next);
        self.0.listeners.borrow_mut().live_mounts.insert(mount);
        mount
    }

    pub(crate) fn send<M: Message>(&self, message: M) {
        if let Ok(mut runtime) = self.0.state.try_borrow_mut() {
            runtime.app.world_mut().write_message(message);
            return;
        }
        self.0.deferred.borrow_mut().push(Box::new(move |world| {
            world.write_message(message);
        }));
    }

    pub(crate) fn listen<M: Message>(
        &self,
        mount: MountId,
        id: &str,
        listener: BytesListener,
        refresh: M,
    ) {
        self.0
            .listeners
            .borrow_mut()
            .register(mount, id.to_string(), listener);
        self.send(refresh);
    }

    pub(crate) fn unmount(&self, mount: MountId) {
        self.0.listeners.borrow_mut().unmount(mount);
        self.send(crate::actions::MountRetired(mount));
    }

    pub(crate) fn report_lifecycle(&self, lifecycle: AppLifecycle) {
        if let Ok(mut runtime) = self.0.state.try_borrow_mut() {
            runtime.lifecycle = lifecycle;
            runtime.app.world_mut().write_message(lifecycle);
            return;
        }
        self.0.deferred.borrow_mut().push(Box::new(move |world| {
            world.write_message(lifecycle);
        }));
    }

    pub(crate) fn update(&self) {
        let emits = {
            let Ok(mut runtime) = self.0.state.try_borrow_mut() else {
                tracing::error!("runtime: re-entered while ECS was running; turn dropped");
                return;
            };
            if runtime.finished {
                return;
            }
            if !matches!(
                runtime.lifecycle,
                AppLifecycle::Running | AppLifecycle::WillSuspend | AppLifecycle::WillResume
            ) {
                return;
            }
            self.flush_deferred(runtime.app.world_mut());
            runtime.app.update();
            self.flush_deferred(runtime.app.world_mut());
            runtime.app.world_mut().run_schedule(CollectUiStateEmits);
            let drained =
                std::mem::take(&mut runtime.app.world_mut().resource_mut::<PendingUiEmits>().0);
            if runtime.lifecycle == AppLifecycle::WillSuspend {
                runtime.lifecycle = AppLifecycle::Suspended;
            }
            if runtime.app.should_exit().is_some() {
                runtime.finished = true;
            }
            drained
        };
        for emit in emits {
            self.deliver_emit(emit);
        }
    }

    fn flush_deferred(&self, world: &mut bevy_ecs::world::World) {
        let pending = std::mem::take(&mut *self.0.deferred.borrow_mut());
        for action in pending {
            action(world);
        }
    }

    fn deliver_emit(&self, emit: UiStateEmit) {
        let listeners = self.0.listeners.borrow_mut().take_for(&emit.id);
        if listeners.is_empty() {
            tracing::debug!(id = emit.id, "UI state emit had no listener");
            return;
        }
        let mut surviving = Vec::with_capacity(listeners.len());
        for (mount, mut listener) in listeners {
            let live = self.0.listeners.borrow().live_mounts.contains(&mount);
            if !live {
                continue;
            }
            if listener(&emit.bytes) {
                surviving.push((mount, listener));
            }
        }
        self.0.listeners.borrow_mut().restore(emit.id, surviving);
    }

    #[cfg(test)]
    pub(crate) fn with_world<R>(&self, read: impl FnOnce(&bevy_ecs::world::World) -> R) -> R {
        let runtime = self.0.state.borrow();
        read(runtime.app.world())
    }

    #[cfg(test)]
    pub(crate) fn with_world_mut<R>(
        &self,
        mutate: impl FnOnce(&mut bevy_ecs::world::World) -> R,
    ) -> R {
        let mut runtime = self.0.state.borrow_mut();
        mutate(runtime.app.world_mut())
    }

    #[cfg(test)]
    pub(crate) fn listener_count(&self, id: &str) -> usize {
        self.0.listeners.borrow().count(id)
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;
    use bevy_app::{AppExit, Update};
    use bevy_ecs::system::ResMut;

    #[derive(Resource, Default)]
    struct Turns(usize);

    #[derive(Message)]
    struct Ping;

    #[derive(Resource, Default)]
    struct Pings(usize);

    struct CountingFixture;

    impl CountingFixture {
        fn build() -> RuntimeHandle {
            let mut app = App::new();
            app.add_plugins(RuntimePlugin)
                .add_message::<Ping>()
                .add_message::<crate::actions::MountRetired>()
                .init_resource::<Turns>()
                .init_resource::<Pings>()
                .add_systems(
                    Update,
                    (
                        |mut turns: ResMut<Turns>| turns.0 += 1,
                        |mut reader: MessageReader<Ping>, mut pings: ResMut<Pings>| {
                            pings.0 += reader.read().count();
                        },
                    ),
                );
            RuntimeHandle::from_app(app)
        }

        fn turns(runtime: &RuntimeHandle) -> usize {
            runtime.with_world(|world| world.resource::<Turns>().0)
        }
    }

    #[test]
    fn an_idle_world_does_not_run_until_it_is_told_the_app_is_running() {
        let runtime = CountingFixture::build();
        runtime.update();
        assert_eq!(
            CountingFixture::turns(&runtime),
            0,
            "a world nobody has resumed must not run"
        );

        runtime.report_lifecycle(AppLifecycle::Running);
        runtime.update();
        assert_eq!(CountingFixture::turns(&runtime), 1);
    }

    #[test]
    fn suspending_owes_exactly_one_more_turn_and_then_stops() {
        let runtime = CountingFixture::build();
        runtime.report_lifecycle(AppLifecycle::Running);
        runtime.update();

        runtime.report_lifecycle(AppLifecycle::WillSuspend);
        runtime.update();
        let owed = CountingFixture::turns(&runtime);
        assert_eq!(owed, 2, "WillSuspend is owed the frame a plugin saves from");

        for _ in 0..5 {
            runtime.update();
        }
        assert_eq!(
            owed,
            CountingFixture::turns(&runtime),
            "a suspended world must not run"
        );
    }

    #[test]
    fn a_resumed_world_runs_again() {
        let runtime = CountingFixture::build();
        runtime.report_lifecycle(AppLifecycle::Running);
        runtime.report_lifecycle(AppLifecycle::WillSuspend);
        runtime.update();
        runtime.update();
        let suspended = CountingFixture::turns(&runtime);

        runtime.report_lifecycle(AppLifecycle::Running);
        runtime.update();
        assert_eq!(CountingFixture::turns(&runtime), suspended + 1);
    }

    #[test]
    fn a_world_that_has_exited_stops_running_systems() {
        let runtime = CountingFixture::build();
        runtime.report_lifecycle(AppLifecycle::Running);
        runtime.update();
        runtime.send(AppExit::Success);
        runtime.update();
        let exited = CountingFixture::turns(&runtime);

        runtime.update();
        assert_eq!(
            exited,
            CountingFixture::turns(&runtime),
            "an exited world must not run again"
        );
    }

    #[test]
    fn callbacks_deliver_after_releasing_runtime_borrow_and_preserve_independent_mounts() {
        let runtime = CountingFixture::build();
        runtime.report_lifecycle(AppLifecycle::Running);

        let mount_a = runtime.allocate_mount();
        let mount_b = runtime.allocate_mount();
        let hits_a = Rc::new(Cell::new(0usize));
        let hits_b = Rc::new(Cell::new(0usize));

        let cb_runtime = runtime.clone();
        let cb_hits_a = hits_a.clone();
        runtime.listen(
            mount_a,
            "state.demo",
            Box::new(move |_| {
                cb_hits_a.set(cb_hits_a.get() + 1);
                cb_runtime.send(Ping);
                true
            }),
            Ping,
        );
        let cb_hits_b = hits_b.clone();
        runtime.listen(
            mount_b,
            "state.demo",
            Box::new(move |_| {
                cb_hits_b.set(cb_hits_b.get() + 1);
                true
            }),
            Ping,
        );
        assert_eq!(runtime.listener_count("state.demo"), 2);

        runtime.send(UiStateEmit {
            id: "state.demo".to_string(),
            bytes: vec![1],
        });
        runtime.update();
        assert_eq!(hits_a.get(), 1);
        assert_eq!(hits_b.get(), 1);

        runtime.unmount(mount_a);
        assert_eq!(runtime.listener_count("state.demo"), 1);

        runtime.send(UiStateEmit {
            id: "state.demo".to_string(),
            bytes: vec![2],
        });
        runtime.update();
        assert_eq!(hits_a.get(), 1);
        assert_eq!(hits_b.get(), 2);
        assert_eq!(runtime.with_world(|world| world.resource::<Pings>().0), 3);
    }
}
