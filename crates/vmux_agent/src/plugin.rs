use bevy::prelude::*;

use crate::claude::ClaudeStrategy;
use crate::codex::CodexStrategy;
use crate::session::{
    self, AgentSessionDirty, AgentSessionExited, AgentSessionToEntity,
    agent_session_dirty_run_condition,
};
use crate::strategy::AgentStrategies;
use crate::vibe::VibeStrategy;

pub struct AgentSessionPlugin;

impl Plugin for AgentSessionPlugin {
    fn build(&self, app: &mut App) {
        let mut strategies = AgentStrategies::default();
        strategies.register_cli(Box::new(VibeStrategy));
        strategies.register_cli(Box::new(ClaudeStrategy));
        strategies.register_cli(Box::new(CodexStrategy));
        app.insert_resource(strategies)
            .init_resource::<AgentSessionToEntity>()
            .init_resource::<AgentSessionDirty>()
            .add_message::<AgentSessionExited>()
            .add_systems(Startup, session::start_agent_session_watchers)
            .add_systems(
                Update,
                (
                    session::track_session_id_inserts,
                    session::track_session_id_removals,
                )
                    .chain(),
            )
            .add_systems(
                Update,
                (
                    session::mark_dirty_on_fs_change,
                    session::mark_dirty_on_pending_added,
                ),
            )
            .add_systems(
                Update,
                (
                    session::discover_pending_agent_sessions,
                    session::detect_file_end_time_exit,
                    session::clear_agent_session_dirty,
                )
                    .chain()
                    .after(session::mark_dirty_on_fs_change)
                    .after(session::mark_dirty_on_pending_added)
                    .run_if(agent_session_dirty_run_condition),
            )
            .add_systems(
                Update,
                session::format_agent_url.after(session::track_session_id_inserts),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_registers_three_strategies() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AgentSessionPlugin));
        let strategies = app.world().resource::<AgentStrategies>();
        assert!(strategies.get_cli(crate::AgentKind::Vibe).is_some());
        assert!(strategies.get_cli(crate::AgentKind::Claude).is_some());
        assert!(strategies.get_cli(crate::AgentKind::Codex).is_some());
    }
}
