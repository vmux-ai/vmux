use super::*;

fn cats(camera: bool, microphone: bool, screen: bool) -> RequestCategories {
    RequestCategories {
        camera,
        microphone,
        screen,
    }
}

fn store_with(origin: &str, perms: OriginPermissions) -> MediaPermissionStore {
    let mut store = MediaPermissionStore::default();
    store.origins.insert(origin.to_string(), perms);
    store
}

#[test]
fn unknown_origin_prompts() {
    let store = MediaPermissionStore::default();
    assert!(matches!(
        store.decision_for("https://meet.google.com", cats(false, true, false)),
        Resolution::Prompt
    ));
}

#[test]
fn all_requested_categories_allowed_grants() {
    let store = store_with(
        "https://meet.google.com",
        OriginPermissions {
            camera: Some(PermissionDecision::Allow),
            microphone: Some(PermissionDecision::Allow),
            screen: None,
        },
    );
    assert!(matches!(
        store.decision_for("https://meet.google.com", cats(true, true, false)),
        Resolution::Grant
    ));
}

#[test]
fn any_blocked_requested_category_denies() {
    let store = store_with(
        "https://meet.google.com",
        OriginPermissions {
            camera: Some(PermissionDecision::Allow),
            microphone: Some(PermissionDecision::Block),
            screen: None,
        },
    );
    assert!(matches!(
        store.decision_for("https://meet.google.com", cats(true, true, false)),
        Resolution::Deny
    ));
}

#[test]
fn stored_decision_for_other_category_still_prompts() {
    let store = store_with(
        "https://meet.google.com",
        OriginPermissions {
            camera: Some(PermissionDecision::Allow),
            microphone: None,
            screen: None,
        },
    );
    assert!(matches!(
        store.decision_for("https://meet.google.com", cats(true, true, false)),
        Resolution::Prompt
    ));
}

#[test]
fn record_sets_only_requested_categories() {
    let mut store = MediaPermissionStore::default();
    store.record("https://meet.google.com", cats(true, true, false), true);
    let entry = store.origins.get("https://meet.google.com").unwrap();
    assert_eq!(entry.camera, Some(PermissionDecision::Allow));
    assert_eq!(entry.microphone, Some(PermissionDecision::Allow));
    assert_eq!(entry.screen, None);
}

#[test]
fn permission_host_strips_scheme_and_path() {
    assert_eq!(
        permission_host("https://meet.google.com/abc"),
        "meet.google.com"
    );
    assert_eq!(permission_host("meet.google.com"), "meet.google.com");
}
