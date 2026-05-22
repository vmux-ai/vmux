use bevy::prelude::*;
use bevy::tasks::IoTaskPool;
use crossbeam_channel::unbounded;

use crate::client::page::strategy_components::{BuildRequestFn, EnvVarName, ParseSseFn};
use crate::client::page::strategy_index::PageStrategyIndex;
use crate::components::{AgentMessages, AgentSession, PendingUserInput};
use crate::http::drive_sse;
use crate::message::Message;
use crate::run_state::AgentRunState;
use crate::stream::StreamEvent;
use crate::tools::mcp_tool_defs;

pub fn process_user_input(
    mut commands: Commands,
    idx: Res<PageStrategyIndex>,
    build_q: Query<&BuildRequestFn>,
    parse_q: Query<&ParseSseFn>,
    env_q: Query<&EnvVarName>,
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
            commands.entity(entity).remove::<PendingUserInput>();
            continue;
        };

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
        let request = build_request(&session.model, &messages.0, &tools, &api_key);
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

    fn make_app() -> App {
        let mut app = App::new();
        app.add_plugins(bevy::app::TaskPoolPlugin::default());
        app.insert_resource(PageStrategyIndex::default());
        app.add_observer(on_strategy_added);
        app.add_observer(on_strategy_removed);
        app.add_systems(Update, process_user_input);
        spawn_mock_strategy(&mut app);
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
        app.insert_resource(PageStrategyIndex::default());
        app.add_observer(on_strategy_added);
        app.add_observer(on_strategy_removed);
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
