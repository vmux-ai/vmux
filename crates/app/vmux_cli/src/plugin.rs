use std::io;

use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use vmux_ecs::cli::{CliInvocation, CliResult};
use vmux_ecs::manifest::{FeatureManifestSource, FeaturePlugin};

use crate::browser::Open;
use crate::command::CliRuntimePlugin;

struct Feature;

impl FeatureManifestSource for Feature {
    const SOURCE: &'static str = include_str!("feature.ron");
}

pub struct CliPlugin;

impl Plugin for CliPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(feature = "dx")]
        app.add_plugins(vmux_dx::Plugin);
        app.add_plugins((
            FeaturePlugin::<Feature>::default(),
            vmux_app::VmuxPlugin::builder().cli().build(),
            CliRuntimePlugin,
        ))
        .add_systems(Update, (open, browse));
    }
}

fn open(
    invocations: Query<(Entity, &CliInvocation), Added<CliInvocation>>,
    mut commands: Commands,
) {
    for (entity, invocation) in &invocations {
        if invocation.is("app.open") {
            commands.entity(entity).insert(CliResult::from(launch()));
        }
    }
}

fn browse(
    invocations: Query<(Entity, &CliInvocation), Added<CliInvocation>>,
    mut commands: Commands,
) {
    for (entity, invocation) in &invocations {
        if !invocation.is("browser.open") {
            continue;
        }
        let result = Open::try_from(invocation)
            .and_then(|request| request.send(&vmux_ecs::profile::ServicePaths::current().socket()));
        commands.entity(entity).insert(CliResult(result));
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
