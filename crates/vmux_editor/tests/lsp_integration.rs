use std::time::{Duration, Instant};

use vmux_editor::lsp::client::ServerClient;
use vmux_editor::lsp::registry::ServerSpec;
use vmux_editor::lsp::LspOutbox;

#[test]
fn mock_server_handshake_and_diagnostics() {
    let mock = env!("CARGO_BIN_EXE_vmux_mock_lsp");
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("main.rs");
    std::fs::write(&file, "fn x() {}\n").unwrap();

    let spec = ServerSpec {
        command: mock.to_string(),
        args: vec![],
        language_id: "rust".into(),
        root_markers: vec![".git".into()],
    };

    let outbox = LspOutbox::default();
    // spawn() runs the initialize/initialized handshake; Ok means it completed.
    let client = ServerClient::spawn(&spec, tmp.path(), outbox.clone())
        .expect("mock server spawns and initializes");

    let uri = url::Url::from_file_path(&file).unwrap().to_string();
    client.did_open(&uri, "rust", 1, "fn x() {}\n");

    // Poll the outbox until the mock's publishDiagnostics arrives.
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some((path, diags)) = outbox.0.lock().unwrap().first().cloned() {
            assert_eq!(path, file);
            assert_eq!(diags.len(), 1);
            assert_eq!(diags[0].message, "mock diagnostic");
            return;
        }
        assert!(Instant::now() < deadline, "no diagnostics within timeout");
        std::thread::sleep(Duration::from_millis(20));
    }
}
