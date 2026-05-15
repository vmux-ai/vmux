use bevy::prelude::*;

use crate::components::{AgentApprovalPolicy, AgentMessages, AgentSession};
use crate::echo::EchoAppStrategy;
use crate::strategy::AgentStrategies;
use crate::systems::{approval, dispatch_tool, drain_stream, process_input};

pub struct AppAgentPlugin;

impl Plugin for AppAgentPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<AgentSession>()
            .register_type::<AgentMessages>()
            .register_type::<AgentApprovalPolicy>()
            .add_observer(approval::handle_approval_reply)
            .add_systems(
                Update,
                (
                    process_input::process_user_input,
                    drain_stream::drain_stream,
                    dispatch_tool::dispatch_tool,
                ),
            );

        if app.world().get_resource::<AgentStrategies>().is_none() {
            app.insert_resource(AgentStrategies::default());
        }
        let mut strategies = app.world_mut().resource_mut::<AgentStrategies>();
        strategies.register_app(Box::new(EchoAppStrategy));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_builds_without_panic() {
        let mut app = App::new();
        app.add_plugins(bevy::app::TaskPoolPlugin::default());
        app.add_plugins(AppAgentPlugin);
        app.update();
    }
}
