use std::collections::BTreeSet;

use bevy_ecs::prelude::*;
use vmux_core::cli::{CliCommandManifest, CliManifest};

use super::CliArguments;

#[derive(Component)]
pub(super) struct CliCatalog {
    pub(super) default: String,
    pub(super) commands: Vec<CliCommandManifest>,
}

pub(super) fn collect_cli_catalog(
    processes: Query<Entity, (With<CliArguments>, Without<CliCatalog>)>,
    manifests: Query<&CliManifest>,
    mut commands: Commands,
) {
    let mut default = None;
    let mut registered = Vec::new();
    for manifest in &manifests {
        if let Some(candidate) = &manifest.default {
            assert!(
                default.is_none(),
                "only one default CLI command may be registered"
            );
            default = Some(candidate.clone());
        }
        registered.extend(manifest.commands.iter().cloned());
    }
    registered.sort_by(|left, right| left.name.cmp(&right.name));
    let mut names = BTreeSet::new();
    for command in &registered {
        assert!(
            names.insert(command.name.clone()),
            "duplicate CLI command: {}",
            command.name
        );
    }
    let default = default.expect("one default CLI command must be registered");
    for entity in &processes {
        commands.entity(entity).insert(CliCatalog {
            default: default.clone(),
            commands: registered.clone(),
        });
    }
}
