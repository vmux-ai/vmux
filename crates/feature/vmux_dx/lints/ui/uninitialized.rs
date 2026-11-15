use bevy_app::{App, Update};
use bevy_ecs::schedule::IntoScheduleConfigs;

pub fn exported() {}

fn main() {
    let system;
    system = exported.into_configs();
    App::new().add_systems(Update, system);
}
