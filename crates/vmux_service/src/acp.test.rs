use super::*;

#[test]
fn manager_routes_input_by_sid() {
    let mgr = AcpSessionManager::default();
    assert!(!mgr.contains("nope"));
    assert!(!mgr.input(
        "nope",
        AcpInput::User {
            text: "x".to_string(),
            context: None,
            attachments: Vec::new(),
        }
    ));
    assert!(mgr.subscribe("nope").is_none());
    assert!(mgr.snapshot("nope").is_none());
    assert!(mgr.agent_info("nope").is_none());
    assert!(mgr.model_info("nope").is_none());
}
