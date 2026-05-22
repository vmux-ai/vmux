use bevy::prelude::*;
use bevy::tasks::IoTaskPool;
use crossbeam_channel::unbounded;

use crate::client::page::strategy_components::{BuildRequestFn, EnvVarName, ParseSseFn};
use crate::client::page::strategy_index::PageStrategyIndex;
use crate::components::{AgentMessages, AgentSession};
use crate::http::drive_sse;
use crate::message::Message;
use crate::run_state::AgentRunState;
use crate::stream::StreamEvent;
use crate::tools::mcp_tool_defs;

pub fn continue_after_tool(
    idx: Res<PageStrategyIndex>,
    build_q: Query<&BuildRequestFn>,
    parse_q: Query<&ParseSseFn>,
    env_q: Query<&EnvVarName>,
    mut q: Query<(&mut AgentRunState, &AgentMessages, &AgentSession)>,
) {
    for (mut state, messages, session) in &mut q {
        if !matches!(*state, AgentRunState::Idle) {
            continue;
        }
        if !matches!(messages.0.last(), Some(Message::ToolResult { .. })) {
            continue;
        }
        let Some((build_request, parse_sse, env_var)) = idx.lookup_fns(
            &session.provider,
            &session.model,
            &build_q,
            &parse_q,
            &env_q,
        ) else {
            *state = AgentRunState::Errored(format!(
                "No registered Page strategy for {}/{}",
                session.provider, session.model
            ));
            continue;
        };
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
        let request = build_request(&session.model, &messages.0, &tools, &api_key);
        let (tx, rx) = unbounded::<StreamEvent>();
        let task = IoTaskPool::get().spawn(async move {
            drive_sse(request, parse_sse, tx).await;
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
    use crate::client::page::strategy_components::{
        BuildRequestFn, Endpoint, EnvVarName, ParseSseFn, Strategy, StrategyKey, StrategyKind,
        StrategyVariant,
    };
    use crate::client::page::strategy_index::PageStrategyIndex;
    use crate::client::page::strategy_indexer::{on_strategy_added, on_strategy_removed};
    use crate::stream::ToolDef;
    use crate::{AgentKind, AgentVariant};

    fn mock_build_request(_: &str, _: &[Message], _: &[ToolDef], _: &str) -> reqwest::Request {
        reqwest::Client::new()
            .get("http://127.0.0.1:9/never")
            .build()
            .unwrap()
    }

    fn mock_parse_sse(_: &str) -> Option<StreamEvent> {
        None
    }

    fn spawn_mock_strategy(app: &mut App) {
        app.world_mut().spawn((
            Strategy,
            StrategyKey {
                provider: "mock".into(),
                model: "m".into(),
            },
            Endpoint("http://127.0.0.1:9/never".into()),
            EnvVarName(""),
            StrategyKind(AgentKind::Vibe),
            StrategyVariant(AgentVariant::Page),
            BuildRequestFn(mock_build_request),
            ParseSseFn(mock_parse_sse),
        ));
        app.update();
    }

    fn make_session() -> AgentSession {
        AgentSession {
            kind: AgentKind::Vibe,
            variant: AgentVariant::Page,
            sid: "t".into(),
            provider: "mock".into(),
            model: "m".into(),
        }
    }

    fn make_app() -> App {
        let mut app = App::new();
        app.add_plugins(bevy::app::TaskPoolPlugin::default());
        app.insert_resource(PageStrategyIndex::default());
        app.add_observer(on_strategy_added);
        app.add_observer(on_strategy_removed);
        app.add_systems(Update, continue_after_tool);
        spawn_mock_strategy(&mut app);
        app
    }

    #[test]
    fn idle_with_tool_result_tail_transitions_to_streaming() {
        let mut app = make_app();
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
        let mut app = make_app();
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
