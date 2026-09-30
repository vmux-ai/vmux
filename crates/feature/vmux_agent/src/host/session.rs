pub(super) mod continuation;
pub(super) mod resume;
mod source;

pub(crate) use source::CliSessionSources;

#[cfg(test)]
use std::path::PathBuf;
use std::sync::{Mutex, mpsc};
#[cfg(test)]
use std::time::SystemTime;

use bevy::prelude::*;
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use vmux_core::PageMetadata;
pub use vmux_core::agent::{AgentSession, PendingAgentSession, SessionId};

use super::cli::CliSessionRoot;
#[cfg(test)]
use crate::AgentKind;

#[derive(Message, Debug, Clone, Copy)]
pub struct AgentSessionExited {
    pub entity: Entity,
}

#[derive(Message)]
pub(super) struct DiscoverAgentSessions;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct DiscoverAgentSessionsSet;

pub(crate) struct AgentSessionLifecyclePlugin;

impl Plugin for AgentSessionLifecyclePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            continuation::AgentContinuationPlugin,
            resume::ChatResumePlugin,
        ))
        .add_message::<AgentSessionExited>()
        .add_message::<DiscoverAgentSessions>()
        .configure_sets(Update, DiscoverAgentSessionsSet)
        .add_systems(
            Update,
            (
                start_agent_session_watchers,
                request_discovery_for_fs_change,
                request_discovery_for_pending,
            )
                .chain()
                .before(DiscoverAgentSessionsSet),
        )
        .add_systems(Update, format_agent_url);
    }
}

#[allow(clippy::type_complexity)]
fn format_agent_url(
    sources: CliSessionSources,
    mut q: Query<
        (Option<&SessionId>, &AgentSession, &mut PageMetadata),
        Or<(Changed<SessionId>, Added<AgentSession>, Added<PageMetadata>)>,
    >,
) {
    for (sid, agent, mut meta) in &mut q {
        if sources.get(agent.kind).is_none() {
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
    use crate::host::cli::VIBE as VIBE_CLI;

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
        app.world_mut().spawn(VIBE_CLI);
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
        assert_eq!(url, "vmux://sessions/vibe/cli/abc");
    }

    #[test]
    fn format_agent_url_emits_fresh_cli_url_when_no_session_id() {
        let mut app = App::new();
        app.world_mut().spawn(VIBE_CLI);
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
        assert_eq!(url, "vmux://sessions/vibe/cli");
    }

    #[test]
    fn format_agent_url_sets_title_with_short_session_id() {
        let mut app = App::new();
        app.world_mut().spawn(VIBE_CLI);
        app.add_systems(Update, format_agent_url);

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
        app.world_mut().spawn(VIBE_CLI);
        app.add_systems(Update, format_agent_url);

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
        app.world_mut().spawn(VIBE_CLI);
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
        let title = &app.world().get::<PageMetadata>(entity).unwrap().title;
        assert_eq!(title, "Vibe CLI");
    }

    #[test]
    fn format_agent_url_clears_stale_builtin_icon_so_provider_favicon_resolves() {
        let mut app = App::new();
        app.world_mut().spawn(VIBE_CLI);
        app.add_systems(Update, format_agent_url);

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

fn request_discovery_for_pending(
    added_pending: Query<(), Added<PendingAgentSession>>,
    added_session: Query<(), Added<SessionId>>,
    mut requests: MessageWriter<DiscoverAgentSessions>,
) {
    if !added_pending.is_empty() || !added_session.is_empty() {
        requests.write(DiscoverAgentSessions);
    }
}

#[derive(Component)]
struct AgentSessionWatcher {
    receiver: Mutex<mpsc::Receiver<()>>,
    _watcher: RecommendedWatcher,
}

fn start_agent_session_watchers(
    roots: Query<&CliSessionRoot, Added<CliSessionRoot>>,
    mut commands: Commands,
) {
    for root in &roots {
        let root = root.0.clone();
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

fn request_discovery_for_fs_change(
    watchers: Query<&AgentSessionWatcher>,
    mut requests: MessageWriter<DiscoverAgentSessions>,
) {
    let mut changed = false;
    for watcher in &watchers {
        let Ok(rx) = watcher.receiver.lock() else {
            continue;
        };
        while rx.try_recv().is_ok() {
            changed = true;
        }
    }
    if changed {
        requests.write(DiscoverAgentSessions);
    }
}
