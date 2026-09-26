use std::time::{Duration, Instant};

use bevy::prelude::*;
use vmux_api::protocol::ProcessId;
use vmux_core::page::PageReady;

use crate::Terminal;
use crate::event::{AgentPromptDraftEvent, TermLoadingEvent};

use super::plugin::{ServiceMessageSet, ShellOutputSeen};
use super::prompt::{BufferedAgentPrompt, PromptCapture};
use super::state::TerminalMode;

pub(crate) struct LoadingPlugin;

impl Plugin for LoadingPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                arm_agent_loading,
                arm_agent_loading_on_restart,
                announce_slow_shell_boot.after(ServiceMessageSet),
                clear_agent_loading.after(ServiceMessageSet),
                reset_terminal_title_on_agent_removed,
                set_terminal_shell_icon,
            ),
        );
    }
}

const AGENT_LOADING_TIMEOUT: Duration = Duration::from_secs(10);
const SHELL_BOOT_GRACE: Duration = Duration::from_millis(250);

#[derive(Component, Debug, Clone, Copy)]
pub(crate) struct AgentLoading {
    pub(crate) since: Instant,
    pub(crate) announced: bool,
}

impl AgentLoading {
    fn armed(announced: bool) -> Self {
        Self {
            since: Instant::now(),
            announced,
        }
    }
}

fn set_terminal_shell_icon(
    mut terminals: Query<
        (&crate::launch::TerminalLaunch, &mut vmux_core::PageMetadata),
        With<Terminal>,
    >,
) {
    for (launch, mut metadata) in &mut terminals {
        if !matches!(launch.kind, crate::launch::TerminalKind::Plain) {
            continue;
        }
        if !metadata.icon.is_none() {
            continue;
        }
        if let Some(icon) = vmux_core::BuiltinIcon::for_shell(&launch.command) {
            metadata.icon = vmux_core::PageIcon::Builtin(icon);
        }
    }
}

fn labels(session: Option<&vmux_core::agent::AgentSession>) -> (String, String) {
    match session {
        Some(session) => (
            session.kind.display_name().to_string(),
            session.kind.as_url_segment().to_string(),
        ),
        None => ("Terminal".to_string(), "terminal".to_string()),
    }
}

fn arm_agent_loading(
    newly_ready: Query<
        (
            Entity,
            Option<&vmux_core::agent::AgentSession>,
            Option<&PromptCapture>,
            Has<ShellOutputSeen>,
        ),
        (With<Terminal>, Added<PageReady>, Without<AgentLoading>),
    >,
    mut commands: Commands,
) {
    for (entity, session, capture, output_seen) in &newly_ready {
        if session.is_none() && output_seen {
            continue;
        }
        let announced = session.is_some();
        commands
            .entity(entity)
            .insert(AgentLoading::armed(announced));
        if session.is_some() && capture.is_none() {
            commands.entity(entity).insert(PromptCapture::default());
        }
        if let Some(capture) = capture {
            commands.trigger(vmux_core::host::UiStateWrite::<
                vmux_core::event::TerminalUiState,
            >::from_event(
                entity,
                &AgentPromptDraftEvent {
                    draft: capture.draft.clone(),
                    skipped: capture.skipped,
                },
            ));
        }
        if !announced {
            continue;
        }
        let (label, segment) = labels(session);
        commands.trigger(vmux_core::host::UiStateWrite::<
            vmux_core::event::TerminalUiState,
        >::from_event(
            entity,
            &TermLoadingEvent {
                loading: true,
                label,
                segment,
            },
        ));
    }
}

fn announce_slow_shell_boot(
    mut waiting: Query<
        (Entity, &mut AgentLoading),
        (
            With<Terminal>,
            Without<vmux_core::agent::AgentSession>,
            Without<ShellOutputSeen>,
        ),
    >,
    mut commands: Commands,
) {
    for (entity, mut loading) in &mut waiting {
        if loading.announced || loading.since.elapsed() < SHELL_BOOT_GRACE {
            continue;
        }
        loading.announced = true;
        let (label, segment) = labels(None);
        commands.trigger(vmux_core::host::UiStateWrite::<
            vmux_core::event::TerminalUiState,
        >::from_event(
            entity,
            &TermLoadingEvent {
                loading: true,
                label,
                segment,
            },
        ));
    }
}

fn arm_agent_loading_on_restart(
    restarted: Query<
        (
            Entity,
            Option<&vmux_core::agent::AgentSession>,
            Option<&PromptCapture>,
        ),
        (
            With<Terminal>,
            With<PageReady>,
            Without<AgentLoading>,
            Changed<ProcessId>,
        ),
    >,
    mut commands: Commands,
) {
    for (entity, session, capture) in &restarted {
        let announced = session.is_some();
        commands
            .entity(entity)
            .insert(AgentLoading::armed(announced));
        if session.is_some() && capture.is_none() {
            commands.entity(entity).insert(PromptCapture::default());
        }
        if let Some(capture) = capture {
            commands.trigger(vmux_core::host::UiStateWrite::<
                vmux_core::event::TerminalUiState,
            >::from_event(
                entity,
                &AgentPromptDraftEvent {
                    draft: capture.draft.clone(),
                    skipped: capture.skipped,
                },
            ));
        }
        if !announced {
            continue;
        }
        let (label, segment) = labels(session);
        commands.trigger(vmux_core::host::UiStateWrite::<
            vmux_core::event::TerminalUiState,
        >::from_event(
            entity,
            &TermLoadingEvent {
                loading: true,
                label,
                segment,
            },
        ));
    }
}

