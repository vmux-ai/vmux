use std::collections::{HashMap, HashSet};
#[cfg(test)]
use std::path::PathBuf;
use std::sync::{Mutex, mpsc};
#[cfg(test)]
use std::time::SystemTime;

use bevy::prelude::*;
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use vmux_core::PageMetadata;
pub use vmux_core::agent::{AgentSession, PendingAgentSession, SessionId};

use crate::AgentKind;
use crate::strategy::AgentStrategies;

#[derive(Message, Debug, Clone, Copy)]
pub struct AgentSessionExited {
    pub entity: Entity,
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
        let Some(strategy) = strategies.get_cli(agent.kind) else {
            continue;
        };
        let prefix = strategy.kind().cli_url_prefix();
        let next = match sid {
            Some(SessionId(id)) => format!("{prefix}{id}"),
            None => prefix,
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
    use crate::client::cli::vibe::VibeStrategy;

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
        strategies.register_cli(Box::new(VibeStrategy));
        app.insert_resource(strategies)
            .add_systems(Update, format_agent_url);

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
        assert_eq!(url, "vmux://agent/vibe/abc");
    }

    #[test]
    fn format_agent_url_emits_scheme_only_when_no_session_id() {
        let mut app = App::new();
        let mut strategies = AgentStrategies::default();
        strategies.register_cli(Box::new(VibeStrategy));
        app.insert_resource(strategies)
            .add_systems(Update, format_agent_url);

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
        assert_eq!(url, "vmux://agent/vibe/");
    }
}

pub const PENDING_DISCOVERY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

pub fn mark_dirty_on_pending_added(
    added_pending: Query<(), Added<PendingAgentSession>>,
    added_session: Query<(), Added<SessionId>>,
    mut dirty: ResMut<AgentSessionDirty>,
) {
    if !added_pending.is_empty() || !added_session.is_empty() {
        dirty.0 = true;
    }
}

pub fn agent_session_dirty_run_condition(dirty: Res<AgentSessionDirty>) -> bool {
    dirty.0
}

pub fn clear_agent_session_dirty(mut dirty: ResMut<AgentSessionDirty>) {
    dirty.0 = false;
}

pub fn discover_pending_agent_sessions(
    mut commands: Commands,
    strategies: Res<AgentStrategies>,
    map: Res<AgentSessionToEntity>,
    q: Query<(Entity, &PendingAgentSession)>,
) {
    let now = std::time::SystemTime::now();
    for (entity, pending) in &q {
        let Some(strategy) = strategies.get_cli(pending.kind) else {
            continue;
        };
        let claimed: HashSet<String> = map
            .0
            .iter()
            .filter_map(|((k, id), _)| {
                if *k == pending.kind {
                    Some(id.clone())
                } else {
                    None
                }
            })
            .collect();
        if let Some(id) = strategy.discover_session(&pending.cwd, pending.spawn_time, &claimed) {
            commands
                .entity(entity)
                .insert(SessionId(id))
                .remove::<PendingAgentSession>();
            continue;
        }
        if now.duration_since(pending.spawn_time).unwrap_or_default() >= PENDING_DISCOVERY_TIMEOUT {
            commands.entity(entity).remove::<PendingAgentSession>();
        }
    }
}

pub fn track_session_id_inserts(
    mut map: ResMut<AgentSessionToEntity>,
    inserted: Query<(Entity, &SessionId, &AgentSession), Added<SessionId>>,
) {
    for (entity, SessionId(id), agent) in &inserted {
        map.0.insert((agent.kind, id.clone()), entity);
    }
}

pub fn track_session_id_removals(
    mut map: ResMut<AgentSessionToEntity>,
    mut removed: RemovedComponents<SessionId>,
) {
    for entity in removed.read() {
        map.0.retain(|_, &mut e| e != entity);
    }
}

#[cfg(test)]
mod tracking_tests {
    use super::*;

    fn make_app() -> App {
        let mut app = App::new();
        app.init_resource::<AgentSessionToEntity>().add_systems(
            Update,
            (track_session_id_inserts, track_session_id_removals).chain(),
        );
        app
    }

    #[test]
    fn insert_populates_map_only_for_agent_session_entities() {
        let mut app = make_app();
        let with = app
            .world_mut()
            .spawn((
                AgentSession {
                    kind: AgentKind::Codex,
                },
                SessionId("c1".into()),
            ))
            .id();
        let without = app.world_mut().spawn(SessionId("nope".into())).id();
        app.update();
        let map = app.world().resource::<AgentSessionToEntity>();
        assert_eq!(map.0.get(&(AgentKind::Codex, "c1".into())), Some(&with));
        assert!(!map.0.contains_key(&(AgentKind::Codex, "nope".into())));
        let _ = without;
    }

    #[test]
    fn entity_despawn_removes_session_from_map() {
        let mut app = make_app();
        let e = app
            .world_mut()
            .spawn((
                AgentSession {
                    kind: AgentKind::Vibe,
                },
                SessionId("v1".into()),
            ))
            .id();
        app.update();
        app.world_mut().despawn(e);
        app.update();
        let map = app.world().resource::<AgentSessionToEntity>();
        assert!(!map.0.contains_key(&(AgentKind::Vibe, "v1".into())));
    }
}

