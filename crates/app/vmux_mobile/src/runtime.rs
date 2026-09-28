use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use bevy_app::{App, Plugin, PluginsState};
use bevy_ecs::component::Component;
use bevy_ecs::message::{Message, MessageReader};
use bevy_ecs::schedule::ScheduleLabel;
use bevy_ecs::system::NonSendMut;
use bevy_window::AppLifecycle;
use vmux_api::page::UiStateEmit;
use vmux_ui::hooks::transport::BytesListener;

#[derive(Clone)]
pub(crate) struct RuntimeHandle(Rc<RefCell<MobileRuntime>>);

thread_local! {
    static UI_RUNTIME: RefCell<Option<RuntimeHandle>> = const { RefCell::new(None) };
}

pub(crate) struct MobileRuntime {
    app: App,
    lifecycle: AppLifecycle,
    finished: bool,
}

#[derive(Default)]
pub(crate) struct UiStateListeners(pub(crate) HashMap<String, BytesListener>);

pub(crate) struct MobileRuntimePlugin;

#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash)]
struct DeliverUiStateEmits;

impl Plugin for MobileRuntimePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<AppLifecycle>()
            .add_message::<UiStateEmit>()
            .insert_non_send(UiStateListeners::default())
            .init_schedule(DeliverUiStateEmits)
            .add_systems(DeliverUiStateEmits, deliver_ui_state);
    }
}

impl RuntimeHandle {
    pub(crate) fn from_app(mut app: App) -> Self {
        while app.plugins_state() == PluginsState::Adding {
            bevy_tasks::tick_global_task_pools_on_main_thread();
        }
        app.finish();
        app.cleanup();
        let runtime = Self(Rc::new(RefCell::new(MobileRuntime {
            app,
            lifecycle: AppLifecycle::Idle,
            finished: false,
        })));
        UI_RUNTIME.with_borrow_mut(|installed| *installed = Some(runtime.clone()));
        runtime
    }

    pub(crate) fn ui() -> Self {
        UI_RUNTIME.with_borrow(|installed| {
            installed
                .as_ref()
                .expect("mobile runtime must be installed before Dioxus starts")
                .clone()
        })
    }

    pub(crate) fn send<M: Message>(&self, message: M) {
        let Ok(mut runtime) = self.0.try_borrow_mut() else {
            tracing::error!("runtime: message arrived while ECS was running; message dropped");
            return;
        };
        runtime.app.world_mut().write_message(message);
    }

    pub(crate) fn project<T: Component, R>(&self, read: impl FnOnce(&T) -> R) -> Option<R> {
        let Ok(mut runtime) = self.0.try_borrow_mut() else {
            return None;
        };
        let world = runtime.app.world_mut();
        let mut query = world.query::<&T>();
        query.single(world).ok().map(read)
    }

    pub(crate) fn listen<M: Message>(&self, id: &str, listener: BytesListener, refresh: M) {
        let Ok(mut runtime) = self.0.try_borrow_mut() else {
            tracing::error!("runtime: listener arrived while ECS was running; listener dropped");
            return;
        };
        let world = runtime.app.world_mut();
        world
            .non_send_mut::<UiStateListeners>()
            .0
            .insert(id.to_string(), listener);
        world.write_message(refresh);
    }

    pub(crate) fn configure_non_send<T: 'static>(&self, configure: impl FnOnce(&mut T)) {
        let Ok(mut runtime) = self.0.try_borrow_mut() else {
            tracing::error!("runtime: subscription arrived while ECS was running");
            return;
        };
        configure(&mut runtime.app.world_mut().non_send_mut::<T>());
    }

    pub(crate) fn report_lifecycle(&self, lifecycle: AppLifecycle) {
        let Ok(mut runtime) = self.0.try_borrow_mut() else {
            tracing::error!("runtime: lifecycle changed while ECS was running; event dropped");
            return;
        };
        runtime.lifecycle = lifecycle;
        runtime.app.world_mut().write_message(lifecycle);
    }

    pub(crate) fn update(&self) {
        let Ok(mut runtime) = self.0.try_borrow_mut() else {
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
        runtime.app.update();
        runtime.app.world_mut().run_schedule(DeliverUiStateEmits);
        if runtime.lifecycle == AppLifecycle::WillSuspend {
            runtime.lifecycle = AppLifecycle::Suspended;
        }
        if runtime.app.should_exit().is_some() {
            runtime.finished = true;
        }
    }
}

fn deliver_ui_state(
    mut emitted: MessageReader<UiStateEmit>,
    mut listeners: NonSendMut<UiStateListeners>,
) {
    for emit in emitted.read() {
        let Some(listener) = listeners.0.get_mut(&emit.id) else {
            tracing::debug!(id = emit.id, "UI state emit had no listener");
            continue;
        };
        listener(&emit.bytes);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_app::{AppExit, Update};
    use bevy_ecs::resource::Resource;
    use bevy_ecs::system::ResMut;

    #[derive(Resource, Default)]
    struct Turns(usize);

    fn counting_runtime() -> RuntimeHandle {
        let mut app = App::new();
        app.add_plugins(MobileRuntimePlugin)
            .init_resource::<Turns>()
            .add_systems(Update, |mut turns: ResMut<Turns>| turns.0 += 1);
        RuntimeHandle::from_app(app)
    }

    fn turns(runtime: &RuntimeHandle) -> usize {
        runtime.0.borrow().app.world().resource::<Turns>().0
    }

    #[test]
    fn an_idle_world_does_not_run_until_it_is_told_the_app_is_running() {
        let runtime = counting_runtime();
        runtime.update();
        assert_eq!(
            turns(&runtime),
            0,
            "a world nobody has resumed must not run"
        );

        runtime.report_lifecycle(AppLifecycle::Running);
        runtime.update();
        assert_eq!(turns(&runtime), 1);
    }

    #[test]
    fn suspending_owes_exactly_one_more_turn_and_then_stops() {
        let runtime = counting_runtime();
        runtime.report_lifecycle(AppLifecycle::Running);
        runtime.update();

        runtime.report_lifecycle(AppLifecycle::WillSuspend);
        runtime.update();
        let owed = turns(&runtime);
        assert_eq!(owed, 2, "WillSuspend is owed the frame a plugin saves from");

        for _ in 0..5 {
            runtime.update();
        }
        assert_eq!(owed, turns(&runtime), "a suspended world must not run");
    }

    #[test]
    fn a_resumed_world_runs_again() {
        let runtime = counting_runtime();
        runtime.report_lifecycle(AppLifecycle::Running);
        runtime.report_lifecycle(AppLifecycle::WillSuspend);
        runtime.update();
        runtime.update();
        let suspended = turns(&runtime);

        runtime.report_lifecycle(AppLifecycle::Running);
        runtime.update();
        assert_eq!(turns(&runtime), suspended + 1);
    }

    #[test]
    fn a_world_that_has_exited_stops_running_systems() {
        let runtime = counting_runtime();
        runtime.report_lifecycle(AppLifecycle::Running);
        runtime.update();
        runtime.send(AppExit::Success);
        runtime.update();
        let exited = turns(&runtime);

        runtime.update();
        assert_eq!(
            exited,
            turns(&runtime),
            "an exited world must not run again"
        );
    }
}
