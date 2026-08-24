use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use vmux_wire::page::PageEmit;
use vmux_wire::team::{TEAM_EVENT, TeamEvent, TeamMemberRow};

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

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct TeamProjection;

#[derive(Resource, Default, PartialEq)]
pub struct Members(pub Vec<TeamMemberRow>);

#[derive(Resource, Default)]
pub struct Team(pub TeamEvent);

impl Team {
    fn project(members: Res<Members>, mut team: ResMut<Team>) {
        team.0 = TeamEvent {
            members: members.0.clone(),
        };
    }

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
