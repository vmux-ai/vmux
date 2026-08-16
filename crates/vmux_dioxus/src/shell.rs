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
    head: String,
    html_attributes: String,
    body_class: String,
    root_class: String,
}

impl InterpreterShell {
    /// `base_uri` is the page's own origin without a trailing slash — the interpreter appends
    /// `/__events` to it, and a trailing slash would ask for `//__events`.
    pub fn new(root_id: &'static str, base_uri: impl Into<String>) -> Self {
        Self {
            root_id,
            base_uri: base_uri.into().trim_end_matches('/').to_string(),
            head: String::new(),
            html_attributes: String::new(),
            body_class: String::new(),
            root_class: String::new(),
        }
    }

    /// Everything the page needs inside `<head>` — its `<base>`, stylesheets and inline rules.
    ///
    /// Taken whole rather than as a list of hrefs because a page's document chrome is the page's
    /// business: this crate cannot know which sheet must come first, or that the root needs a
    /// height before flex layout means anything.
    pub fn with_head(mut self, head: impl Into<String>) -> Self {
        self.head = head.into();
        self
    }

    /// Attributes for `<html>`, verbatim.
    pub fn with_html_attributes(mut self, attributes: impl Into<String>) -> Self {
        self.html_attributes = attributes.into();
        self
    }

    pub fn with_body_class(mut self, class: impl Into<String>) -> Self {
        self.body_class = class.into();
        self
    }

    /// Classes for the element the page renders into.
    pub fn with_root_class(mut self, class: impl Into<String>) -> Self {
        self.root_class = class.into();
        self
    }

    pub fn html(&self) -> String {
        let Self {
            root_id,
            base_uri,
            head,
            html_attributes,
            body_class,
            root_class,
        } = self;

        // `initialize` is a handshake, not a formality: the host must not evaluate an edit batch
        // until the interpreter exists and has been given a root, and `window.onload` is the only
        // point at which that is true.
        //
        // Deliberately no `waitForRequest`. That opens the WebSocket dioxus-desktop serves edits
        // over; here they arrive by script evaluation instead, so calling it would leave a socket
        // retrying against a port nothing is listening on.
        format!(
            r#"<!DOCTYPE html>
<html {html_attributes}>
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width">
{head}
</head>
<body class="{body_class}">
<div id="{root_id}" class="{root_class}"></div>
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

        assert!(shell.contains(r#"<div id="vmux-root""#));
        assert!(shell.contains(r#"getElementById("vmux-root")"#));
    }

    #[test]
    fn the_page_keeps_the_document_chrome_it_was_written_against() {
        let shell = InterpreterShell::new("main", "vmux://layout")
            .with_head(r#"<base href="/"><link rel="stylesheet" href="./assets/index.css">"#)
            .with_html_attributes(r#"class="h-full""#)
            .with_body_class("flex h-full")
            .with_root_class("flex flex-1")
            .html();

        assert!(shell.contains(r#"<base href="/">"#));
        assert!(shell.contains("./assets/index.css"));
        assert!(shell.contains(r#"<html class="h-full">"#));
        assert!(shell.contains(r#"<body class="flex h-full">"#));
        assert!(
            shell.contains(r#"<div id="main" class="flex flex-1"></div>"#),
            "the root carries the page's own layout classes, or nothing it renders has a size"
        );
    }
}
