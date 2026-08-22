//! The launcher's model on a phone: what the paired Mac has, and the payload the page reads.
//!
//! The desktop builds the same payload out of its own world — spaces, panes, webview entities —
//! in [`host`](crate::host). None of those exist over a relay, so the two projections share their
//! *output* and nothing else. That output is [`CommandBarOpenEvent`], which already lives in
//! `vmux_wire` because both ends had to agree on it anyway.
//!
//! What this deliberately does not have is a way to reach the link. The app fetches the roster and
//! writes it here; this only reads. A plugin in a page crate that could call the phone's `Api`
//! would invert the layering — the app depends on the page, never the other way round.

use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use vmux_wire::command_bar::{CommandBarOpenEvent, CommandBarPage, CommandBarTab, OpenId};
use vmux_wire::page::PageEmit;

use crate::event::START_COMMAND_BAR_OPEN_EVENT;
use vmux_wire::icon::PageIcon;
use vmux_wire::room::{RemoteAgent, RemoteSession};

/// Keeps [`Launcher`] current with whatever the app last heard from the Mac.
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

/// When [`Launcher`] is rebuilt, so the emit ordered after it carries what this turn produced
/// rather than what the last one did.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct LauncherProjection;

/// What the paired Mac last said it had. Written by the app, read by nothing else.
#[derive(Resource, Default, PartialEq)]
pub struct Roster {
    pub sessions: Vec<RemoteSession>,
    pub agents: Vec<RemoteAgent>,
}

/// The launcher payload, as the page expects to be told it.
#[derive(Resource, Default)]
pub struct Launcher(pub CommandBarOpenEvent);

impl Launcher {
    /// Describe the Mac the way the shared launcher expects to be told about it.
    ///
    /// Sessions become the open-stack rows and agents the prompt targets, which is the same shape
    /// the desktop contributes — so the launcher ranks, filters and renders them without knowing
    /// one list came over a relay.
    fn project(roster: Res<Roster>, mut launcher: ResMut<Launcher>) {
        launcher.0 = Self::of(&roster);
    }

    /// Hand the rebuilt launcher to whichever page is listening for it.
    fn emit(launcher: Res<Launcher>, mut emits: MessageWriter<PageEmit>) {
        let Some(emit) = PageEmit::of(START_COMMAND_BAR_OPEN_EVENT, &launcher.0) else {
            return;
        };
        emits.write(emit);
    }

    fn of(roster: &Roster) -> CommandBarOpenEvent {
        let mut tabs = Vec::with_capacity(roster.sessions.len());
        for (index, session) in roster.sessions.iter().enumerate() {
            let cwd = vmux_ui::file_icon::FilePath(&session.cwd).name();
            tabs.push(CommandBarTab {
                title: session.name.clone(),
                url: format!("vmux://agent/{sid}", sid = session.sid),
                pane_id: 0,
                // What comes back on activation, so it has to index the list this was built from.
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
            // Documented as the start page's live-refresh id: reusing it is what stops each
            // refresh reading as a reopen and clobbering what is being typed.
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
    use vmux_wire::room::{RemoteStatus, RoomId};

    impl Roster {
        /// One session and one agent, enough to tell the two halves of the payload apart.
        fn of_one() -> Self {
            Self {
                sessions: vec![Self::session("api")],
                agents: vec![RemoteAgent {
                    id: "claude".into(),
                    name: "Claude".into(),
                    url: "vmux://agent/claude".into(),
                    icon: String::new(),
                }],
            }
        }

        /// Several sessions, so an index that addresses the wrong one has somewhere to show.
        fn of_sessions(names: &[&str]) -> Self {
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

    /// A world running the plugin, so what is asserted is what the schedule produced.
    struct Started(App);

    impl Started {
        fn with(roster: Roster) -> Self {
            let mut app = App::new();
            app.add_plugins(StartRosterPlugin).insert_resource(roster);
            app.update();
            Self(app)
        }

        fn launcher(&self) -> &CommandBarOpenEvent {
            &self.0.world().resource::<Launcher>().0
        }

        fn reroster(&mut self, roster: Roster) {
            self.0.insert_resource(roster);
            self.0.update();
        }
    }

    /// The launcher hands an activated row back by index alone, so the list it was built from is
    /// the only thing that can name the session again. A row whose index stops addressing its own
    /// session opens the wrong conversation — silently, and only for whoever has two of them.
    #[test]
    fn every_session_is_addressed_by_the_index_it_comes_back_as() {
        let roster = Roster::of_sessions(&["alpha", "beta", "gamma"]);
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
        let started = Started::with(Roster::of_one());
        let tab = &started.launcher().tabs[0];

        assert_eq!(tab.url, "vmux://agent/sid-api");
        assert!(
            tab.location.contains("api") && tab.location.contains("claude"),
            "the row names the runtime and the directory: {}",
            tab.location
        );
    }

    /// An agent has to arrive as something the launcher will send a prompt to. Contributed without
    /// this flag it renders as an ordinary row that opens a url, and the phone has no browser to
    /// open one in — so the agent would be listed and unreachable.
    #[test]
    fn an_agent_becomes_a_prompt_target() {
        let started = Started::with(Roster::of_one());
        let pages = &started.launcher().pages;

        assert_eq!(pages.len(), 1);
        assert!(pages[0].prompt_target);
        assert_eq!(pages[0].url, "vmux://agent/claude");
    }

    /// Every rebuild reuses the one id documented as "not a reopen". A real id here would reset
    /// the palette's input on each refresh, deleting whatever was half-typed.
    #[test]
    fn a_refresh_does_not_read_as_a_reopen() {
        let started = Started::with(Roster::of_one());
        assert!(!started.launcher().open_id.is_open());
    }

    #[test]
    fn the_payload_follows_the_roster_it_was_built_from() {
        let mut started = Started::with(Roster::default());
        assert!(started.launcher().tabs.is_empty());

        started.reroster(Roster::of_one());
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
