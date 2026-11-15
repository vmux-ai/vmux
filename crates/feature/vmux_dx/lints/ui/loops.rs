use bevy_app::{App, Update};
use bevy_ecs::schedule::{IntoScheduleConfigs, ScheduleConfigs};
use bevy_ecs::system::ScheduleSystem;

pub fn carried() {}

pub fn continued() {}

pub fn from_loop() {}

pub fn unused() {}

fn private() {}

fn main() {
    let mut app = App::new();
    let mut system: ScheduleConfigs<ScheduleSystem> = private.into_configs();
    for _ in 0..2 {
        app.add_systems(Update, system);
        system = carried.into_configs();
    }
    let mut system: ScheduleConfigs<ScheduleSystem> = unused.into_configs();
    loop {
        system = private.into_configs();
        break;
    }
    app.add_systems(Update, system);
    let mut system: ScheduleConfigs<ScheduleSystem> = private.into_configs();
    for index in 0..2 {
        if index == 0 {
            system = continued.into_configs();
            continue;
        }
        app.add_systems(Update, system);
        system = private.into_configs();
    }
    let system: ScheduleConfigs<ScheduleSystem> = loop {
        break from_loop.into_configs();
    };
    app.add_systems(Update, system);
    let mut system: ScheduleConfigs<ScheduleSystem> = unused.into_configs();
    'outer: loop {
        loop {
            system = private.into_configs();
            break 'outer;
        }
    }
    app.add_systems(Update, system);
    let mut system: ScheduleConfigs<ScheduleSystem> = private.into_configs();
    loop {
        system = system;
        if std::hint::black_box(true) {
            break;
        }
    }
    app.add_systems(Update, system);
    let mut system: ScheduleConfigs<ScheduleSystem> = private.into_configs();
    if std::hint::black_box(true) {
        system = unused.into_configs();
        return;
    }
    app.add_systems(Update, system);
}
