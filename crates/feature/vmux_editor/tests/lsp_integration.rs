use std::path::Path;
use std::time::{Duration, Instant};

use vmux_editor::lsp::client::ServerClient;
use vmux_editor::lsp::registry::ServerSpec;
use vmux_editor::lsp::{LspDiagnosticsInbox, LspDiagnosticsSender};

struct Mock {
    _client: ServerClient,
    diagnostics: LspDiagnosticsInbox,
    _events: crossbeam_channel::Receiver<vmux_editor::lsp::server_request::ServerEvent>,
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

        let (diagnostics, inbox) = LspDiagnosticsSender::channel();
        let (events, event_inbox) = crossbeam_channel::unbounded();
        let client = ServerClient::spawn(&spec, dir, diagnostics, events)
            .expect("mock server spawns and initializes");

        let uri = url::Url::from_file_path(&file).unwrap().to_string();
        client.did_open(&uri, "rust", 1, "fn x() {}\n");

        Self {
            _client: client,
            diagnostics: inbox,
            _events: event_inbox,
        }
    }

    fn await_message(&self, wanted: &str) -> (std::path::PathBuf, String) {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut seen = Vec::new();
        loop {
            for (path, diagnostics) in self.diagnostics.0.try_iter() {
                for diagnostic in diagnostics {
                    if diagnostic.message.contains(wanted) {
                        return (path, diagnostic.message);
                    }
                    seen.push(diagnostic.message);
                }
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
    let (path, _) = mock.await_message("mock diagnostic");
    assert_eq!(path, tmp.path().join("main.rs"));
}

#[test]
fn unimplemented_server_request_is_refused_over_real_pipes() {
    let tmp = tempfile::tempdir().unwrap();
    let mock = Mock::open("probe-requests.rs", tmp.path());
    assert_eq!(mock.await_message("answered").1, "answered -32601");
}
