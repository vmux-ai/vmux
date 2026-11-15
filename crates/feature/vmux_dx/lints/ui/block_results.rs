use bevy_app::{App, Update};
use bevy_ecs::schedule::IntoScheduleConfigs;

pub fn exported_block() {}

pub fn exported_if() {}

pub fn exported_match() {}

pub fn condition() -> bool {
    std::hint::black_box(true)
}

fn private() {}

fn main() {
    let mut app = App::new();
    app.add_systems(Update, { exported_block.into_configs() });
    app.add_systems(
        Update,
        if condition() {
            exported_if.into_configs()
        } else {
            private.into_configs()
        },
    );
    app.add_systems(
        Update,
        match condition() {
            true => exported_match.into_configs(),
            false => private.into_configs(),
        },
    );
}
