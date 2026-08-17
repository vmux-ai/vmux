//! Serving a natively-hosted page, and hearing it back.
//!
//! Two directions over one webview. `vmux://` in: the shell document, the `__events` verdict the
//! page blocks on, and every asset, which the host resolves. wry's string IPC out: the
//! interpreter's own handshake, the caret the document volunteers, the console, and the rkyv
//! envelopes a page emits.

use tracing::{error, info, warn};

use crate::dom::SurfaceDom;
use crate::embed::{AssetReply, Assets, Outbox};
use crate::page::NativePage;

/// `vmux://` for the wry view, answered by the host that would answer it for any other engine.
pub(crate) struct VmuxProtocol;

impl VmuxProtocol {
    pub(crate) fn serve(
        page: &NativePage,
        dom: &SurfaceDom,
        assets: &dyn Assets,
        request: wry::http::Request<Vec<u8>>,
        responder: wry::RequestAsyncResponder,
    ) {
        let url = request.uri().to_string();

        // Both branches must come before the host sees the url. Neither path has a file extension,
        // so an asset lookup would map them to the host's default document and hand the page HTML
        // where it expects JSON — or the wasm bundle where it expects the shell.
        if url.trim_end_matches('/').ends_with("/__events") {
            return Self::answer_event(dom, &request, responder);
        }
        if Self::is_document_request(&url) {
            return responder.respond(Self::shell_response(page));
        }

        assets.fetch(&url, AssetReply::of(responder));
    }

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
    /// The chrome a page carries is not decoration: without its stylesheet nothing has a Tailwind
    /// rule, and without the height and flex rules on `html`, `body` and the root, a flex child
    /// has no box to fill — which renders as one icon at its intrinsic size filling the window.
    fn shell_response(page: &NativePage) -> wry::http::Response<Vec<u8>> {
        let html = crate::InterpreterShell::new(page.root_id, page.url)
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
}

/// One page-to-host message, as it arrives over wry's string IPC.
///
/// A page emits through an `ArrayBuffer`; wry's IPC carries text, so the shim base64s the same
/// `BinIpcEnvelope` bytes and this undoes it. The envelope framing is left alone, because the host
/// matches its id with `bin_ipc_event_id::<E>()` and that has to keep agreeing.
pub(crate) struct PageMessage {
    outbox: std::rc::Rc<dyn Outbox>,
    name: &'static str,
    dom: SurfaceDom,
}

impl PageMessage {
    pub(crate) fn new(page: &NativePage, outbox: std::rc::Rc<dyn Outbox>, dom: SurfaceDom) -> Self {
        Self {
            outbox,
            name: page.url,
            dom,
        }
    }

    pub(crate) fn receive(&self, body: &str) {
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
                error!("vmux_native: ipc payload was not base64: {error}");
                return;
            }
        };
        let Some((id, payload)) = BinIpcEnvelope::decode(&bytes) else {
            error!(
                "vmux_native: ipc payload was not a bin ipc envelope, {} bytes",
                bytes.len()
            );
            return;
        };
        if self.outbox.send(&id, &payload).is_err() {
            error!("vmux_native: the host outbox is closed");
        }
    }
}

/// What a natively-hosted page finds on `window` instead of the bundle's own bridge.
///
/// Deliberately the same two verbs. `vmux_ui::transport` picks its `PageHost` at runtime, so the
/// page half of this is a matter of answering to whichever object is present, not of teaching the
/// pages a second protocol.
pub(crate) const WRY_HOST_SHIM: &str = r#"
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
