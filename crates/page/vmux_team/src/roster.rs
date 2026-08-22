//! The team roster on a phone: who the paired Mac reports, and the payload the page reads.
//!
//! The desktop builds the same payload from its own world — profiles, agents, run state — in
//! [`host`](crate::host). None of that exists over a relay, so the two share their *output* and
//! nothing else, which is [`TeamEvent`] and already lives in `vmux_wire`.
//!
//! Unlike the desktop's, this projection is an identity: the relay answers in the shape the page
//! wants. What the plugin is for is the two things around it — noticing that a poll returned the
//! same roster as last time, and letting a host emit only when it did not.

use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use vmux_wire::page::PageEmit;
use vmux_wire::team::{TEAM_EVENT, TeamEvent, TeamMemberRow};

/// Keeps [`Team`] current with whatever the app last heard from the Mac, and hands it to the page.
pub struct TeamRosterPlugin;

impl Plugin for TeamRosterPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Members>()
            .init_resource::<Team>()
            .add_message::<PageEmit>()
            .add_systems(
                Update,
                (
                    Team::project
                        .in_set(TeamProjection)
                        .run_if(resource_changed::<Members>),
                    Team::emit
                        .after(TeamProjection)
                        .run_if(resource_changed::<Team>),
                ),
            );
    }
}

/// When [`Team`] is rebuilt, so the emit ordered after it carries what this turn produced rather
/// than what the last one did.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct TeamProjection;

/// Who the paired Mac last said was on the team. Written by the app, read by nothing else.
#[derive(Resource, Default, PartialEq)]
pub struct Members(pub Vec<TeamMemberRow>);

/// The roster payload, as the page expects to be told it.
#[derive(Resource, Default)]
pub struct Team(pub TeamEvent);

impl Team {
    fn project(members: Res<Members>, mut team: ResMut<Team>) {
        team.0 = TeamEvent {
            members: members.0.clone(),
        };
    }

    /// Hand the rebuilt roster to whichever page is listening for it.
    fn emit(team: Res<Team>, mut emits: MessageWriter<PageEmit>) {
        let Some(emit) = PageEmit::of(TEAM_EVENT, &team.0) else {
            return;
        };
        emits.write(emit);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A world running the plugin, so what is asserted is what the schedule produced.
    struct Started(App);

    impl Started {
        fn with(members: Vec<TeamMemberRow>) -> Self {
            let mut app = App::new();
            app.add_plugins(TeamRosterPlugin)
                .insert_resource(Members(members));
            app.update();
            Self(app)
        }

        fn team(&self) -> &TeamEvent {
            &self.0.world().resource::<Team>().0
        }

        fn reroster(&mut self, members: Vec<TeamMemberRow>) {
            self.0.insert_resource(Members(members));
            self.0.update();
        }
    }

    fn member(name: &str) -> TeamMemberRow {
        TeamMemberRow {
            name: name.to_string(),
            ..TeamMemberRow::default()
        }
    }

    #[test]
    fn the_payload_follows_the_roster_it_was_built_from() {
        let mut started = Started::with(Vec::new());
        assert!(started.team().members.is_empty());

        started.reroster(vec![member("ada"), member("grace")]);
        let names: Vec<&str> = started
            .team()
            .members
            .iter()
            .map(|m| m.name.as_str())
            .collect();
        assert_eq!(names, ["ada", "grace"], "in the order the Mac gave them");

        started.reroster(Vec::new());
        assert!(
            started.team().members.is_empty(),
            "a member who left has to leave the page too"
        );
    }
}
