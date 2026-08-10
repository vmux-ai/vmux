//! Which agent CLIs this machine actually has.
//!
//! One provider entity per built-in [`AgentKind`], marked [`Ready`] when its executable resolves
//! on `PATH`. [`AgentExecutableOverride`] lets a test pin that answer rather than depend on what
//! happens to be installed.

use std::path::PathBuf;

use bevy::prelude::*;
use vmux_core::Ready;
use vmux_core::agent::{AgentKind, AgentProviderTargetKind};

pub(super) struct ProviderPlugin;

impl Plugin for ProviderPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Startup,
            (
                spawn_builtin_agent_providers,
                detect_agent_provider_availability,
            )
                .chain(),
        );
    }
}

pub(crate) const BUILTIN_AGENT_PROVIDERS: &[AgentKind] =
    &[AgentKind::Vibe, AgentKind::Claude, AgentKind::Codex];

/// Per-[`AgentKind`] override for CLI executable resolution: `true` forces present, `false` forces
/// missing, absent falls back to a real `PATH` lookup. Lets tests drive the spawn/setup-page flow
/// without depending on which CLIs are installed on the host.
#[derive(Resource, Clone, Default)]
pub struct AgentExecutableOverride(pub std::collections::HashMap<AgentKind, bool>);

pub(crate) fn resolve_agent_executable(
    kind: AgentKind,
    override_: Option<&AgentExecutableOverride>,
) -> Option<PathBuf> {
    if let Some(forced) = override_.and_then(|o| o.0.get(&kind).copied()) {
        return forced.then(|| PathBuf::from(kind.executable()));
    }
    crate::exec::find_executable(kind.executable())
}

fn spawn_builtin_agent_providers(mut commands: Commands) {
    for kind in BUILTIN_AGENT_PROVIDERS {
        commands.spawn((
            AgentProviderTargetKind(*kind),
            Name::new(kind.display_name()),
        ));
    }
}

fn detect_agent_provider_availability(
    mut commands: Commands,
    q: Query<(Entity, &AgentProviderTargetKind), Without<Ready>>,
) {
    for (entity, kind) in &q {
        if crate::exec::find_executable(kind.0.executable()).is_some() {
            commands.entity(entity).insert(Ready);
        }
    }
}
