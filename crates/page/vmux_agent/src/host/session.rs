use std::collections::HashSet;
#[cfg(test)]
use std::path::PathBuf;
use std::sync::{Mutex, mpsc};
#[cfg(test)]
use std::time::SystemTime;

use bevy::prelude::*;
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use vmux_core::PageMetadata;
pub use vmux_core::agent::{AgentSession, PendingAgentSession, SessionId};

#[cfg(test)]
use crate::AgentKind;
use crate::strategy::AgentStrategies;

#[derive(Message, Debug, Clone, Copy)]
pub struct AgentSessionExited {
    pub entity: Entity,
}

#[derive(Component, Default)]
struct AgentSessionDiscovery {
    dirty: bool,
}

pub(crate) struct AgentSessionLifecyclePlugin;

impl Plugin for AgentSessionLifecyclePlugin {
    fn build(&self, app: &mut App) {
        app.world_mut().spawn((
            Name::new("Agent session discovery"),
            AgentSessionDiscovery::default(),
        ));
        app.add_message::<AgentSessionExited>()
            .add_systems(Startup, start_agent_session_watchers)
            .add_systems(
                Update,
                (mark_dirty_on_fs_change, mark_dirty_on_pending_added),
            )
            .add_systems(
                Update,
                (
                    discover_pending_agent_sessions,
                    detect_file_end_time_exit,
                    clear_agent_session_dirty,
                )
                    .chain()
                    .after(mark_dirty_on_fs_change)
                    .after(mark_dirty_on_pending_added)
                    .run_if(agent_session_dirty_run_condition),
            )
            .add_systems(Update, format_agent_url);
    }
}

#[allow(clippy::type_complexity)]
fn format_agent_url(
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
mod tests {
    use super::*;

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
    use crate::runtime::cli::vibe::VibeStrategy;

    fn empty_meta() -> PageMetadata {
        PageMetadata {
            title: String::new(),
            url: String::new(),
            icon: vmux_core::PageIcon::None,
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
        assert_eq!(url, "vmux://sessions/vibe/cli/abc");
    }

    #[test]
    fn format_agent_url_emits_fresh_cli_url_when_no_session_id() {
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
        assert_eq!(url, "vmux://sessions/vibe/cli");
    }

    #[test]
    fn format_agent_url_sets_title_with_short_session_id() {
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
                SessionId("abc12345".into()),
                empty_meta(),
            ))
            .id();
        app.update();
        let title = &app.world().get::<PageMetadata>(entity).unwrap().title;
        assert_eq!(title, "Vibe CLI (abc12345)");
    }

    #[test]
    fn format_agent_url_truncates_long_session_id_in_title() {
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
                SessionId("550e8400e29b41d4a716446655440000".into()),
                empty_meta(),
            ))
            .id();
        app.update();
        let title = &app.world().get::<PageMetadata>(entity).unwrap().title;
        assert_eq!(title, "Vibe CLI (550e84…0000)");
    }

    #[test]
    fn format_agent_url_sets_bare_name_title_when_no_session_id() {
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
        let title = &app.world().get::<PageMetadata>(entity).unwrap().title;
        assert_eq!(title, "Vibe CLI");
    }

    #[test]
    fn format_agent_url_clears_stale_builtin_icon_so_provider_favicon_resolves() {
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
                PageMetadata {
                    title: "Terminal".into(),
                    url: vmux_core::event::TERMINAL_PAGE_URL.to_string(),
                    icon: vmux_core::PageIcon::Builtin(vmux_core::BuiltinIcon::Terminal),
                    bg_color: None,
                },
            ))
            .id();
        app.update();
        assert_eq!(
            app.world().get::<PageMetadata>(entity).unwrap().icon,
            vmux_core::PageIcon::None
        );
    }

    #[test]
    fn truncate_sid_keeps_short_ids() {
        assert_eq!(truncate_sid("abc"), "abc");
        assert_eq!(truncate_sid("abcdefghijkl"), "abcdefghijkl");
    }

    #[test]
    fn truncate_sid_middle_truncates_long_ids() {
        assert_eq!(truncate_sid("abcdefghijklm"), "abcdef…jklm");
        assert_eq!(
            truncate_sid("550e8400e29b41d4a716446655440000"),
            "550e84…0000"
        );
    }
}

