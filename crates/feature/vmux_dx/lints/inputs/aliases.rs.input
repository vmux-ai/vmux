use bevy_app::{App, Update};
use bevy_ecs::schedule::IntoScheduleConfigs;

pub fn exported() {}

fn main() {
    let system = exported;
    let configured = system.run_if(|| true);
    App::new().add_systems(Update, configured);
}
