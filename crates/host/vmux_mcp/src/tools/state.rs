use super::{DispatchTarget, ToolCall, ToolManifest};
use bevy_app::{App, Plugin};
use bevy_ecs::prelude::{Commands, On};
use vmux_client::protocol::{AgentCommand, AgentQuery};

pub(super) struct StateToolsPlugin;

impl Plugin for StateToolsPlugin {
    fn build(&self, app: &mut App) {
        let mut tools = ToolManifest::from_ron(include_str!("state.ron"));
        tools.observe(app, "read_layout", read_layout);
        tools.observe(app, "update_layout", update_layout);
        tools.observe(app, "get_settings", get_settings);
        tools.observe(app, "list_spaces", list_spaces);
        tools.finish();
    }
}

fn read_layout(trigger: On<ToolCall>, mut commands: Commands) {
    trigger.finish_dispatch(
        &mut commands,
        Ok(DispatchTarget::Query(AgentQuery::ReadLayout {
            anchor: trigger.anchor,
        })),
    );
}

fn update_layout(trigger: On<ToolCall>, mut commands: Commands) {
    fn target(call: &ToolCall) -> Result<DispatchTarget, String> {
        let layout = serde_json::from_value(call.arguments.clone())
            .map_err(|error| format!("update_layout: invalid layout payload: {error}"))?;
        Ok(DispatchTarget::Command(AgentCommand::UpdateLayout {
            layout,
        }))
    }

    trigger.finish_dispatch(&mut commands, target(&trigger));
}

fn get_settings(trigger: On<ToolCall>, mut commands: Commands) {
    trigger.finish_dispatch(
        &mut commands,
        Ok(DispatchTarget::Query(AgentQuery::GetSettings)),
    );
}

fn list_spaces(trigger: On<ToolCall>, mut commands: Commands) {
    trigger.finish_dispatch(
        &mut commands,
        Ok(DispatchTarget::Query(AgentQuery::ListSpaces)),
    );
}
