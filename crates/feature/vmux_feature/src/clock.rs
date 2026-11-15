use std::time::{SystemTime, UNIX_EPOCH};

use bevy_app::{App, Update};
use bevy_ecs::prelude::*;

use crate::{Set, Tick};

pub struct ClockPlugin;

impl bevy_app::Plugin for ClockPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, sample.before(Set::Clock));
    }
}

fn sample(mut ticks: MessageWriter<Tick>) {
    let Ok(elapsed) = SystemTime::now().duration_since(UNIX_EPOCH) else {
        return;
    };
    ticks.write(Tick(elapsed.as_secs()));
}
