use bevy::prelude::*;

use crate::components::{AgentMessages, AgentSession};
use crate::message::{AssistantBlock, Message};
use crate::run_state::AgentRunState;
use crate::run_state_kind::{AgentRunStateKind, LastRunStateKind};
use crate::toast::{AgentToast, ToastLevel};

pub fn surface_errors(
    mut writer: MessageWriter<AgentToast>,
    mut q: Query<(
        &AgentRunState,
        &mut LastRunStateKind,
        &mut AgentMessages,
        &AgentSession,
    )>,
) {
    for (state, mut last, mut messages, session) in &mut q {
        let cur = AgentRunStateKind::from(state);
        if last.0 == cur {
            continue;
        }
        last.0 = cur;
        if cur != AgentRunStateKind::Errored {
            continue;
        }
        let AgentRunState::Errored(msg) = state else {
            continue;
        };
        messages.0.push(Message::Assistant {
            blocks: vec![AssistantBlock::Text(format!("\u{26A0} {msg}"))],
        });
        writer.write(AgentToast {
            session_sid: session.sid.clone(),
            level: ToastLevel::Error,
            message: msg.clone(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AgentKind, AgentVariant};

    fn make_app() -> App {
        let mut app = App::new();
        app.add_plugins(bevy::app::TaskPoolPlugin::default())
            .add_message::<AgentToast>()
            .add_systems(Update, surface_errors);
        app
    }

    fn make_session() -> AgentSession {
        AgentSession {
            kind: AgentKind::Vibe,
            variant: AgentVariant::Page,
            sid: "abc".into(),
            provider: "mock".into(),
            model: "m".into(),
        }
    }

    #[test]
    fn errored_transition_appends_inline_and_fires_toast() {
        let mut app = make_app();
        let entity = app
            .world_mut()
            .spawn((
                make_session(),
                AgentMessages::default(),
                LastRunStateKind::default(),
                AgentRunState::Errored("boom".into()),
            ))
            .id();
        app.update();
        let msgs = app.world().get::<AgentMessages>(entity).unwrap();
        assert_eq!(msgs.0.len(), 1);
        match &msgs.0[0] {
            Message::Assistant { blocks } => match &blocks[0] {
                AssistantBlock::Text(t) => assert!(t.contains("boom")),
                _ => panic!("expected text block"),
            },
            _ => panic!("expected assistant message"),
        }
        let events: Vec<AgentToast> = app
            .world_mut()
            .resource_mut::<bevy::ecs::message::Messages<AgentToast>>()
            .drain()
            .collect();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].session_sid, "abc");
        assert_eq!(events[0].level, ToastLevel::Error);
        assert!(events[0].message.contains("boom"));
    }

    #[test]
    fn no_op_when_state_kind_unchanged() {
        let mut app = make_app();
        let _entity = app
            .world_mut()
            .spawn((
                make_session(),
                AgentMessages::default(),
                LastRunStateKind(AgentRunStateKind::Errored),
                AgentRunState::Errored("old".into()),
            ))
            .id();
        app.update();
        let msgs = app.world().get::<AgentMessages>(_entity).unwrap();
        assert!(msgs.0.is_empty());
        let events: Vec<AgentToast> = app
            .world_mut()
            .resource_mut::<bevy::ecs::message::Messages<AgentToast>>()
            .drain()
            .collect();
        assert!(events.is_empty());
    }
}
