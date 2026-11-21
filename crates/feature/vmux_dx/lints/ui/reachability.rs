#![deny(vmux_ecs_ownership)]

use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;

#[derive(Component)]
struct State(u32);

fn read<T: Component>(_: Query<&T>) {}

fn install(app: &mut App) {
    app.add_systems(Update, read::<State>);
}

struct OwnershipPlugin;

impl Plugin for OwnershipPlugin {
    fn build(&self, app: &mut App) {
        install(app);
    }
}

mod orphan {
    use super::*;

    #[derive(Component)]
    struct Uncalled(u32);

    fn read(_: Query<&Uncalled>) {}

    fn install(app: &mut App) {
        app.add_systems(Update, read);
        install(app);
    }
}

fn main() {
    App::new().add_plugins(OwnershipPlugin);
}