fn clear_agent_loading(
    loading: Query<
        (
            Entity,
            Option<&vmux_core::agent::AgentSession>,
            &AgentLoading,
            Option<&PromptCapture>,
            Has<ShellOutputSeen>,
            Option<&TerminalMode>,
        ),
        With<Terminal>,
    >,
    mut commands: Commands,
) {
    for (entity, session, state, capture, output_seen, mode) in &loading {
        let ready = match session {
            Some(_) => mode.is_some_and(TerminalMode::agent_ready),
            None => output_seen,
        };
        if !ready && state.since.elapsed() < AGENT_LOADING_TIMEOUT {
            continue;
        }
        if let Some(capture) = capture {
            if !capture.skipped && !capture.draft.trim().is_empty() {
                commands.entity(entity).insert(BufferedAgentPrompt {
                    text: capture.draft.clone(),
                    submit: true,
                });
            }
            commands.entity(entity).remove::<PromptCapture>();
        }
        commands.entity(entity).remove::<AgentLoading>();
        if !state.announced {
            continue;
        }
        let (label, segment) = labels(session);
        commands.trigger(vmux_core::host::UiStateWrite::<
            vmux_core::event::TerminalUiState,
        >::from_event(
            entity,
            &TermLoadingEvent {
                loading: false,
                label,
                segment,
            },
        ));
    }
}

