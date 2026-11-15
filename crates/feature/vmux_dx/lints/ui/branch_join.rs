use bevy_app::{App, Update};
use bevy_ecs::schedule::{IntoScheduleConfigs, ScheduleConfigs};
use bevy_ecs::system::ScheduleSystem;

pub fn exported() {}

fn private() {}

fn main() {
    let mut system: ScheduleConfigs<ScheduleSystem> = exported.into_configs();
    if std::hint::black_box(true) {
        system = private.into_configs();
    } else {
        system = private.into_configs();
    }
    App::new().add_systems(Update, system);
}
