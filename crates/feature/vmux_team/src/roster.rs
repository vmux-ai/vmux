use bevy_app::{App, Plugin, Startup, Update};
use bevy_ecs::prelude::*;
use vmux_api::page::UiStateEmit;
use vmux_api::team::{TeamEvent, TeamMemberRow};

use crate::projection::TeamStateProjection;

pub struct TeamRosterPlugin;

impl Plugin for TeamRosterPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Members>()
            .add_message::<RepublishTeam>()
            .add_message::<UiStateEmit>()
            .add_systems(Startup, spawn_team_runtime)
            .add_systems(
                Update,
                (
                    receive_members.before(TeamProjection),
                    project_team.in_set(TeamProjection),
                    emit_team.after(TeamProjection),
                ),
            );
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct TeamProjection;

#[derive(Component, Message, Clone, Default, PartialEq)]
pub struct Members(pub Vec<TeamMemberRow>);

#[derive(Component, Default)]
pub struct Team(pub TeamEvent);

#[derive(Component)]
struct TeamRuntime;

#[derive(Message)]
pub struct RepublishTeam;

fn spawn_team_runtime(mut commands: Commands) {
    commands.spawn((TeamRuntime, Members::default(), Team::default()));
}

fn receive_members(
    mut messages: MessageReader<Members>,
    mut runtimes: Query<&mut Members, With<TeamRuntime>>,
) {
    let Ok(mut members) = runtimes.single_mut() else {
        return;
    };
    for update in messages.read() {
        if *members != *update {
            *members = update.clone();
        }
    }
}

fn project_team(mut runtimes: Query<(&Members, &mut Team), (With<TeamRuntime>, Changed<Members>)>) {
    let Ok((members, mut team)) = runtimes.single_mut() else {
        return;
    };
    team.0 = TeamStateProjection::build(members.0.clone(), Vec::new());
}

fn emit_team(
    mut refreshes: MessageReader<RepublishTeam>,
    runtimes: Query<Ref<Team>, With<TeamRuntime>>,
    mut emits: MessageWriter<UiStateEmit>,
) {
    let refresh = refreshes.read().next().is_some();
    let Ok(team) = runtimes.single() else {
        return;
    };
    if !refresh && !team.is_changed() {
        return;
    }
    let Some(emit) = UiStateEmit::from_state(&team.0) else {
        return;
    };
    emits.write(emit);
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Started(App);

    impl Started {
        fn with(members: Vec<TeamMemberRow>) -> Self {
            let mut app = App::new();
            app.add_plugins(TeamRosterPlugin);
            app.update();
            let mut started = Self(app);
            started.reroster(members);
            started
        }

        fn team(&self) -> &TeamEvent {
            &self
                .0
                .world()
                .iter_entities()
                .find_map(|entity| entity.get::<Team>())
                .expect("team runtime")
                .0
        }

        fn reroster(&mut self, members: Vec<TeamMemberRow>) {
            self.0.world_mut().write_message(Members(members));
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
        let agents: Vec<&str> = started
            .team()
            .agents
            .iter()
            .map(|agent| agent.member.name.as_str())
            .collect();
        assert_eq!(agents, ["ada", "grace"]);

        started.reroster(Vec::new());
        assert!(
            started.team().members.is_empty(),
            "a member who left has to leave the page too"
        );
    }
}
