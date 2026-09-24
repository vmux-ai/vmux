pub const TEAM_PAGE_URL: &str = "vmux://team/";

#[vmux_api::ui_state(Default, Eq, targets = ["team", "layout", "spaces"])]
pub struct TeamEvent {
    pub members: Vec<TeamMemberRow>,
    #[serde(default)]
    pub profiles: Vec<ProfileRow>,
    #[serde(default)]
    pub active_profile: Option<ProfileRow>,
    #[serde(default)]
    pub agents: Vec<TeamAgentPresentation>,
}

#[vmux_api::contract(Default, Eq)]
pub struct ProfileRow {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub color: String,
    pub is_active: bool,
}

#[vmux_api::contract(Default, Eq)]
pub struct TeamAgentPresentation {
    pub member: TeamMemberRow,
    pub subtitle: TeamAgentSubtitle,
}

#[vmux_api::contract(Default, Eq)]
pub enum TeamAgentSubtitle {
    #[default]
    None,
    Role,
    Title(String),
}

#[vmux_api::contract(Default, Eq)]
pub struct TeamMemberRow {
    pub id: String,
    pub name: String,
    pub initials: String,
    pub color: String,
    #[serde(default)]
    pub icon: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub sid: String,
    pub is_user: bool,
    pub is_running: bool,
    #[serde(default)]
    pub is_done_unseen: bool,
}

impl TeamEvent {
    pub fn project(members: Vec<TeamMemberRow>, profiles: Vec<ProfileRow>) -> Self {
        let active_profile = profiles.iter().find(|profile| profile.is_active).cloned();
        let mut agents = Vec::new();
        for member in &members {
            if let Some(agent) = TeamAgentPresentation::project(member) {
                agents.push(agent);
            }
        }
        Self {
            members,
            profiles,
            active_profile,
            agents,
        }
    }
}

impl TeamAgentPresentation {
    fn project(member: &TeamMemberRow) -> Option<Self> {
        if member.is_user {
            return None;
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
        Some(Self {
            member: member.clone(),
            subtitle,
        })
    }
}

#[vmux_api::ui_event(Copy, Default, Eq, targets = ["team", "layout", "spaces"])]
pub struct TeamOpenRequest;

#[vmux_api::ui_event(Default, Eq, targets = ["team", "layout", "spaces"])]
pub struct TeamMemberFocusRequest {
    pub member_id: String,
}

#[vmux_api::ui_event(Default, Eq, targets = ["team", "layout", "spaces"])]
pub struct TeamProfileCreateRequest {
    pub name: String,
}

#[vmux_api::ui_event(Default, Eq, targets = ["team", "layout", "spaces"])]
pub struct TeamProfileSwitchRequest {
    pub profile_id: String,
}

#[vmux_api::ui_event(Default, Eq, targets = ["team", "layout", "spaces"])]
pub struct TeamProfileUpdateRequest {
    pub profile_id: String,
    pub name: String,
}

#[vmux_api::ui_event(Default, Eq, targets = ["team", "layout", "spaces"])]
pub struct TeamRequest {
    pub command: String,
    #[serde(default)]
    pub member_id: Option<String>,
    #[serde(default)]
    pub profile_id: Option<String>,
    #[serde(default)]
    pub profile_name: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn member_row_keeps_flags() {
        let row = TeamMemberRow {
            id: "1".to_string(),
            name: "Claude".to_string(),
            initials: "CL".to_string(),
            color: "#d97757".to_string(),
            icon: String::new(),
            url: String::new(),
            title: String::new(),
            sid: String::new(),
            is_user: false,
            is_running: true,
            is_done_unseen: false,
        };
        assert!(row.is_running && !row.is_user);
    }

    #[test]
    fn team_event_rkyv_roundtrip() {
        let original = TeamEvent {
            members: vec![TeamMemberRow {
                id: "9".to_string(),
                name: "You".to_string(),
                initials: "You".to_string(),
                color: "#3b82f6".to_string(),
                icon: "https://x/favicon.png".to_string(),
                url: "vmux://sessions/vibe/".to_string(),
                title: "Vibe session".to_string(),
                sid: "021fb65c".to_string(),
                is_user: true,
                is_running: false,
                is_done_unseen: true,
            }],
            profiles: vec![ProfileRow {
                id: "personal".to_string(),
                name: "Personal".to_string(),
                color: "#3b82f6".to_string(),
                is_active: true,
            }],
            active_profile: Some(ProfileRow {
                id: "personal".to_string(),
                name: "Personal".to_string(),
                color: "#3b82f6".to_string(),
                is_active: true,
            }),
            agents: Vec::new(),
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&original).expect("serialize");
        let recovered =
            rkyv::from_bytes::<TeamEvent, rkyv::rancor::Error>(&bytes).expect("deserialize");
        assert_eq!(original, recovered);
    }

    #[test]
    fn team_profile_update_request_rkyv_roundtrip() {
        let original = TeamProfileUpdateRequest {
            profile_id: "work".to_string(),
            name: "Work".to_string(),
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&original).expect("serialize");
        let recovered = rkyv::from_bytes::<TeamProfileUpdateRequest, rkyv::rancor::Error>(&bytes)
            .expect("deserialize");
        assert_eq!(original, recovered);
    }

    #[test]
    fn projection_selects_the_active_profile_and_agent_subtitle() {
        let state = TeamEvent::project(
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
