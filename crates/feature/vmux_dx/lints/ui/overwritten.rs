use bevy_app::{App, Update};
use bevy_ecs::schedule::{IntoScheduleConfigs, ScheduleConfigs};
use bevy_ecs::system::ScheduleSystem;

pub fn exported() {}

fn private() {}

fn main() {
    let mut system: ScheduleConfigs<ScheduleSystem> = exported.into_configs();
    system = private.into_configs();
    App::new().add_systems(Update, system);
}
