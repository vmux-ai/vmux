use super::*;
use serde_json::json;
use std::sync::mpsc;

fn outbox() -> LspOutbox {
    LspOutbox::default()
}
fn pending() -> PendingMap {
    PendingMap::default()
}

#[test]
fn publish_diagnostics_lands_in_outbox() {
    let ob = outbox();
    let pd = pending();
    let msg = json!({
        "jsonrpc": "2.0",
        "method": "textDocument/publishDiagnostics",
        "params": {
            "uri": "file:///tmp/main.rs",
            "diagnostics": [{
                "range": {"start": {"line": 1, "character": 2},
                          "end": {"line": 1, "character": 5}},
                "severity": 1,
                "message": "boom",
                "source": "rustc"
            }]
        }
    });
    dispatch_message(msg, &pd, &ob);
    let q = ob.0.lock().unwrap();
    assert_eq!(q.len(), 1);
    assert_eq!(q[0].0, PathBuf::from("/tmp/main.rs"));
    assert_eq!(q[0].1.len(), 1);
    assert_eq!(q[0].1[0].message, "boom");
}

#[test]
fn response_routes_to_pending_sender() {
    let ob = outbox();
    let pd = pending();
    let (tx, rx) = mpsc::channel();
    pd.lock().unwrap().insert(7, tx);
    dispatch_message(json!({"jsonrpc": "2.0", "id": 7, "result": {}}), &pd, &ob);
    let got = rx.recv_timeout(std::time::Duration::from_secs(1)).unwrap();
    assert_eq!(got["id"], 7);
    assert!(pd.lock().unwrap().is_empty(), "pending entry consumed");
}

#[test]
fn unknown_notification_is_ignored() {
    let ob = outbox();
    let pd = pending();
    dispatch_message(
        json!({"method": "window/logMessage", "params": {}}),
        &pd,
        &ob,
    );
    assert!(ob.0.lock().unwrap().is_empty());
}
