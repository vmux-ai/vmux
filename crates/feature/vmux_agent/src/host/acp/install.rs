use bevy::prelude::*;

pub(super) struct InstallPlugin;

impl Plugin for InstallPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<super::AcpPackageChanged>()
            .add_observer(super::cancel_install_on_remove)
            .add_systems(
                Update,
                (super::start_installs, super::poll_installs).chain(),
            );
    }
}
