#![deny(vmux_ecs_ownership)]

use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use std::collections::HashMap;

struct Row {
    id: u32,
}

struct Graph {
    rows: Vec<Row>,
}

#[derive(Component)]
struct State {
    graph: Graph,
}

#[derive(Resource)]
struct Registry {
    rows: HashMap<String, Row>,
}

trait Engine: Send + Sync {}

#[derive(Resource)]
struct Dynamic {
    engine: Box<dyn Engine>,
}

#[derive(Component)]
struct Child(u32);

#[derive(Resource)]
struct Embedded {
    child: Child,
}

fn read(_: Query<&State>, _: Res<Registry>, _: Res<Dynamic>, _: Res<Embedded>, _: Query<&Child>) {}

struct OwnershipPlugin;

impl Plugin for OwnershipPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, read);
    }
}

fn main() {
    App::new().add_plugins(OwnershipPlugin);
}
