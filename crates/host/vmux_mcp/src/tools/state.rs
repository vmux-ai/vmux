use super::{DispatchTarget, ToolCall, ToolManifest};
use bevy_app::{App, Plugin};
use vmux_client::protocol::{AgentCommand, AgentQuery};

pub(super) struct StateToolsPlugin;

impl Plugin for StateToolsPlugin {
    fn build(&self, app: &mut App) {
        let mut tools = ToolManifest::from_ron(include_str!("state.ron"));
        tools.local(app, "read_layout", read_layout);
        tools.local(app, "update_layout", update_layout);
        tools.local(app, "get_settings", get_settings);
        tools.local(app, "list_spaces", list_spaces);
        tools.finish();
    }
}

pub(super) fn read_layout(call: &ToolCall) -> Result<DispatchTarget, String> {
    Ok(DispatchTarget::Query(AgentQuery::ReadLayout {
        anchor: call.anchor,
    }))
}

pub(super) fn update_layout(call: &ToolCall) -> Result<DispatchTarget, String> {
    let layout = serde_json::from_value(call.arguments.clone())
        .map_err(|error| format!("update_layout: invalid layout payload: {error}"))?;
    Ok(DispatchTarget::Command(AgentCommand::UpdateLayout {
        layout,
    }))
}

pub(super) fn get_settings(_call: &ToolCall) -> Result<DispatchTarget, String> {
    Ok(DispatchTarget::Query(AgentQuery::GetSettings))
}

pub(super) fn list_spaces(_call: &ToolCall) -> Result<DispatchTarget, String> {
    Ok(DispatchTarget::Query(AgentQuery::ListSpaces))
}
