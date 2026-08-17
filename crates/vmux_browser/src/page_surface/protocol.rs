//! Serving a natively-hosted page, and hearing it back.
//!
//! Two directions over one webview. `vmux://` in: the shell document, the `__events` verdict the
//! page blocks on, and every asset, which resolve through the same Bevy systems that answer CEF.
//! wry's string IPC out: the interpreter's own handshake, the caret the document volunteers, the
//! console, and the rkyv envelopes a page emits.

use bevy::prelude::*;
use bevy_cef_core::prelude::{
    BinIpcEventRaw, CefRequest, CefResponse, Requester, Responser,
    asset_load_path_from_request_url, embedded_page_host_of,
};

use super::SurfacePage;
use super::dom::SurfaceDom;

/// `vmux://` for the wry view, answered by the same Bevy systems that answer it for CEF.
pub(super) struct VmuxProtocol;

impl VmuxProtocol {
    /// The responder is handed to a thread rather than awaited here: the reply comes from a Bevy
    /// system, and this runs on the main thread, so blocking would stop the schedule that produces
    /// it and deadlock.
    /// Whether this asks for the page itself rather than something it references.
    fn is_document_request(url: &str) -> bool {
        let path = url
            .split_once("://")
            .map(|(_, rest)| rest)
            .unwrap_or(url)
            .split(['?', '#'])
            .next()
            .unwrap_or("");
        let path = path.split_once('/').map(|(_, rest)| rest).unwrap_or("");

        path.is_empty() || path == "/" || path == "index.html"
    }

    /// The document a natively-hosted page loads: the interpreter, and nothing else.
    ///
    /// The chrome below is the bundle's own `index.html` with the wasm removed. It is not
    /// decoration: without `index.css` nothing has a Tailwind rule, and without the height and
    /// flex rules on `html`, `body` and the root, a flex child has no box to fill — which renders
    /// as one icon at its intrinsic size filling the window.
    fn shell_response(page: &SurfacePage) -> wry::http::Response<Vec<u8>> {
        let html = vmux_dioxus::InterpreterShell::new(page.root_id, page.url)
            .with_head(page.head)
            .with_html_attributes(page.html_attributes)
            .with_body_class(page.body_class)
            .with_root_class(page.root_class)
            .html();

        wry::http::Response::builder()
            .header(wry::http::header::CONTENT_TYPE, "text/html")
            .body(html.into_bytes())
            .unwrap_or_else(|_| wry::http::Response::new(Vec::new()))
    }

    /// Answer the synchronous request the page is blocked on.
    fn answer_event(
        dom: &SurfaceDom,
        request: &wry::http::Request<Vec<u8>>,
        responder: wry::RequestAsyncResponder,
    ) {
        let header = request
            .headers()
            .get("dioxus-data")
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default();
        let body = dom.handle_event(header).response_bytes();
        let response = wry::http::Response::builder()
            .header(wry::http::header::CONTENT_TYPE, "application/json")
            .body(body)
            .unwrap_or_else(|_| wry::http::Response::new(Vec::new()));

        responder.respond(response);
    }

    pub(super) fn serve(
        page: &SurfacePage,
        dom: &SurfaceDom,
        requester: &Requester,
        request: wry::http::Request<Vec<u8>>,
        responder: wry::RequestAsyncResponder,
    ) {
        let url = request.uri().to_string();

        // Both branches must come before `asset_load_path_from_request_url`. Neither path has a
        // file extension, so it would map them to the host's default document and hand the page
        // HTML where it expects JSON — or the wasm bundle where it expects the shell.
        if url.trim_end_matches('/').ends_with("/__events") {
            return Self::answer_event(dom, &request, responder);
        }
        if Self::is_document_request(&url) {
            return responder.respond(Self::shell_response(page));
        }

        let uri = asset_load_path_from_request_url(&url);
        if uri.is_empty() {
            error!("page_surface: vmux:// url maps to no asset path, url={url}");
            responder.respond(Self::error_response("no asset path for url"));
            return;
        }
        let (tx, rx) = async_channel::bounded::<CefResponse>(1);
        if requester
            .send_blocking(CefRequest {
                uri: uri.clone(),
                responser: Responser(tx),
            })
            .is_err()
        {
            error!("page_surface: vmux:// request channel closed, uri={uri}");
            responder.respond(Self::error_response("request channel closed"));
            return;
        }
        std::thread::spawn(move || match rx.recv_blocking() {
            Ok(response) => {
                let built = wry::http::Response::builder()
                    .status(response.status_code as u16)
                    .header(wry::http::header::CONTENT_TYPE, response.mime_type)
                    .body(response.data);
                match built {
                    Ok(built) => responder.respond(built),
                    Err(error) => {
                        error!("page_surface: vmux:// response invalid uri={uri}: {error}");
                        responder.respond(Self::error_response("response invalid"));
                    }
                }
            }
            Err(_) => {
                error!("page_surface: vmux:// responder dropped, uri={uri}");
                responder.respond(Self::error_response("responder dropped"));
            }
        });
    }

    fn error_response(reason: &str) -> wry::http::Response<Vec<u8>> {
        wry::http::Response::builder()
            .status(500)
            .header(wry::http::header::CONTENT_TYPE, "text/plain")
            .body(reason.as_bytes().to_vec())
            .expect("a literal status and body always build")
    }
}

