use std::cell::RefCell;
use std::collections::HashMap;

use bevy_app::{App, PluginsState};
use bevy_ecs::change_detection::DetectChangesMut;
use bevy_ecs::component::Mutable;
use bevy_ecs::message::{Message, Messages};
use bevy_ecs::resource::Resource;
use bevy_window::AppLifecycle;
use vmux_ui::hooks::transport::BytesListener;
use vmux_wire::page::PageEmit;

thread_local! {
    static REPORTED: RefCell<Vec<AppLifecycle>> = const { RefCell::new(Vec::new()) };

    static INSTALLED: RefCell<Option<World>> = const { RefCell::new(None) };
}

pub struct World {
    app: App,
    lifecycle: AppLifecycle,
    listeners: HashMap<&'static str, BytesListener>,
    finished: bool,
}

impl World {
    pub fn new(plugins: impl FnOnce(&mut App)) -> Self {
        let mut app = App::new();
        app.add_message::<AppLifecycle>().add_message::<PageEmit>();
        plugins(&mut app);
        while app.plugins_state() == PluginsState::Adding {
            bevy_tasks::tick_global_task_pools_on_main_thread();
        }
        app.finish();
        app.cleanup();
        Self {
            app,
            lifecycle: AppLifecycle::Idle,
            listeners: HashMap::new(),
            finished: false,
        }
    }

    pub fn install(self) {
        INSTALLED.with_borrow_mut(|slot| *slot = Some(self));
    }

    pub fn with<R>(act: impl FnOnce(&mut World) -> R) -> Option<R> {
        INSTALLED
            .try_with(|slot| match slot.try_borrow_mut() {
                Ok(mut slot) => slot.as_mut().map(act),
                Err(_) => {
                    tracing::error!("world: re-entered while running, this turn is dropped");
                    None
                }
            })
            .ok()
            .flatten()
    }

    pub fn insert<R: Resource + PartialEq>(&mut self, resource: R) {
        if self.app.world().get_resource::<R>() == Some(&resource) {
            return;
        }
        self.app.insert_resource(resource);
    }

    pub fn send<M: Message>(&mut self, message: M) {
        self.app.world_mut().write_message(message);
    }

    pub fn refresh<R: Resource<Mutability = Mutable>>(&mut self) {
        if let Some(mut resource) = self.app.world_mut().get_resource_mut::<R>() {
            resource.set_changed();
        }
    }

    pub fn listen(&mut self, id: &'static str, on_bytes: BytesListener) {
        self.listeners.insert(id, on_bytes);
    }

    pub fn report(lifecycle: AppLifecycle) {
        REPORTED.with_borrow_mut(|reported| reported.push(lifecycle));
    }

    pub fn tick(&mut self) {
        if self.finished {
            return;
        }
        self.drain_reported();
        if !self.is_active() {
            return;
        }
        self.app.update();
        self.deliver();
        if self.lifecycle == AppLifecycle::WillSuspend {
            self.lifecycle = AppLifecycle::Suspended;
        }
        if self.app.should_exit().is_some() {
            self.finished = true;
        }
    }

    fn deliver(&mut self) {
        let emitted = self
            .app
            .world_mut()
            .resource_mut::<Messages<PageEmit>>()
            .drain()
            .collect::<Vec<_>>();
        for emit in emitted {
            let Some(listener) = self.listeners.get_mut(emit.id) else {
                tracing::debug!(id = emit.id, "page emit had no listener");
                continue;
            };
            listener(&emit.bytes);
        }
    }

    fn is_active(&self) -> bool {
        matches!(
            self.lifecycle,
            AppLifecycle::Running | AppLifecycle::WillSuspend | AppLifecycle::WillResume
        )
    }

    fn drain_reported(&mut self) {
        let reported = REPORTED.with_borrow_mut(std::mem::take);
        for lifecycle in reported {
            self.lifecycle = lifecycle;
            self.app.world_mut().write_message(lifecycle);
        }
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

    impl World {
        fn counting() -> Self {
            REPORTED.with_borrow_mut(Vec::clear);
            Self::new(|app| {
                app.init_resource::<Turns>()
                    .add_systems(Update, |mut turns: ResMut<Turns>| turns.0 += 1);
            })
        }

        fn turns(&self) -> usize {
            self.app.world().resource::<Turns>().0
        }
    }

    #[test]
    fn an_idle_world_does_not_run_until_it_is_told_the_app_is_running() {
        let mut world = World::counting();
        world.tick();
        assert_eq!(world.turns(), 0, "a world nobody has resumed must not run");

        World::report(AppLifecycle::Running);
        world.tick();
        assert_eq!(world.turns(), 1);
    }

    #[test]
    fn suspending_owes_exactly_one_more_turn_and_then_stops() {
        let mut world = World::counting();
        World::report(AppLifecycle::Running);
        world.tick();

        World::report(AppLifecycle::WillSuspend);
        world.tick();
        let owed = world.turns();
        assert_eq!(owed, 2, "WillSuspend is owed the frame a plugin saves from");

        for _ in 0..5 {
            world.tick();
        }
        assert_eq!(owed, world.turns(), "a suspended world must not run");
    }

    #[test]
    fn a_resumed_world_runs_again() {
        let mut world = World::counting();
        World::report(AppLifecycle::Running);
        World::report(AppLifecycle::WillSuspend);
        world.tick();
        world.tick();
        let suspended = world.turns();

        World::report(AppLifecycle::Running);
        world.tick();
        assert_eq!(world.turns(), suspended + 1);
    }

    #[test]
    fn a_world_that_has_exited_stops_running_systems() {
        let mut world = World::counting();
        World::report(AppLifecycle::Running);
        world.tick();
        world.app.world_mut().write_message(AppExit::Success);
        world.tick();
        let exited = world.turns();

        world.tick();
        assert_eq!(exited, world.turns(), "an exited world must not run again");
    }
}
