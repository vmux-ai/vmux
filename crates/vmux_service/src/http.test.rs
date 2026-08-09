use super::*;
use crossbeam_channel::unbounded;

fn echo_parse(payload: &str) -> Option<StreamEvent> {
    payload
        .strip_prefix("data: ")
        .map(|s| StreamEvent::TextDelta(s.to_string()))
}

#[tokio::test(flavor = "current_thread")]
async fn drives_two_text_deltas_from_mock_server() {
    let mut server = mockito::Server::new_async().await;
    let body = "data: hello\n\ndata: world\n\n";
    let _m = server
        .mock("POST", "/test")
        .with_status(200)
        .with_header("content-type", "text/event-stream")
        .with_body(body)
        .create_async()
        .await;
    let req = reqwest::Client::new()
        .post(format!("{}/test", server.url()))
        .build()
        .unwrap();
    let (tx, rx) = unbounded::<StreamEvent>();
    drive_sse(req, echo_parse, tx).await;
    let collected: Vec<StreamEvent> = rx.try_iter().collect();
    assert_eq!(
        collected,
        vec![
            StreamEvent::TextDelta("hello".into()),
            StreamEvent::TextDelta("world".into())
        ]
    );
}

#[tokio::test(flavor = "current_thread")]
async fn http_error_status_emits_error_event() {
    let mut server = mockito::Server::new_async().await;
    let _m = server
        .mock("POST", "/fail")
        .with_status(401)
        .with_body("unauthorized")
        .create_async()
        .await;
    let req = reqwest::Client::new()
        .post(format!("{}/fail", server.url()))
        .build()
        .unwrap();
    let (tx, rx) = unbounded::<StreamEvent>();
    drive_sse(req, echo_parse, tx).await;
    let collected: Vec<StreamEvent> = rx.try_iter().collect();
    assert_eq!(collected.len(), 1);
    match &collected[0] {
        StreamEvent::Error(msg) => {
            assert!(msg.contains("401"));
            assert!(msg.contains("unauthorized"));
        }
        other => panic!("expected Error, got {other:?}"),
    }
}
