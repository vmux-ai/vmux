use bevy::prelude::*;
use bevy_cef::prelude::BinEventEmitterPlugin;

use crate::components::{AgentApprovalPolicy, AgentMessages, AgentSession};
use crate::run_state_kind::LastRunStateKind;
use crate::strategy::AgentStrategies;
use crate::systems::{
    approval, continue_after_tool, dispatch_tool, drain_stream, process_input, surface_errors,
};
use crate::toast::AgentToast;

pub struct PageAgentPlugin;

impl Plugin for PageAgentPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<AgentSession>()
            .register_type::<AgentMessages>()
            .register_type::<AgentApprovalPolicy>()
            .add_message::<AgentToast>()
            .add_plugins(BinEventEmitterPlugin::<(AgentToast,)>::with_id(
                "vmux-agent-toast",
            ))
            .add_observer(approval::handle_approval_reply)
            .add_systems(
                Update,
                (
                    process_input::process_user_input,
                    drain_stream::drain_stream,
                    dispatch_tool::dispatch_tool,
                    continue_after_tool::continue_after_tool,
                    surface_errors::surface_errors,
                    attach_last_run_state_kind,
                ),
            );

        if app.world().get_resource::<AgentStrategies>().is_none() {
            app.insert_resource(AgentStrategies::default());
        }
    }
}

fn attach_last_run_state_kind(
    mut commands: Commands,
    q: Query<Entity, (With<AgentSession>, Without<LastRunStateKind>)>,
) {
    for entity in &q {
        commands.entity(entity).insert(LastRunStateKind::default());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_cef::prelude::BinIpcEventRawBuffer;

    #[test]
    fn plugin_builds_without_panic() {
        let mut app = App::new();
        app.add_plugins(bevy::app::TaskPoolPlugin::default());
        app.init_resource::<BinIpcEventRawBuffer>();
        app.add_plugins(PageAgentPlugin);
        app.update();
    }
}