fn mark_dirty_on_pending_added(
    added_pending: Query<(), Added<PendingAgentSession>>,
    added_session: Query<(), Added<SessionId>>,
    mut discovery: Single<&mut AgentSessionDiscovery>,
) {
    if !added_pending.is_empty() || !added_session.is_empty() {
        discovery.dirty = true;
    }
}

fn agent_session_dirty_run_condition(discovery: Single<&AgentSessionDiscovery>) -> bool {
    discovery.dirty
}

fn clear_agent_session_dirty(mut discovery: Single<&mut AgentSessionDiscovery>) {
    discovery.dirty = false;
}

fn discover_pending_agent_sessions(
    mut commands: Commands,
    strategies: Res<AgentStrategies>,
    pending_sessions: Query<(Entity, &PendingAgentSession)>,
    sessions: Query<(&AgentSession, &SessionId)>,
) {
    for (entity, pending) in &pending_sessions {
        let Some(strategy) = strategies.get_cli(pending.kind) else {
            continue;
        };
        let claimed = sessions
            .iter()
            .filter_map(|(session, id)| {
                if session.kind == pending.kind {
                    Some(id.0.clone())
                } else {
                    None
                }
            })
            .collect::<HashSet<_>>();
        if let Some(id) = strategy.discover_session(&pending.cwd, pending.spawn_time, &claimed) {
            commands
                .entity(entity)
                .insert(SessionId(id))
                .remove::<PendingAgentSession>();
        }
    }
}

#[derive(Component)]
struct AgentSessionWatcher {
    receiver: Mutex<mpsc::Receiver<()>>,
    _watcher: RecommendedWatcher,
}

fn start_agent_session_watchers(mut commands: Commands, strategies: Res<AgentStrategies>) {
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
        commands.spawn(AgentSessionWatcher {
            receiver: Mutex::new(rx),
            _watcher: watcher,
        });
    }
}

fn mark_dirty_on_fs_change(
    watchers: Query<&AgentSessionWatcher>,
    mut discovery: Single<&mut AgentSessionDiscovery>,
) {
    for watcher in &watchers {
        let Ok(rx) = watcher.receiver.lock() else {
            continue;
        };
        while rx.try_recv().is_ok() {
            discovery.dirty = true;
        }
    }
}

fn detect_file_end_time_exit(
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
    use crate::runtime::cli::vibe::VibeStrategy;

    #[test]
    fn pending_with_no_match_keeps_pending() {
        let mut app = App::new();
        let mut strategies = AgentStrategies::default();
        strategies.register_cli(Box::new(VibeStrategy));
        app.insert_resource(strategies)
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

    #[test]
    fn pending_is_retained_long_after_spawn_for_late_session_dir() {
        use std::path::Path;

        struct NeverDiscovers;
        impl crate::strategy::AgentStrategy for NeverDiscovers {
            fn kind(&self) -> AgentKind {
                AgentKind::Vibe
            }

            fn variant(&self) -> crate::AgentVariant {
                crate::AgentVariant::Cli
            }
        }

        impl crate::CliAgentStrategy for NeverDiscovers {
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
                false
            }
        }

        let mut app = App::new();
        let mut strategies = AgentStrategies::default();
        strategies.register_cli(Box::new(NeverDiscovers));
        app.insert_resource(strategies)
            .add_systems(Update, discover_pending_agent_sessions);

        let pending = PendingAgentSession {
            kind: AgentKind::Vibe,
            spawn_time: SystemTime::UNIX_EPOCH,
            cwd: PathBuf::from("/this/path/does/not/exist"),
        };
        let entity = app.world_mut().spawn(pending).id();
        app.update();
        assert!(
            app.world().get::<PendingAgentSession>(entity).is_some(),
            "pending must survive long after spawn so a vibe session dir written mid-session is still discovered"
        );
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