fn reset_terminal_title_on_agent_removed(
    mut removed: RemovedComponents<vmux_core::agent::AgentSession>,
    mut terminals: Query<(&ProcessId, &mut vmux_core::PageMetadata), With<Terminal>>,
) {
    for entity in removed.read() {
        if let Ok((process_id, mut metadata)) = terminals.get_mut(entity) {
            let title = format!("Terminal ({})", &process_id.to_string()[..8]);
            if metadata.title != title {
                metadata.title = title;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_core::agent::{AgentKind, AgentSession};

    #[test]
    fn agent_terminal_is_armed_when_the_page_becomes_ready() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, LoadingPlugin));
        let entity = app
            .world_mut()
            .spawn((
                Terminal,
                AgentSession {
                    kind: AgentKind::Vibe,
                },
                PageReady {},
            ))
            .id();

        app.update();

        assert!(app.world().get::<AgentLoading>(entity).is_some());
    }

    #[test]
    fn shell_with_output_does_not_arm_loading() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, LoadingPlugin));
        let entity = app
            .world_mut()
            .spawn((Terminal, ShellOutputSeen, PageReady {}))
            .id();

        app.update();

        assert!(app.world().get::<AgentLoading>(entity).is_none());
    }

    #[test]
    fn plain_terminal_announces_after_the_boot_grace() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, LoadingPlugin));
        let entity = app.world_mut().spawn((Terminal, PageReady {})).id();

        app.update();
        assert!(!app.world().get::<AgentLoading>(entity).unwrap().announced);

        app.world_mut()
            .get_mut::<AgentLoading>(entity)
            .unwrap()
            .since = Instant::now() - SHELL_BOOT_GRACE - Duration::from_millis(1);
        app.update();

        assert!(app.world().get::<AgentLoading>(entity).unwrap().announced);
    }

    #[test]
    fn agent_announces_immediately() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, LoadingPlugin));
        let entity = app
            .world_mut()
            .spawn((
                Terminal,
                AgentSession {
                    kind: AgentKind::Vibe,
                },
                PageReady {},
            ))
            .id();

        app.update();

        assert!(app.world().get::<AgentLoading>(entity).unwrap().announced);
    }

    #[test]
    fn loading_preserves_initial_prompt_capture() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, LoadingPlugin));
        let entity = app
            .world_mut()
            .spawn((
                Terminal,
                AgentSession {
                    kind: AgentKind::Vibe,
                },
                PromptCapture {
                    draft: "@asdfas".to_string(),
                    skipped: false,
                },
                PageReady {},
            ))
            .id();

        app.update();

        let capture = app.world().get::<PromptCapture>(entity).unwrap();
        assert_eq!(capture.draft, "@asdfas");
        assert!(!capture.skipped);
    }

    #[test]
    fn loading_is_armed_after_process_restart() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, LoadingPlugin));
        let entity = app
            .world_mut()
            .spawn((
                Terminal,
                AgentSession {
                    kind: AgentKind::Vibe,
                },
                ProcessId::new(),
                PageReady {},
            ))
            .id();

        app.update();
        assert!(app.world().get::<AgentLoading>(entity).is_some());

        app.world_mut().entity_mut(entity).remove::<AgentLoading>();
        app.update();
        assert!(app.world().get::<AgentLoading>(entity).is_none());

        *app.world_mut().get_mut::<ProcessId>(entity).unwrap() = ProcessId::new();
        app.update();
        assert!(app.world().get::<AgentLoading>(entity).is_some());
    }

    #[test]
    fn loading_clears_when_the_alt_screen_is_active() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, LoadingPlugin));
        let process_id = ProcessId::new();
        let entity = app
            .world_mut()
            .spawn((
                Terminal,
                AgentSession {
                    kind: AgentKind::Vibe,
                },
                process_id,
                AgentLoading {
                    since: Instant::now(),
                    announced: true,
                },
                TerminalMode {
                    alt_screen: true,
                    ..default()
                },
            ))
            .id();

        app.update();

        assert!(app.world().get::<AgentLoading>(entity).is_none());
    }

    fn clear_with_capture(capture: PromptCapture) -> (App, Entity) {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, LoadingPlugin));
        let process_id = ProcessId::new();
        let entity = app
            .world_mut()
            .spawn((
                Terminal,
                AgentSession {
                    kind: AgentKind::Claude,
                },
                process_id,
                AgentLoading {
                    since: Instant::now(),
                    announced: true,
                },
                capture,
                TerminalMode {
                    alt_screen: true,
                    ..default()
                },
            ))
            .id();
        app.update();
        (app, entity)
    }

    #[test]
    fn ready_prompt_is_buffered_for_delivery() {
        let (app, entity) = clear_with_capture(PromptCapture {
            draft: "find me a hotel".to_string(),
            skipped: false,
        });

        assert!(app.world().get::<PromptCapture>(entity).is_none());
        let buffered = app.world().get::<BufferedAgentPrompt>(entity).unwrap();
        assert_eq!(buffered.text, "find me a hotel");
        assert!(buffered.submit);
    }

    #[test]
    fn skipped_prompt_is_not_buffered() {
        let (app, entity) = clear_with_capture(PromptCapture {
            draft: "ignored".to_string(),
            skipped: true,
        });

        assert!(app.world().get::<PromptCapture>(entity).is_none());
        assert!(app.world().get::<BufferedAgentPrompt>(entity).is_none());
    }

    #[test]
    fn loading_clears_after_timeout() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, LoadingPlugin));
        let entity = app
            .world_mut()
            .spawn((
                Terminal,
                AgentSession {
                    kind: AgentKind::Vibe,
                },
                ProcessId::new(),
                AgentLoading {
                    since: Instant::now() - AGENT_LOADING_TIMEOUT - Duration::from_secs(1),
                    announced: true,
                },
            ))
            .id();

        app.update();

        assert!(app.world().get::<AgentLoading>(entity).is_none());
    }

    #[test]
    fn loading_is_retained_while_the_agent_starts() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, LoadingPlugin));
        let entity = app
            .world_mut()
            .spawn((
                Terminal,
                AgentSession {
                    kind: AgentKind::Vibe,
                },
                ProcessId::new(),
                AgentLoading {
                    since: Instant::now(),
                    announced: true,
                },
            ))
            .id();

        app.update();

        assert!(app.world().get::<AgentLoading>(entity).is_some());
    }

    #[test]
    fn plain_terminal_is_armed() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, LoadingPlugin));
        let entity = app.world_mut().spawn((Terminal, PageReady {})).id();

        app.update();

        assert!(app.world().get::<AgentLoading>(entity).is_some());
    }

    #[test]
    fn plain_terminal_loading_waits_for_output() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, LoadingPlugin));
        let entity = app
            .world_mut()
            .spawn((
                Terminal,
                ProcessId::new(),
                AgentLoading {
                    since: Instant::now(),
                    announced: true,
                },
            ))
            .id();

        app.update();

        assert!(app.world().get::<AgentLoading>(entity).is_some());
    }

    #[test]
    fn plain_terminal_loading_clears_after_output() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, LoadingPlugin));
        let entity = app
            .world_mut()
            .spawn((
                Terminal,
                ProcessId::new(),
                AgentLoading {
                    since: Instant::now(),
                    announced: true,
                },
            ))
            .id();

        app.update();
        assert!(app.world().get::<AgentLoading>(entity).is_some());

        app.world_mut().entity_mut(entity).insert(ShellOutputSeen);
        app.update();

        assert!(app.world().get::<AgentLoading>(entity).is_none());
    }

    #[test]
    fn title_resets_when_agent_session_is_removed() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, LoadingPlugin));
        let process_id = ProcessId::new();
        let entity = app
            .world_mut()
            .spawn((
                Terminal,
                process_id,
                vmux_core::PageMetadata {
                    title: "Vibe (abc12345)".to_string(),
                    url: "vmux://sessions/vibe/abc12345".to_string(),
                    icon: vmux_core::PageIcon::None,
                    bg_color: None,
                },
                AgentSession {
                    kind: AgentKind::Vibe,
                },
            ))
            .id();

        app.update();
        app.world_mut().entity_mut(entity).remove::<AgentSession>();
        app.update();

        assert_eq!(
            app.world()
                .get::<vmux_core::PageMetadata>(entity)
                .unwrap()
                .title,
            format!("Terminal ({})", &process_id.to_string()[..8])
        );
    }
}
