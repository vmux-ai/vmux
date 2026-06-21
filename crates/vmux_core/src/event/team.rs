pub const TEAM_PAGE_URL: &str = "vmux://team/";
pub const TEAM_EVENT: &str = "team";

#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct TeamEvent {
    pub members: Vec<TeamMemberRow>,
}

#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct TeamMemberRow {
    pub id: String,
    pub name: String,
    pub initials: String,
    pub color: String,
    pub is_user: bool,
    pub is_active: bool,
    pub is_running: bool,
}

#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct TeamCommandEvent {
    pub command: String,
    #[serde(default)]
    pub member_id: Option<String>,
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
            is_user: false,
            is_active: true,
            is_running: true,
        };
        assert!(row.is_active && row.is_running && !row.is_user);
    }

    #[test]
    fn team_event_rkyv_roundtrip() {
        let original = TeamEvent {
            members: vec![TeamMemberRow {
                id: "9".to_string(),
                name: "You".to_string(),
                initials: "You".to_string(),
                color: "#3b82f6".to_string(),
                is_user: true,
                is_active: true,
                is_running: false,
            }],
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&original).expect("serialize");
        let recovered =
            rkyv::from_bytes::<TeamEvent, rkyv::rancor::Error>(&bytes).expect("deserialize");
        assert_eq!(original, recovered);
    }

    #[test]
    fn team_command_event_rkyv_roundtrip() {
        let original = TeamCommandEvent {
            command: "activate".to_string(),
            member_id: Some("42".to_string()),
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&original).expect("serialize");
        let recovered =
            rkyv::from_bytes::<TeamCommandEvent, rkyv::rancor::Error>(&bytes).expect("deserialize");
        assert_eq!(original, recovered);
    }
}
