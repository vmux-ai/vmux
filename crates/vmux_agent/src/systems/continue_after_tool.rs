use bevy::prelude::*;
use bevy::tasks::IoTaskPool;
use crossbeam_channel::unbounded;

use crate::components::{AgentMessages, AgentSession};
use crate::http::drive_sse;
use crate::message::Message;
use crate::run_state::AgentRunState;
use crate::strategy::AgentStrategies;
use crate::stream::StreamEvent;
use crate::tools::mcp_tool_defs;

pub fn continue_after_tool(
    strategies: Res<AgentStrategies>,
    mut q: Query<(&mut AgentRunState, &AgentMessages, &AgentSession)>,
) {
    for (mut state, messages, session) in &mut q {
        if !matches!(*state, AgentRunState::Idle) {
            continue;
        }
        if !matches!(messages.0.last(), Some(Message::ToolResult { .. })) {
            continue;
        }
        let Some(strategy) =
            strategies.get_app_by_provider_model(&session.provider, &session.model)
        else {
            *state = AgentRunState::Errored(format!(
                "No registered App strategy for {}/{}",
                session.provider, session.model
            ));
            continue;
        };
        let env_var = strategy.env_var();
        let api_key = if env_var.is_empty() {
            String::new()
        } else {
            match std::env::var(env_var) {
                Ok(k) => k,
                Err(_) => {
                    *state = AgentRunState::Errored(format!("Missing {env_var}"));
                    continue;
                }
            }
        };
        let tools = mcp_tool_defs();
        let request = strategy.build_request(&session.model, &messages.0, &tools, &api_key);
        let (tx, rx) = unbounded::<StreamEvent>();
        let strat_arc = strategy.clone();
        let task = IoTaskPool::get().spawn(async move {
            drive_sse(request, strat_arc, tx).await;
        });
        *state = AgentRunState::Streaming {
            rx,
            _task: task,
            partial: None,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::AppAgentStrategy;
    use crate::strategy::AgentStrategy;
    use crate::stream::ToolDef;
    use crate::{AgentKind, AgentVariant};
    use std::sync::Arc;

    struct MockAppStrategy;
    impl AgentStrategy for MockAppStrategy {
        fn kind(&self) -> AgentKind {
            AgentKind::Vibe
        }
        fn variant(&self) -> AgentVariant {
            AgentVariant::App
        }
    }
    impl AppAgentStrategy for MockAppStrategy {
        fn provider(&self) -> &str {
            "mock"
        }
        fn model(&self) -> &str {
            "m"
        }
        fn endpoint(&self) -> &str {
            "http://127.0.0.1:9/never"
        }
        fn env_var(&self) -> &'static str {
            ""
        }
        fn build_request(
            &self,
            _: &str,
            _: &[Message],
            _: &[ToolDef],
            _: &str,
        ) -> reqwest::Request {
            reqwest::Client::new()
                .get("http://127.0.0.1:9/never")
                .build()
                .unwrap()
        }
        fn parse_sse_event(&self, _: &str) -> Option<StreamEvent> {
            None
        }
    }

    fn make_session() -> AgentSession {
        AgentSession {
            kind: AgentKind::Vibe,
            variant: AgentVariant::App,
            sid: "t".into(),
            provider: "mock".into(),
            model: "m".into(),
        }
    }

    #[test]
    fn idle_with_tool_result_tail_transitions_to_streaming() {
        let mut app = App::new();
        app.add_plugins(bevy::app::TaskPoolPlugin::default());
        let mut s = AgentStrategies::default();
        s.register_app(Arc::new(MockAppStrategy));
        app.insert_resource(s);
        app.add_systems(Update, continue_after_tool);
        let entity = app
            .world_mut()
            .spawn((
                make_session(),
                AgentMessages(vec![Message::ToolResult {
                    call_id: "c1".into(),
                    content: "ok".into(),
                    is_error: false,
                }]),
                AgentRunState::Idle,
            ))
            .id();
        app.update();
        assert!(matches!(
            app.world().get::<AgentRunState>(entity),
            Some(AgentRunState::Streaming { .. })
        ));
    }

    #[test]
    fn idle_without_tool_result_tail_stays_idle() {
        let mut app = App::new();
        app.add_plugins(bevy::app::TaskPoolPlugin::default());
        let mut s = AgentStrategies::default();
        s.register_app(Arc::new(MockAppStrategy));
        app.insert_resource(s);
        app.add_systems(Update, continue_after_tool);
        let entity = app
            .world_mut()
            .spawn((
                make_session(),
                AgentMessages(vec![Message::User { text: "hi".into() }]),
                AgentRunState::Idle,
            ))
            .id();
        app.update();
        assert!(matches!(
            app.world().get::<AgentRunState>(entity),
            Some(AgentRunState::Idle)
        ));
    }
}
