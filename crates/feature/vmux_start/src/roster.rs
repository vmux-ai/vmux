use bevy_app::{App, Startup, Update};
use bevy_ecs::prelude::*;
use vmux_api::command_bar::{
    CommandBarOpenEvent, CommandBarPage, CommandBarTab, CommandBarUiState, OpenId,
};
use vmux_api::page::UiStateEmit;

use vmux_api::icon::PageIcon;
use vmux_api::room::{RemoteAgent, RemoteSession};

pub struct Plugin;

impl bevy_app::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Roster>()
            .add_message::<RepublishLauncher>()
            .add_message::<UiStateEmit>()
            .add_systems(Startup, spawn)
            .add_systems(
                Update,
                (
                    receive.before(Projection),
                    project.in_set(Projection),
                    emit.after(Projection),
                ),
            );
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct Projection;

#[derive(Component, Message, Clone, Default, PartialEq)]
pub struct Roster {
    pub sessions: Vec<RemoteSession>,
    pub agents: Vec<RemoteAgent>,
}

#[derive(Component, Default)]
pub struct Launcher {
    snapshot: CommandBarOpenEvent,
    sequence: u64,
}

#[derive(Component)]
struct Runtime;

#[derive(Message)]
pub struct RepublishLauncher;

fn spawn(mut commands: Commands) {
    commands.spawn((Runtime, Roster::default(), Launcher::default()));
}

fn receive(mut messages: MessageReader<Roster>, mut runtimes: Query<&mut Roster, With<Runtime>>) {
    let Ok(mut roster) = runtimes.single_mut() else {
        return;
    };
    for update in messages.read() {
        if *roster != *update {
            *roster = update.clone();
        }
    }
}

fn project(mut runtimes: Query<(&Roster, &mut Launcher), (With<Runtime>, Changed<Roster>)>) {
    let Ok((roster, mut launcher)) = runtimes.single_mut() else {
        return;
    };
    launcher.snapshot = Launcher::snapshot(roster);
}

fn emit(
    mut refreshes: MessageReader<RepublishLauncher>,
    mut runtimes: Query<&mut Launcher, With<Runtime>>,
    mut emits: MessageWriter<UiStateEmit>,
) {
    let refresh = refreshes.read().next().is_some();
    let Ok(mut launcher) = runtimes.single_mut() else {
        return;
    };
    if !refresh && !launcher.is_changed() {
        return;
    }
    launcher.sequence = launcher.sequence.wrapping_add(1).max(1);
    let state = CommandBarUiState {
        sequence: launcher.sequence,
        patches: vec![launcher.snapshot.clone().into()],
    };
    let Some(emit) = UiStateEmit::from_state(&state) else {
        return;
    };
    emits.write(emit);
}

impl Launcher {
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
            app.add_plugins(Plugin);
            app.update();
            let mut started = Self(app);
            started.reroster(roster);
            started
        }

        fn launcher(&self) -> &CommandBarOpenEvent {
            &self
                .0
                .world()
                .iter_entities()
                .find_map(|entity| entity.get::<Launcher>())
                .expect("start roster runtime")
                .snapshot
        }

        fn reroster(&mut self, roster: Roster) {
            self.0.world_mut().write_message(roster);
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
