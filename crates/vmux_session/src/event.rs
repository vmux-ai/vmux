pub const SESSIONS_WEBVIEW_URL: &str = "vmux://sessions/";
pub const SESSIONS_LIST_EVENT: &str = "sessions_list";

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
pub struct SessionsListEvent {
    pub sessions: Vec<SessionRow>,
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
pub struct SessionRow {
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
pub struct SessionCommandEvent {
    pub command: String,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_row_keeps_profile_and_active_state() {
        let row = SessionRow {
            id: "work".to_string(),
            name: "Work".to_string(),
            profile: "default".to_string(),
            is_active: true,
            tab_count: 3,
        };
        assert_eq!(row.profile, "default");
        assert!(row.is_active);
        assert_eq!(row.tab_count, 3);
    }

    #[test]
    fn attach_event_carries_target_session_id() {
        let event = SessionCommandEvent {
            command: "attach".to_string(),
            session_id: Some("work".to_string()),
            name: None,
        };
        assert_eq!(event.session_id.as_deref(), Some("work"));
    }

    #[test]
    fn session_command_event_rkyv_roundtrip() {
        let original = SessionCommandEvent {
            command: "attach".to_string(),
            session_id: Some("work".to_string()),
            name: Some("Work".to_string()),
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&original).expect("serialize");
        let recovered = rkyv::from_bytes::<SessionCommandEvent, rkyv::rancor::Error>(&bytes)
            .expect("deserialize");
        assert_eq!(original, recovered);
    }

    #[test]
    fn sessions_list_event_rkyv_roundtrip() {
        let original = SessionsListEvent {
            sessions: vec![SessionRow {
                id: "work".to_string(),
                name: "Work".to_string(),
                profile: "default".to_string(),
                is_active: true,
                tab_count: 2,
            }],
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&original).expect("serialize");
        let recovered = rkyv::from_bytes::<SessionsListEvent, rkyv::rancor::Error>(&bytes)
            .expect("deserialize");
        assert_eq!(original, recovered);
    }
}
