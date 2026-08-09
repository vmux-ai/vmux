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
        if strategies.get_cli(agent.kind).is_none() {
            continue;
        }
        let next = match sid {
            Some(SessionId(id)) => crate::url::AgentUrl::Cli {
                kind: agent.kind,
                sid: id.clone(),
            }
            .format(),
            None => crate::url::AgentUrl::Cli {
                kind: agent.kind,
                sid: crate::url::CLI_FRESH_SID.to_string(),
            }
            .format(),
        };
        if meta.url != next {
            meta.url = next;
        }
        let title = match sid {
            Some(SessionId(id)) => {
                format!("{} CLI ({})", agent.kind.display_name(), truncate_sid(id))
            }
            None => format!("{} CLI", agent.kind.display_name()),
        };
        if meta.title != title {
            meta.title = title;
        }
        if !meta.icon.is_none() {
            meta.icon = vmux_core::PageIcon::None;
        }
    }
}

fn truncate_sid(id: &str) -> String {
    let chars: Vec<char> = id.chars().collect();
    if chars.len() <= 12 {
        return id.to_string();
    }
    let head: String = chars[..6].iter().collect();
    let tail: String = chars[chars.len() - 4..].iter().collect();
    format!("{head}…{tail}")
}

#[cfg(test)]
#[path = "session.test.rs"]
mod tests;
#[cfg(test)]
#[path = "session.url.test.rs"]
mod url_tests;
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
#[path = "session.tracking.test.rs"]
mod tracking_tests;
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
#[path = "session.discovery.test.rs"]
mod discovery_tests;
#[cfg(test)]
#[path = "session.exit.test.rs"]
mod exit_tests;