/// One page-to-host message, as it arrives over wry's string IPC.
///
/// `window.cef.binEmit` takes an `ArrayBuffer`; wry's IPC carries text, so the shim base64s the
/// same `BinIpcEnvelope` bytes and this undoes it. The envelope framing is left alone, because
/// the Bevy side matches its id with `bin_ipc_event_id::<E>()` and that has to keep agreeing.
pub(super) struct PageMessage {
    bin_ipc: async_channel::Sender<BinIpcEventRaw>,
    webview: Entity,
    host: String,
    name: &'static str,
    dom: SurfaceDom,
}

impl PageMessage {
    pub(super) fn new(
        page: &SurfacePage,
        bin_ipc: async_channel::Sender<BinIpcEventRaw>,
        webview: Entity,
        dom: SurfaceDom,
    ) -> Self {
        let host = embedded_page_host_of(page.url).unwrap_or_default();
        Self {
            bin_ipc,
            webview,
            host,
            name: page.url,
            dom,
        }
    }

    pub(super) fn receive(&self, body: &str) {
        use base64::Engine;
        use vmux_ui::transport::bin_ipc_envelope::BinIpcEnvelope;

        // The interpreter's own two messages, sent as `{"method":..}` by `sendIpcMessage`.
        // `initialize` says the page can take a batch at all; `flushed` says it applied the last
        // one, which is what releases the next render.
        if body.contains(r#""method":"initialize""#) {
            self.dom.page_is_ready();
            return;
        }
        if body.contains(r#""method":"flushed""#) {
            self.dom.page_flushed();
            return;
        }
        if let Some(rest) = body.strip_prefix("caret:")
            && let Some((element_id, byte)) = rest.rsplit_once(':')
            && let Ok(byte) = byte.parse::<usize>()
        {
            self.dom.report_caret(element_id, byte);
            return;
        }
        if let Some(rest) = body.strip_prefix("log:") {
            let (level, text) = rest.split_once(':').unwrap_or(("log", rest));
            let name = self.name;
            match level {
                "error" | "reject" => error!("{name}: {text}"),
                "warn" => warn!("{name}: {text}"),
                _ => info!("{name}: {text}"),
            }
            return;
        }
        let bytes = match base64::engine::general_purpose::STANDARD.decode(body) {
            Ok(bytes) => bytes,
            Err(error) => {
                error!("page_surface: ipc payload was not base64: {error}");
                return;
            }
        };
        let Some((id, payload)) = BinIpcEnvelope::decode(&bytes) else {
            error!(
                "layout_view: ipc payload was not a bin ipc envelope, {} bytes",
                bytes.len()
            );
            return;
        };
        let sent = self.bin_ipc.send_blocking(BinIpcEventRaw {
            webview: self.webview,
            host: self.host.clone(),
            id,
            payload,
        });
        if sent.is_err() {
            error!("page_surface: bin ipc channel closed");
        }
    }
}

/// What a wasm page finds on `window` instead of `window.cef`.
///
/// Deliberately the same two verbs. `vmux_ui::transport` picks its `PageHost` at runtime, so the
/// page half of this is a matter of answering to whichever object is present, not of teaching the
/// pages a second protocol.
pub(super) const WRY_HOST_SHIM: &str = r#"
(function () {
  const report = (kind, text) => {
    try { window.ipc.postMessage('log:' + kind + ':' + text); } catch (e) {}
  };
  // Volunteered rather than asked for: the host reaches this document by evaluating a script,
  // which returns nothing, so a component wanting the caret has no way to ask. Reported in UTF-8
  // bytes because that is the unit the Rust side counts in.
  const reportCaret = () => {
    const el = document.activeElement;
    if (!el || !el.id || typeof el.selectionStart !== 'number') return;
    const bytes = new TextEncoder().encode(el.value.slice(0, el.selectionStart)).length;
    try { window.ipc.postMessage('caret:' + el.id + ':' + bytes); } catch (e) {}
  };
  document.addEventListener('selectionchange', reportCaret);
  for (const name of ['keyup', 'mouseup', 'input', 'focusin']) {
    document.addEventListener(name, reportCaret, true);
  }
  window.addEventListener('error', (e) => {
    report('error', (e.message || 'error') + ' @ ' + (e.filename || '?') + ':' + (e.lineno || 0));
  });
  window.addEventListener('unhandledrejection', (e) => {
    report('reject', String((e.reason && e.reason.stack) || e.reason));
  });
  for (const level of ['error', 'warn', 'log']) {
    const original = console[level].bind(console);
    console[level] = (...args) => {
      report(level, args.map((a) => {
        if (a instanceof Error) return a.stack || a.message;
        if (typeof a === 'object') { try { return JSON.stringify(a); } catch (e) { return String(a); } }
        return String(a);
      }).join(' '));
      original(...args);
    };
  }
  const listeners = new Map();
  function toBase64(buffer) {
    const bytes = new Uint8Array(buffer);
    let binary = '';
    for (let i = 0; i < bytes.length; i += 1) binary += String.fromCharCode(bytes[i]);
    return btoa(binary);
  }
  function fromBase64(text) {
    const binary = atob(text);
    const bytes = new Uint8Array(binary.length);
    for (let i = 0; i < binary.length; i += 1) bytes[i] = binary.charCodeAt(i);
    return bytes.buffer;
  }
  window.vmuxWry = {
    binEmit(buffer) { window.ipc.postMessage(toBase64(buffer)); },
    binListen(id, callback) {
      const existing = listeners.get(id) || [];
      existing.push(callback);
      listeners.set(id, existing);
    },
    _dispatch(id, base64) {
      const buffer = fromBase64(base64);
      for (const callback of listeners.get(id) || []) callback(buffer);
    },
  };
})();
"#;
