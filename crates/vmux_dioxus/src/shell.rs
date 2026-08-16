//! The document a natively-hosted page loads instead of a wasm bundle.

use dioxus_interpreter_js::NATIVE_JS;
use dioxus_interpreter_js::unified_bindings::SLEDGEHAMMER_JS;

/// The HTML served to a page whose components run in the host process.
///
/// It carries no wasm and no application script: the only JavaScript is the interpreter, waiting
/// to be handed batches of edits. Everything the page displays arrives from [`crate::PageDom`].
pub struct InterpreterShell {
    root_id: &'static str,
    base_uri: String,
    stylesheets: Vec<String>,
}

impl InterpreterShell {
    /// `base_uri` is the page's own origin without a trailing slash — the interpreter appends
    /// `/__events` to it, and a trailing slash would ask for `//__events`.
    pub fn new(root_id: &'static str, base_uri: impl Into<String>) -> Self {
        Self {
            root_id,
            base_uri: base_uri.into().trim_end_matches('/').to_string(),
            stylesheets: Vec::new(),
        }
    }

    /// Link a stylesheet, in the order added.
    ///
    /// The page's CSS is still fetched over the same scheme the shell came from; only the markup
    /// stops being the bundle's.
    pub fn with_stylesheet(mut self, href: impl Into<String>) -> Self {
        self.stylesheets.push(href.into());
        self
    }

    pub fn html(&self) -> String {
        let Self {
            root_id,
            base_uri,
            stylesheets,
        } = self;

        let mut links = String::new();
        for href in stylesheets {
            links.push_str(&format!(r#"<link rel="stylesheet" href="{href}">"#));
            links.push('\n');
        }

        // `initialize` is a handshake, not a formality: the host must not evaluate an edit batch
        // until the interpreter exists and has been given a root, and `window.onload` is the only
        // point at which that is true.
        //
        // Deliberately no `waitForRequest`. That opens the WebSocket dioxus-desktop serves edits
        // over; here they arrive by script evaluation instead, so calling it would leave a socket
        // retrying against a port nothing is listening on.
        format!(
            r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
{links}</head>
<body>
<div id="{root_id}"></div>
<script type="module">
{SLEDGEHAMMER_JS}
{NATIVE_JS}
window.interpreter = new NativeInterpreter("{base_uri}", false);
window.onload = function() {{
  const root = window.document.getElementById("{root_id}");
  if (root != null) {{
    window.interpreter.initialize(root);
    window.interpreter.sendIpcMessage("initialize");
  }}
}};
</script>
</body>
</html>"#
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_events_path_the_interpreter_builds_has_one_slash() {
        let shell = InterpreterShell::new("main", "vmux://layout/").html();

        assert!(
            shell.contains(r#"new NativeInterpreter("vmux://layout", false)"#),
            "native.js appends /__events to the base uri, so a trailing slash asks for //__events"
        );
    }

    #[test]
    fn the_shell_never_waits_on_an_edit_socket() {
        let shell = InterpreterShell::new("main", "vmux://layout").html();

        // The interpreter class defines `waitForRequest`; what must not appear is a call to it.
        assert!(
            !shell.contains("interpreter.waitForRequest"),
            "edits arrive by script evaluation; calling it would retry against a port nothing binds"
        );
    }

    #[test]
    fn the_root_the_interpreter_is_given_is_the_one_the_document_holds() {
        let shell = InterpreterShell::new("vmux-root", "vmux://layout").html();

        assert!(shell.contains(r#"<div id="vmux-root"></div>"#));
        assert!(shell.contains(r#"getElementById("vmux-root")"#));
    }

    #[test]
    fn stylesheets_are_linked_in_the_order_they_were_added() {
        let shell = InterpreterShell::new("main", "vmux://layout")
            .with_stylesheet("/assets/theme.css")
            .with_stylesheet("/assets/tailwind.css")
            .html();

        let theme = shell.find("theme.css").expect("theme linked");
        let tailwind = shell.find("tailwind.css").expect("tailwind linked");

        assert!(theme < tailwind, "cascade order is the caller's to decide");
    }
}
