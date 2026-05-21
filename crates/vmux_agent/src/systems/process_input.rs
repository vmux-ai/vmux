use bevy::prelude::*;
use bevy::tasks::IoTaskPool;
use crossbeam_channel::unbounded;

use crate::components::{AgentMessages, AgentSession, PendingUserInput};
use crate::http::drive_sse;
use crate::message::Message;
use crate::run_state::AgentRunState;
use crate::strategy::AgentStrategies;
use crate::stream::StreamEvent;
use crate::tools::mcp_tool_defs;

pub fn process_user_input(
    mut commands: Commands,
    strategies: Res<AgentStrategies>,
    mut q: Query<(
        Entity,
        &PendingUserInput,
        &mut AgentMessages,
        &mut AgentRunState,
        &AgentSession,
    )>,
) {
    for (entity, pending, mut messages, mut state, session) in &mut q {
        if !matches!(*state, AgentRunState::Idle | AgentRunState::Errored(_)) {
            continue;
        }
        messages.0.push(Message::User {
            text: pending.0.clone(),
        });

        let Some(strategy) =
            strategies.get_page_by_provider_model(&session.provider, &session.model)
        else {
            *state = AgentRunState::Errored(format!(
                "No registered Page strategy for {}/{}",
                session.provider, session.model
            ));
            commands.entity(entity).remove::<PendingUserInput>();
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
                    commands.entity(entity).remove::<PendingUserInput>();
                    continue;
                }
            }
        };

        let tools = mcp_tool_defs();
        let request = strategy.build_request(&session.model, &messages.0, &tools, &api_key);
        let parse_sse = strategy.parse_sse_fn();
        let (tx, rx) = unbounded::<StreamEvent>();
        let task = IoTaskPool::get().spawn(async move {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("tokio runtime");
            rt.block_on(drive_sse(request, parse_sse, tx));
        });

        *state = AgentRunState::Streaming {
            rx,
            _task: task,
            partial: None,
        };
        commands.entity(entity).remove::<PendingUserInput>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::page::strategy::AgentPageStrategy;
    use crate::strategy::AgentStrategy;
    use crate::stream::ToolDef;
    use crate::{AgentKind, AgentVariant};
    use std::sync::Arc;

    struct MockPageStrategy;
    impl AgentStrategy for MockPageStrategy {
        fn kind(&self) -> AgentKind {
            AgentKind::Vibe
        }
        fn variant(&self) -> AgentVariant {
            AgentVariant::Page
        }
    }
    impl AgentPageStrategy for MockPageStrategy {
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
        fn parse_sse_fn(&self) -> crate::client::page::strategy_components::ParseSse {
            fn mock_parse(_: &str) -> Option<crate::stream::StreamEvent> {
                None
            }
            mock_parse
        }
    }

    fn make_app() -> App {
        let mut app = App::new();
        app.add_plugins(bevy::app::TaskPoolPlugin::default());
        let mut s = AgentStrategies::default();
        s.register_page(Arc::new(MockPageStrategy));
        app.insert_resource(s);
        app.add_systems(Update, process_user_input);
        app
    }

    #[test]
    fn transitions_idle_to_streaming_when_strategy_present() {
        let mut app = make_app();
        let entity = app
            .world_mut()
            .spawn((
                AgentSession {
                    kind: AgentKind::Vibe,
                    variant: AgentVariant::Page,
                    sid: "t".into(),
                    provider: "mock".into(),
                    model: "m".into(),
                },
                AgentMessages::default(),
                AgentRunState::Idle,
                PendingUserInput("hi".into()),
            ))
            .id();
        app.update();
        let world = app.world();
        let state = world.get::<AgentRunState>(entity).unwrap();
        assert!(matches!(state, AgentRunState::Streaming { .. }));
        assert!(world.get::<PendingUserInput>(entity).is_none());
    }

    #[test]
    fn errors_when_no_strategy_registered() {
        let mut app = App::new();
        app.add_plugins(bevy::app::TaskPoolPlugin::default());
        app.insert_resource(AgentStrategies::default());
        app.add_systems(Update, process_user_input);
        let entity = app
            .world_mut()
            .spawn((
                AgentSession {
                    kind: AgentKind::Vibe,
                    variant: AgentVariant::Page,
                    sid: "t".into(),
                    provider: "missing".into(),
                    model: "m".into(),
                },
                AgentMessages::default(),
                AgentRunState::Idle,
                PendingUserInput("hi".into()),
            ))
            .id();
        app.update();
        let state = app.world().get::<AgentRunState>(entity).unwrap();
        match state {
            AgentRunState::Errored(msg) => assert!(msg.contains("missing/m"), "msg was: {msg}"),
            _ => panic!("expected Errored"),
        }
    }
}
