use vmux_api::team::{
    ProfileRow, TeamAgentPresentation, TeamAgentSubtitle, TeamEvent, TeamMemberRow,
};

pub(crate) struct TeamStateProjection;

impl TeamStateProjection {
    pub(crate) fn build(members: Vec<TeamMemberRow>, profiles: Vec<ProfileRow>) -> TeamEvent {
        let active_profile = profiles.iter().find(|profile| profile.is_active).cloned();
        let mut agents = Vec::new();
        for member in &members {
            if member.is_user {
                continue;
            }
            let default_title = format!("{} (", member.name);
            let subtitle = if !member.title.is_empty()
                && member.title != member.name
                && !member.title.starts_with(&default_title)
            {
                TeamAgentSubtitle::Title(member.title.clone())
            } else if member.sid.is_empty() {
                TeamAgentSubtitle::Role
            } else {
                TeamAgentSubtitle::None
            };
            agents.push(TeamAgentPresentation {
                member: member.clone(),
                subtitle,
            });
        }
        TeamEvent {
            members,
            profiles,
            active_profile,
            agents,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_the_active_profile_and_agent_subtitle() {
        let state = TeamStateProjection::build(
            vec![
                TeamMemberRow {
                    name: "You".to_string(),
                    is_user: true,
                    ..TeamMemberRow::default()
                },
                TeamMemberRow {
                    name: "Codex".to_string(),
                    title: "Reviewing the diff".to_string(),
                    ..TeamMemberRow::default()
                },
            ],
            vec![
                ProfileRow {
                    id: "work".to_string(),
                    name: "Work".to_string(),
                    color: "#3b82f6".to_string(),
                    is_active: true,
                },
                ProfileRow {
                    id: "personal".to_string(),
                    name: "Personal".to_string(),
                    color: "#8b5cf6".to_string(),
                    is_active: false,
                },
            ],
        );

        assert_eq!(state.active_profile.as_ref().unwrap().id, "work");
        assert_eq!(state.agents.len(), 1);
        assert_eq!(
            state.agents[0].subtitle,
            TeamAgentSubtitle::Title("Reviewing the diff".to_string())
        );
    }
}
