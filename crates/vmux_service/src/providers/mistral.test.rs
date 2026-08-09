use super::*;

#[test]
fn build_request_sets_headers_and_url() {
    let msgs = vec![Message::user("hi")];
    let req = build_request("devstral-2", &msgs, &[], "test-key");
    assert_eq!(req.url().as_str(), ENDPOINT);
    let auth = req
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    assert_eq!(auth, "Bearer test-key");
    let body = req.body().unwrap().as_bytes().unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(body).unwrap();
    assert_eq!(parsed["model"], "devstral-2");
    assert_eq!(parsed["stream"], true);
    assert_eq!(parsed["messages"][0]["role"], "user");
}

#[test]
fn parse_sse_event_delegates_to_shared_parser() {
    let frame = r#"data: {"id":"c1","choices":[{"index":0,"delta":{"content":"hi"},"finish_reason":null}]}"#;
    assert_eq!(parse_sse(frame), Some(StreamEvent::TextDelta("hi".into())));
}
