use bevy_app::{App as BevyApp, Update};
use bevy_ecs::schedule::IntoScheduleConfigs;
use systems::public as renamed;

mod systems {
    pub fn public() {}

    pub(crate) fn crate_visible() {}

    pub(super) fn parent_visible() {}
}

fn main() {
    BevyApp::new().add_systems(
        Update,
        (renamed, systems::crate_visible, systems::parent_visible).chain(),
    );
}
