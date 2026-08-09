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
            url: "vmux://agent/vibe/".to_string(),
            title: "Vibe session".to_string(),
            sid: "021fb65c".to_string(),
            is_user: true,
            is_running: false,
            is_done_unseen: true,
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
