use super::{ProtocolTool, ToolManifest};
use bevy_app::{App, Plugin};

pub(super) struct FileToolsPlugin;

impl Plugin for FileToolsPlugin {
    fn build(&self, app: &mut App) {
        let mut tools = ToolManifest::from_ron(include_str!("files.ron"));
        tools.protocol(app, "read_file", ProtocolTool::ReadFile);
        tools.protocol(app, "grep", ProtocolTool::Grep);
        tools.finish();
    }
}
