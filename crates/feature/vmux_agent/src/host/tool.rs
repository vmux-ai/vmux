use bevy::prelude::*;
use serde::Deserialize;
use vmux_api::protocol::{AgentRequest, AgentResumeInAcp};
use vmux_core::ProcessAnchor;
use vmux_tool::{AddedTool, ToolAppExt, ToolCommand, ToolDispatchSet, ToolManifestPlugin};

pub struct AgentToolPlugin;

impl Plugin for AgentToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ToolManifestPlugin::from_feature(
            include_str!("../feature.ron"),
            "default",
        ))
        .register_tool::<ResumeInAcpArgs>("resume_in_acp")
        .add_systems(Update, resume_in_acp.in_set(ToolDispatchSet));
    }
}

#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct ResumeInAcpArgs {}

fn resume_in_acp(
    mut commands: Commands,
    calls: Query<(Entity, &Name, Option<&ProcessAnchor>), AddedTool<ResumeInAcpArgs>>,
) {
    for (request, name, anchor) in &calls {
        let result = ProcessAnchor::required(anchor, name.as_str())
            .and_then(|anchor| AgentRequest::encode(&AgentResumeInAcp { anchor }));
        commands.entity(request).insert(ToolCommand(result));
    }
}
