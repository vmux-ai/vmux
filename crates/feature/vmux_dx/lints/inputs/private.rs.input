use bevy_app::{App, Update};
use bevy_ecs::schedule::IntoScheduleConfigs;

fn first() {}

fn second() {}

fn condition() -> bool {
    true
}

fn main() {
    let pair = (first, second);
    App::new().add_systems(Update, pair.chain().run_if(condition));
}
