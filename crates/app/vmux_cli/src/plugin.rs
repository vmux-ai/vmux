use std::io;

use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use vmux_ecs::cli::{CliInvocation, CliResult};
use vmux_ecs::host::manifest::{FeatureManifestSource, FeaturePlugin};

use crate::command::CliRuntimePlugin;

struct Feature;

impl FeatureManifestSource for Feature {
    const SOURCE: &'static str = include_str!("feature.ron");
}

pub struct CliPlugin;

impl Plugin for CliPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            FeaturePlugin::<Feature>::default(),
            vmux_app::VmuxToolPlugin,
            CliRuntimePlugin,
        ))
        .add_systems(Update, open);
    }
}

fn open(
    invocations: Query<(Entity, &CliInvocation), Added<CliInvocation>>,
    mut commands: Commands,
) {
    for (entity, invocation) in &invocations {
        if invocation.is("app.open") {
            commands
                .entity(entity)
                .insert(CliResult::from_unit(launch()));
        }
    }
}

fn launch() -> io::Result<()> {
    #[cfg(target_os = "macos")]
    {
        let status = std::process::Command::new("open")
            .arg("-a")
            .arg("Vmux")
            .status()?;
        if status.success() {
            return Ok(());
        }
        Err(io::Error::other(format!(
            "open -a Vmux exited with {status}"
        )))
    }

    #[cfg(not(target_os = "macos"))]
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "launching the Vmux app is not supported on this platform yet",
    ))
}
