use bevy_ecs::prelude::{Event, On};

#[derive(Event)]
pub struct Pulse;

pub fn public(_: On<Pulse>) {}

struct App;

impl App {
    fn add_observer(&mut self, _: fn(On<Pulse>)) {}
}

struct World;

impl World {
    fn add_observer(&mut self, _: fn(On<Pulse>)) {}
}

struct EntityCommands;

impl EntityCommands {
    fn observe(&mut self, _: fn(On<Pulse>)) {}
}

struct Observer;

impl Observer {
    fn new(_: fn(On<Pulse>)) -> Self {
        Self
    }
}

fn main() {
    App.add_observer(public);
    World.add_observer(public);
    EntityCommands.observe(public);
    let _ = Observer::new(public);
}
