use bevy::prelude::*;
use vmux_ecs::profile::CurrentProfile;

pub(super) use vmux_session::preferences::{
    AgentPreferences, Kind, ModelSelectionSet, ProfileRequested, RememberMode, RememberModel,
    RememberModels, RememberModes,
};

pub(super) struct Plugin;

impl bevy::prelude::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ProfileRequested>()
            .add_systems(
                Startup,
                (counter, profile)
                    .chain()
                    .before(vmux_session::preferences::ProfileSet),
            )
            .add_systems(
                Update,
                profile.before(vmux_session::preferences::ProfileSet),
            );
    }
}

#[derive(Component, Default)]
pub(super) struct AcpSessionConfigRequestCounter(pub(super) u64);

fn counter(existing: Query<(), With<AcpSessionConfigRequestCounter>>, mut commands: Commands) {
    if existing.is_empty() {
        commands.spawn((
            Name::new("ACP configuration requests"),
            AcpSessionConfigRequestCounter::default(),
        ));
    }
}

fn profile(
    current: CurrentProfile,
    preferences: AgentPreferences,
    mut requests: MessageWriter<ProfileRequested>,
) {
    let Some(paths) = current.paths() else {
        return;
    };
    if preferences.profile_matches(&paths.profile()) {
        return;
    }
    let directory = vmux_path::PathIdentity::resolve(paths.profile()).into_path_buf();
    if !preferences.profile_matches(&directory) {
        requests.write(ProfileRequested(directory));
    }
}
