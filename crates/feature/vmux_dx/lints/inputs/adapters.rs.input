use bevy_app::{App, Update};
use bevy_ecs::prelude::*;

pub fn source() -> u32 {
    1
}

pub fn sink(_: In<u32>) {}

pub fn mapped() -> u32 {
    2
}

fn main() {
    App::new().add_systems(Update, (source.pipe(sink), mapped.map(drop)));
}
