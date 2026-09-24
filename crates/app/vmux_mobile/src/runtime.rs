use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use bevy_app::{App, Plugin, PluginsState};
use bevy_ecs::message::MessageReader;
use bevy_ecs::schedule::ScheduleLabel;
use bevy_ecs::system::NonSendMut;
use bevy_window::AppLifecycle;
use vmux_api::page::PageEmit;
use vmux_ui::hooks::transport::BytesListener;

pub(crate) type RuntimeHandle = Rc<RefCell<MobileRuntime>>;

thread_local! {
    static UI_RUNTIME: RefCell<Option<RuntimeHandle>> = const { RefCell::new(None) };
}

pub(crate) struct MobileRuntime {
    pub(crate) app: App,
    lifecycle: AppLifecycle,
    finished: bool,
}

#[derive(Default)]
pub(crate) struct PageListeners(pub(crate) HashMap<String, BytesListener>);

struct MobileRuntimePlugin;

#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash)]
struct DeliverPageEmits;

impl Plugin for MobileRuntimePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<AppLifecycle>()
            .add_message::<PageEmit>()
            .insert_non_send(PageListeners::default())
            .init_schedule(DeliverPageEmits)
            .add_systems(DeliverPageEmits, deliver_page_emits);
    }
}

pub(crate) fn create(plugins: impl FnOnce(&mut App)) -> RuntimeHandle {
    let mut app = App::new();
    app.add_plugins(MobileRuntimePlugin);
    plugins(&mut app);
    while app.plugins_state() == PluginsState::Adding {
        bevy_tasks::tick_global_task_pools_on_main_thread();
    }
    app.finish();
    app.cleanup();
    Rc::new(RefCell::new(MobileRuntime {
        app,
        lifecycle: AppLifecycle::Idle,
        finished: false,
    }))
}

pub(crate) fn install_ui(runtime: RuntimeHandle) {
    UI_RUNTIME.with_borrow_mut(|installed| *installed = Some(runtime));
}

pub(crate) fn ui() -> RuntimeHandle {
    UI_RUNTIME.with_borrow(|installed| {
        installed
            .as_ref()
            .expect("mobile runtime must be installed before Dioxus starts")
            .clone()
    })
}

pub(crate) fn report_lifecycle(runtime: &RuntimeHandle, lifecycle: AppLifecycle) {
    let mut runtime = match runtime.try_borrow_mut() {
        Ok(runtime) => runtime,
        Err(_) => {
            tracing::error!("runtime: lifecycle changed while ECS was running; event dropped");
            return;
        }
    };
    runtime.lifecycle = lifecycle;
    runtime.app.world_mut().write_message(lifecycle);
}

pub(crate) fn update(runtime: &RuntimeHandle) {
    let Ok(mut runtime) = runtime.try_borrow_mut() else {
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
    runtime.app.world_mut().run_schedule(DeliverPageEmits);
    if runtime.lifecycle == AppLifecycle::WillSuspend {
        runtime.lifecycle = AppLifecycle::Suspended;
    }
    if runtime.app.should_exit().is_some() {
        runtime.finished = true;
    }
}

fn deliver_page_emits(
    mut emitted: MessageReader<PageEmit>,
    mut listeners: NonSendMut<PageListeners>,
) {
    for emit in emitted.read() {
        let Some(listener) = listeners.0.get_mut(&emit.id) else {
            tracing::debug!(id = emit.id, "page emit had no listener");
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
        create(|app| {
            app.init_resource::<Turns>()
                .add_systems(Update, |mut turns: ResMut<Turns>| turns.0 += 1);
        })
    }

    fn turns(runtime: &RuntimeHandle) -> usize {
        runtime.borrow().app.world().resource::<Turns>().0
    }

    #[test]
    fn an_idle_world_does_not_run_until_it_is_told_the_app_is_running() {
        let runtime = counting_runtime();
        update(&runtime);
        assert_eq!(
            turns(&runtime),
            0,
            "a world nobody has resumed must not run"
        );

        report_lifecycle(&runtime, AppLifecycle::Running);
        update(&runtime);
        assert_eq!(turns(&runtime), 1);
    }

    #[test]
    fn suspending_owes_exactly_one_more_turn_and_then_stops() {
        let runtime = counting_runtime();
        report_lifecycle(&runtime, AppLifecycle::Running);
        update(&runtime);

        report_lifecycle(&runtime, AppLifecycle::WillSuspend);
        update(&runtime);
        let owed = turns(&runtime);
        assert_eq!(owed, 2, "WillSuspend is owed the frame a plugin saves from");

        for _ in 0..5 {
            update(&runtime);
        }
        assert_eq!(owed, turns(&runtime), "a suspended world must not run");
    }

    #[test]
    fn a_resumed_world_runs_again() {
        let runtime = counting_runtime();
        report_lifecycle(&runtime, AppLifecycle::Running);
        report_lifecycle(&runtime, AppLifecycle::WillSuspend);
        update(&runtime);
        update(&runtime);
        let suspended = turns(&runtime);

        report_lifecycle(&runtime, AppLifecycle::Running);
        update(&runtime);
        assert_eq!(turns(&runtime), suspended + 1);
    }

    #[test]
    fn a_world_that_has_exited_stops_running_systems() {
        let runtime = counting_runtime();
        report_lifecycle(&runtime, AppLifecycle::Running);
        update(&runtime);
        runtime
            .borrow_mut()
            .app
            .world_mut()
            .write_message(AppExit::Success);
        update(&runtime);
        let exited = turns(&runtime);

        update(&runtime);
        assert_eq!(
            exited,
            turns(&runtime),
            "an exited world must not run again"
        );
    }
}
