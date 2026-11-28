use bevy_app::{App, Plugin, Update};
use bevy_ecs::schedule::IntoScheduleConfigs;
use vmux_job::{JobPlugin, JobSet};

pub use crate::process::Execute;
pub use files::{
    BytesCopied, CopyFile, CreateDirectory, DirectoryCreated, FileIdentity, MovePath, ObservePath,
    PathFacts,
};
pub use fs::{
    DirectoryEntries, DirectoryEntry, ExecutableFacts, FileBytes, FindExecutable, InspectLink,
    LinkFacts, ReadDirectory, ReadFile, WriteFile,
};
pub use http::{Authorization, Get};
pub use links::{CleanupPath, LinkPath, PathChanged, PathReceipt, UnlinkPath};
pub use temporary::{CloseTemporary, CreateTemporary, TemporaryDirectory, TemporaryTransfer};

mod files;
pub(crate) mod fs;
mod http;
mod links;
mod temporary;

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
            JobPlugin::<LinkPath>::default(),
            JobPlugin::<UnlinkPath>::default(),
            JobPlugin::<ObservePath>::default(),
            JobPlugin::<MovePath>::default(),
            JobPlugin::<CopyFile>::default(),
            JobPlugin::<CreateDirectory>::default(),
            JobPlugin::<CleanupPath>::default(),
        ))
        .configure_sets(
            Update,
            crate::observation::ObservationSet.in_set(crate::ToolTaskSet::Start),
        )
        .configure_sets(Update, JobSet::Submit.after(crate::ToolTaskSet::Start));
    }
}