#[derive(Resource)]
pub struct AgentSessionWatchers {
    receivers: Vec<Mutex<mpsc::Receiver<()>>>,
    _watchers: Vec<RecommendedWatcher>,
}

pub fn start_agent_session_watchers(mut commands: Commands, strategies: Res<AgentStrategies>) {
    let mut receivers = Vec::new();
    let mut watchers = Vec::new();
    for strategy in strategies.cli_strategies() {
        let root = strategy.sessions_root();
        if std::fs::create_dir_all(&root).is_err() {
            continue;
        }
        let (tx, rx) = mpsc::channel();
        let watcher =
            notify::recommended_watcher(move |res: Result<notify::Event, notify::Error>| {
                if let Ok(event) = res
                    && (event.kind.is_create() || event.kind.is_modify())
                {
                    let _ = tx.send(());
                }
            });
        let Ok(mut watcher) = watcher else { continue };
        if watcher.watch(&root, RecursiveMode::Recursive).is_err() {
            continue;
        }
        watchers.push(watcher);
        receivers.push(Mutex::new(rx));
    }
    if receivers.is_empty() {
        return;
    }
    commands.insert_resource(AgentSessionWatchers {
        receivers,
        _watchers: watchers,
    });
}

pub fn mark_dirty_on_fs_change(
    watchers: Option<Res<AgentSessionWatchers>>,
    mut dirty: ResMut<AgentSessionDirty>,
) {
    let Some(watchers) = watchers else { return };
    for rx in &watchers.receivers {
        let Ok(rx) = rx.lock() else { continue };
        while rx.try_recv().is_ok() {
            dirty.0 = true;
        }
    }
}

pub fn detect_file_end_time_exit(
    mut commands: Commands,
    mut exited_writer: MessageWriter<AgentSessionExited>,
    strategies: Res<AgentStrategies>,
    sessioned: Query<(Entity, &AgentSession, &SessionId)>,
) {
    for (entity, agent, sid) in &sessioned {
        let Some(strategy) = strategies.get_cli(agent.kind) else {
            continue;
        };
        if !strategy.detect_end_time(&sid.0) {
            continue;
        }
        commands
            .entity(entity)
            .remove::<AgentSession>()
            .remove::<SessionId>()
            .remove::<PendingAgentSession>();
        exited_writer.write(AgentSessionExited { entity });
    }
}

#[cfg(test)]
mod discovery_tests {
    use super::*;
    use crate::client::cli::vibe::VibeStrategy;

    #[test]
    fn pending_with_no_match_within_timeout_keeps_pending() {
        let mut app = App::new();
        let mut strategies = AgentStrategies::default();
        strategies.register_cli(Box::new(VibeStrategy));
        app.insert_resource(strategies)
            .init_resource::<AgentSessionToEntity>()
            .add_systems(Update, discover_pending_agent_sessions);

        let pending = PendingAgentSession {
            kind: AgentKind::Vibe,
            spawn_time: std::time::SystemTime::now(),
            cwd: PathBuf::from("/this/path/does/not/exist"),
        };
        let entity = app.world_mut().spawn(pending).id();
        app.update();
        assert!(app.world().get::<PendingAgentSession>(entity).is_some());
        assert!(app.world().get::<SessionId>(entity).is_none());
    }
}

#[cfg(test)]
mod exit_tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn detect_file_end_time_exit_strips_components_when_strategy_says_ended() {
        struct EndedStrategy;
        impl crate::strategy::AgentStrategy for EndedStrategy {
            fn kind(&self) -> AgentKind {
                AgentKind::Vibe
            }
            fn variant(&self) -> crate::AgentVariant {
                crate::AgentVariant::Cli
            }
        }
        impl crate::CliAgentStrategy for EndedStrategy {
            fn sessions_root(&self) -> PathBuf {
                PathBuf::from("/tmp/none")
            }
            fn build_args(&self, _: &crate::McpServerConfig, _: Option<&str>) -> Vec<String> {
                vec![]
            }
            fn build_env(&self, _: &crate::McpServerConfig) -> Vec<(String, String)> {
                vec![]
            }
            fn discover_session(
                &self,
                _: &Path,
                _: SystemTime,
                _: &HashSet<String>,
            ) -> Option<String> {
                None
            }
            fn detect_end_time(&self, _: &str) -> bool {
                true
            }
        }

        let mut app = App::new();
        let mut strategies = AgentStrategies::default();
        strategies.register_cli(Box::new(EndedStrategy));
        app.insert_resource(strategies)
            .add_message::<AgentSessionExited>()
            .add_systems(Update, detect_file_end_time_exit);

        let entity = app
            .world_mut()
            .spawn((
                AgentSession {
                    kind: AgentKind::Vibe,
                },
                SessionId("x".into()),
            ))
            .id();
        app.update();
        assert!(app.world().get::<AgentSession>(entity).is_none());
        assert!(app.world().get::<SessionId>(entity).is_none());
    }
}
