use super::*;

#[test]
fn hello_round_trips_as_tagged_json() {
    let message = BridgeClientMessage::Hello(BridgeHello {
        protocol_version: BRIDGE_PROTOCOL_VERSION,
        extension_id: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
        profile_id: "personal".into(),
        token: "secret".into(),
        context_id: "bridge-page".into(),
        context_kind: ExtensionContextKind::BridgePage,
    });
    let json = serde_json::to_string(&message).unwrap();
    assert!(json.contains("hello"));
    assert_eq!(
        serde_json::from_str::<BridgeClientMessage>(&json).unwrap(),
        message
    );
}

#[test]
fn api_response_has_exactly_one_result_channel() {
    let response = ApiResponse::success("r1", serde_json::json!({ "ok": true }));
    response.validate().unwrap();
    assert!(
        ApiResponse {
            request_id: "r2".into(),
            result: Some(serde_json::Value::Null),
            error: Some(ChromeError::new("invalid", "bad")),
        }
        .validate()
        .is_err()
    );
}

#[test]
fn heartbeat_round_trips_as_tagged_json() {
    let message = BridgeServerMessage::Heartbeat;
    let json = serde_json::to_string(&message).unwrap();

    assert_eq!(
        serde_json::from_str::<BridgeServerMessage>(&json).unwrap(),
        message
    );
}
