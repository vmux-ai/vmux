use bevy_app::{App, Plugin, Update};
use bevy_ecs::schedule::IntoScheduleConfigs;
use vmux_ecs::{JobPlugin, JobSet};

mod fs;
mod http;
mod temporary;

pub use crate::process::Execute;
pub use fs::{
    DirectoryEntries, DirectoryEntry, ExecutableFacts, FileBytes, FindExecutable, InspectLink,
    LinkFacts, ReadDirectory, ReadFile, WriteFile,
};
pub use http::{Authorization, Get};
pub use temporary::{CloseTemporary, CreateTemporary, TemporaryDirectory, TemporaryTransfer};

pub struct ObservationIoPlugin;

impl Plugin for ObservationIoPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            JobPlugin::<ReadFile>::default(),
            JobPlugin::<ReadDirectory>::default(),
            JobPlugin::<WriteFile>::default(),
            JobPlugin::<Get>::default(),
            JobPlugin::<Execute>::default(),
            JobPlugin::<InspectLink>::default(),
            JobPlugin::<FindExecutable>::default(),
        ))
        .configure_sets(
            Update,
            crate::observation::ObservationSet.in_set(crate::ToolTaskSet::Start),
        )
        .configure_sets(Update, JobSet::Submit.after(crate::ToolTaskSet::Start));
    }
}
