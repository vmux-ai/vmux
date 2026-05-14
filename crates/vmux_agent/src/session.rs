use std::collections::HashMap;
use std::path::PathBuf;
use std::time::SystemTime;

use bevy::prelude::*;
use vmux_core::PageMetadata;

use crate::AgentKind;
use crate::strategy::AgentStrategies;

#[derive(Component, Debug, Clone)]
pub struct AgentSession {
    pub kind: AgentKind,
}

#[derive(Component, Debug, Clone)]
pub struct SessionId(pub String);

#[derive(Component, Debug, Clone)]
pub struct PendingAgentSession {
    pub kind: AgentKind,
    pub spawn_time: SystemTime,
    pub cwd: PathBuf,
}

#[derive(Resource, Default, Debug)]
pub struct AgentSessionToEntity(pub HashMap<(AgentKind, String), Entity>);

#[derive(Resource, Default, Debug)]
pub struct AgentSessionDirty(pub bool);

#[allow(clippy::type_complexity)]
pub fn format_agent_url(
    strategies: Res<AgentStrategies>,
    mut q: Query<
        (Option<&SessionId>, &AgentSession, &mut PageMetadata),
        Or<(Changed<SessionId>, Added<AgentSession>, Added<PageMetadata>)>,
    >,
) {
    for (sid, agent, mut meta) in &mut q {
        let Some(strategy) = strategies.get(agent.kind) else {
            continue;
        };
        let scheme = strategy.kind().url_scheme();
        let next = match sid {
            Some(SessionId(id)) => format!("{scheme}{id}"),
            None => scheme.to_string(),
        };
        if meta.url != next {
            meta.url = next;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_session_to_entity_starts_empty() {
        let map = AgentSessionToEntity::default();
        assert!(map.0.is_empty());
    }

    #[test]
    fn pending_session_carries_cwd_and_kind() {
        let pending = PendingAgentSession {
            kind: AgentKind::Claude,
            spawn_time: SystemTime::UNIX_EPOCH,
            cwd: PathBuf::from("/tmp/x"),
        };
        assert_eq!(pending.kind, AgentKind::Claude);
        assert_eq!(pending.cwd, PathBuf::from("/tmp/x"));
    }
}

#[cfg(test)]
mod url_tests {
    use super::*;
    use crate::vibe::VibeStrategy;

    fn empty_meta() -> PageMetadata {
        PageMetadata {
            title: String::new(),
            url: String::new(),
            favicon_url: String::new(),
            bg_color: None,
        }
    }

    #[test]
    fn format_agent_url_emits_scheme_with_session_id() {
        let mut app = App::new();
        let mut strategies = AgentStrategies::default();
        strategies.register(Box::new(VibeStrategy));
        app.insert_resource(strategies);
        app.add_systems(Update, format_agent_url);

        let entity = app
            .world_mut()
            .spawn((
                AgentSession {
                    kind: AgentKind::Vibe,
                },
                SessionId("abc".into()),
                empty_meta(),
            ))
            .id();
        app.update();
        let url = &app.world().get::<PageMetadata>(entity).unwrap().url;
        assert_eq!(url, "vmux://vibe/abc");
    }

    #[test]
    fn format_agent_url_emits_scheme_only_when_no_session_id() {
        let mut app = App::new();
        let mut strategies = AgentStrategies::default();
        strategies.register(Box::new(VibeStrategy));
        app.insert_resource(strategies);
        app.add_systems(Update, format_agent_url);

        let entity = app
            .world_mut()
            .spawn((
                AgentSession {
                    kind: AgentKind::Vibe,
                },
                empty_meta(),
            ))
            .id();
        app.update();
        let url = &app.world().get::<PageMetadata>(entity).unwrap().url;
        assert_eq!(url, "vmux://vibe/");
    }
}
