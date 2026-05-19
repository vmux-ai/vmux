use std::path::PathBuf;

use bevy::prelude::*;
use vmux_page::{PageConfig, PageRegistry};

use crate::command::{AppCommand, ReadAppCommands, WriteAppCommands};

pub struct CommandPlugin;

impl Plugin for CommandPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PageRegistry>()
            .add_message::<AppCommand>()
            .configure_sets(Update, ReadAppCommands.after(WriteAppCommands));

        app.world_mut().resource_mut::<PageRegistry>().register(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")),
            &PageConfig::with_custom_host("command-bar"),
        );
    }
}
