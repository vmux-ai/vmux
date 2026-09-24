use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use vmux_api::command_bar::{
    CommandBarOpenEvent, CommandBarPage, CommandBarTab, CommandBarUiState, OpenId,
};
use vmux_api::page::PageEmit;

use vmux_api::icon::PageIcon;
use vmux_api::room::{RemoteAgent, RemoteSession};

pub struct StartRosterPlugin;

impl Plugin for StartRosterPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Roster>()
            .init_resource::<Launcher>()
            .add_message::<PageEmit>()
            .add_systems(
                Update,
                (
                    Launcher::project
                        .in_set(LauncherProjection)
                        .run_if(resource_changed::<Roster>),
                    Launcher::emit
                        .after(LauncherProjection)
                        .run_if(resource_changed::<Launcher>),
                ),
            );
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct LauncherProjection;

#[derive(Resource, Default, PartialEq)]
pub struct Roster {
    pub sessions: Vec<RemoteSession>,
    pub agents: Vec<RemoteAgent>,
}

#[derive(Resource, Default)]
pub struct Launcher {
    snapshot: CommandBarOpenEvent,
    sequence: u64,
}

impl Launcher {
    fn project(roster: Res<Roster>, mut launcher: ResMut<Launcher>) {
        launcher.snapshot = Self::snapshot(&roster);
    }

    fn emit(mut launcher: ResMut<Launcher>, mut emits: MessageWriter<PageEmit>) {
        launcher.sequence = launcher.sequence.wrapping_add(1).max(1);
        let state = CommandBarUiState {
            sequence: launcher.sequence,
            patches: vec![launcher.snapshot.clone().into()],
        };
        let Some(emit) = PageEmit::from_event(&state) else {
            return;
        };
        emits.write(emit);
    }

    fn snapshot(roster: &Roster) -> CommandBarOpenEvent {
        let mut tabs = Vec::with_capacity(roster.sessions.len());
        for (index, session) in roster.sessions.iter().enumerate() {
            let cwd = vmux_ui::file_icon::FilePath(&session.cwd).name();
            tabs.push(CommandBarTab {
                title: session.name.clone(),
                url: format!("vmux://sessions/{sid}", sid = session.sid),
                pane_id: 0,
                tab_index: index as u32,
                is_active: false,
                location: vmux_ui::i18n::translate_with(
                    "mobile-start-tab-location",
                    &[
                        (
                            "runtime",
                            vmux_ui::i18n::TranslationValue::String(&session.runtime),
                        ),
                        ("cwd", vmux_ui::i18n::TranslationValue::String(cwd)),
                    ],
                ),
            });
        }
        let mut pages = Vec::with_capacity(roster.agents.len());
        for agent in &roster.agents {
            pages.push(CommandBarPage {
                host: agent.id.clone(),
                url: agent.url.clone(),
                title: agent.name.clone(),
                keywords: Vec::new(),
                icon: PageIcon::favicon(agent.icon.clone()),
                shortcut: String::new(),
                prompt_target: true,
            });
        }
        CommandBarOpenEvent {
            open_id: OpenId::NONE,
            tabs,
            pages,
            ..CommandBarOpenEvent::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_api::room::{RemoteStatus, RoomId};

    impl Roster {
        fn one() -> Self {
            Self {
                sessions: vec![Self::session("api")],
                agents: vec![RemoteAgent {
                    id: "claude".into(),
                    name: "Claude".into(),
                    url: "vmux://sessions/claude".into(),
                    icon: String::new(),
                }],
            }
        }

        fn with_sessions(names: &[&str]) -> Self {
            Self {
                sessions: names.iter().map(|name| Self::session(name)).collect(),
                agents: Vec::new(),
            }
        }

        fn session(name: &str) -> RemoteSession {
            RemoteSession {
                sid: format!("sid-{name}"),
                room_id: RoomId::for_session(name),
                title: String::new(),
                name: name.into(),
                runtime: "claude".into(),
                model: None,
                cwd: format!("/src/{name}"),
                status: RemoteStatus::Idle,
                approval: None,
                created_at_ms: 0,
            }
        }
    }

    struct Started(App);

    impl Started {
        fn with(roster: Roster) -> Self {
            let mut app = App::new();
            app.add_plugins(StartRosterPlugin).insert_resource(roster);
            app.update();
            Self(app)
        }

        fn launcher(&self) -> &CommandBarOpenEvent {
            &self.0.world().resource::<Launcher>().snapshot
        }

        fn reroster(&mut self, roster: Roster) {
            self.0.insert_resource(roster);
            self.0.update();
        }
    }

    #[test]
    fn every_session_is_addressed_by_the_index_it_comes_back_as() {
        let roster = Roster::with_sessions(&["alpha", "beta", "gamma"]);
        let names: Vec<String> = roster.sessions.iter().map(|s| s.name.clone()).collect();
        let started = Started::with(roster);
        let tabs = &started.launcher().tabs;

        assert_eq!(tabs.len(), 3);
        for tab in tabs {
            assert_eq!(
                tab.title,
                names[tab.tab_index as usize],
                "row {index} must address the session it was built from",
                index = tab.tab_index
            );
        }
    }

    #[test]
    fn a_session_row_says_where_the_session_is() {
        let started = Started::with(Roster::one());
        let tab = &started.launcher().tabs[0];

        assert_eq!(tab.url, "vmux://sessions/sid-api");
        assert!(
            tab.location.contains("api") && tab.location.contains("claude"),
            "the row names the runtime and the directory: {}",
            tab.location
        );
    }

    #[test]
    fn an_agent_becomes_a_prompt_target() {
        let started = Started::with(Roster::one());
        let pages = &started.launcher().pages;

        assert_eq!(pages.len(), 1);
        assert!(pages[0].prompt_target);
        assert_eq!(pages[0].url, "vmux://sessions/claude");
    }

    #[test]
    fn a_refresh_does_not_read_as_a_reopen() {
        let started = Started::with(Roster::one());
        assert!(!started.launcher().open_id.is_open());
    }

    #[test]
    fn the_payload_follows_the_roster_it_was_built_from() {
        let mut started = Started::with(Roster::default());
        assert!(started.launcher().tabs.is_empty());

        started.reroster(Roster::one());
        assert_eq!(
            started.launcher().tabs.len(),
            1,
            "a new roster must be seen"
        );

        started.reroster(Roster::default());
        assert!(
            started.launcher().tabs.is_empty(),
            "a session that went away must leave the launcher"
        );
    }
}
