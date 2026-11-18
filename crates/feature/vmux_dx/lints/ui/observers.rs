use bevy_app::App;
use bevy_ecs::observer::Observer;
use bevy_ecs::prelude::{Entity, EntityEvent, On};
use bevy_ecs::world::World;
use handlers::public as aliased;

#[derive(EntityEvent)]
pub struct Pulse {
    pub entity: Entity,
}

mod handlers {
    use super::*;

    pub fn public(_: On<Pulse>) {}
    pub(crate) fn crate_visible(_: On<Pulse>) {}
    pub(super) fn parent_visible(_: On<Pulse>) {}
    pub fn world_visible(_: On<Pulse>) {}
    pub fn entity_visible(_: On<Pulse>) {}
    pub fn commands_visible(_: On<Pulse>) {}
    pub fn constructed(_: On<Pulse>) {}
    fn private(_: On<Pulse>) {}

    pub fn install(app: &mut App) {
        app.add_observer(private);
    }
}

fn main() {
    let mut app = App::new();
    let configured = aliased;
    app.add_observer(configured);
    App::add_observer(&mut app, handlers::crate_visible);
    app.main_mut().add_observer(handlers::parent_visible);
    World::add_observer(app.world_mut(), handlers::world_visible);
    app.world_mut()
        .spawn_empty()
        .observe(handlers::entity_visible);
    app.world_mut()
        .commands()
        .spawn_empty()
        .observe(handlers::commands_visible);
    let observer = Observer::new(handlers::constructed);
    app.world_mut().spawn(observer);
    handlers::install(&mut app);
}
