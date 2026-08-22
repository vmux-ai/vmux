use std::path::Path;
use std::time::{Duration, Instant};

use vmux_editor::lsp::LspOutbox;
use vmux_editor::lsp::client::ServerClient;
use vmux_editor::lsp::registry::ServerSpec;
use vmux_editor::lsp::server_request::ServerEvents;

struct Mock {
    _client: ServerClient,
    outbox: LspOutbox,
    _events: ServerEvents,
}

impl Mock {
    fn open(name: &str, dir: &Path) -> Self {
        let file = dir.join(name);
        std::fs::write(&file, "fn x() {}\n").unwrap();

        let spec = ServerSpec {
            command: env!("CARGO_BIN_EXE_vmux_mock_lsp").to_string(),
            args: vec![],
            language_id: "rust".into(),
            root_markers: vec![".git".into()],
        };

        let outbox = LspOutbox::default();
        let events = ServerEvents::default();
        let client = ServerClient::spawn(&spec, dir, outbox.clone(), events.sender())
            .expect("mock server spawns and initializes");

        let uri = url::Url::from_file_path(&file).unwrap().to_string();
        client.did_open(&uri, "rust", 1, "fn x() {}\n");

        Self {
            _client: client,
            outbox,
            _events: events,
        }
    }

    fn await_message(&self, wanted: &str) -> String {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let seen: Vec<String> = self
                .outbox
                .0
                .lock()
                .unwrap()
                .iter()
                .flat_map(|(_, diags)| diags.iter().map(|d| d.message.clone()))
                .collect();
            if let Some(found) = seen.iter().find(|m| m.contains(wanted)) {
                return found.clone();
            }
            assert!(
                Instant::now() < deadline,
                "no {wanted:?} within timeout; saw {seen:?}"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

#[test]
fn mock_server_handshake_and_diagnostics() {
    let tmp = tempfile::tempdir().unwrap();
    let mock = Mock::open("main.rs", tmp.path());
    mock.await_message("mock diagnostic");

    let (path, _) = mock.outbox.0.lock().unwrap().first().cloned().unwrap();
    assert_eq!(path, tmp.path().join("main.rs"));
}

/// The client must answer a request it does not implement. Left unanswered, the id stays
/// pending in the server forever and some servers serialise behind it.
#[test]
fn unimplemented_server_request_is_refused_over_real_pipes() {
    let tmp = tempfile::tempdir().unwrap();
    let mock = Mock::open("probe-requests.rs", tmp.path());
    assert_eq!(mock.await_message("answered"), "answered -32601");
}
