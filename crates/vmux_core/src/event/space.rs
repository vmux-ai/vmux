pub const SPACES_PAGE_URL: &str = "vmux://spaces/";
pub const SPACES_LIST_EVENT: &str = "spaces_list";

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
pub struct SpacesListEvent {
    pub spaces: Vec<SpaceRow>,
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
pub struct SpaceRow {
    pub id: String,
    pub name: String,
    pub profile: String,
    pub is_active: bool,
    pub tab_count: u32,
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
pub struct SpaceCommandEvent {
    pub command: String,
    #[serde(default)]
    pub space_id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn space_row_keeps_profile_and_active_state() {
        let row = SpaceRow {
            id: "work".to_string(),
            name: "Work".to_string(),
            profile: "Personal".to_string(),
            is_active: true,
            tab_count: 3,
        };
        assert_eq!(row.profile, "Personal");
        assert!(row.is_active);
        assert_eq!(row.tab_count, 3);
    }

    #[test]
    fn attach_event_carries_target_space_id() {
        let event = SpaceCommandEvent {
            command: "attach".to_string(),
            space_id: Some("work".to_string()),
            name: None,
        };
        assert_eq!(event.space_id.as_deref(), Some("work"));
    }

    #[test]
    fn space_command_event_rkyv_roundtrip() {
        let original = SpaceCommandEvent {
            command: "attach".to_string(),
            space_id: Some("work".to_string()),
            name: Some("Work".to_string()),
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&original).expect("serialize");
        let recovered = rkyv::from_bytes::<SpaceCommandEvent, rkyv::rancor::Error>(&bytes)
            .expect("deserialize");
        assert_eq!(original, recovered);
    }

    #[test]
    fn spaces_list_event_rkyv_roundtrip() {
        let original = SpacesListEvent {
            spaces: vec![SpaceRow {
                id: "work".to_string(),
                name: "Work".to_string(),
                profile: "Personal".to_string(),
                is_active: true,
                tab_count: 2,
            }],
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&original).expect("serialize");
        let recovered =
            rkyv::from_bytes::<SpacesListEvent, rkyv::rancor::Error>(&bytes).expect("deserialize");
        assert_eq!(original, recovered);
    }
}
