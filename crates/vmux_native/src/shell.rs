use dioxus_interpreter_js::NATIVE_JS;
use dioxus_interpreter_js::unified_bindings::SLEDGEHAMMER_JS;

pub struct InterpreterShell {
    root_id: &'static str,
    base_uri: String,
    head: String,
    html_attributes: String,
    body_class: String,
    root_class: String,
}

impl InterpreterShell {
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

    pub fn with_head(mut self, head: impl Into<String>) -> Self {
        self.head = head.into();
        self
    }

    pub fn with_html_attributes(mut self, attributes: impl Into<String>) -> Self {
        self.html_attributes = attributes.into();
        self
    }

    pub fn with_body_class(mut self, class: impl Into<String>) -> Self {
        self.body_class = class.into();
        self
    }

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
    window.vmuxWry.start();
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

        assert!(
            !shell.contains("interpreter.waitForRequest"),
            "the page fetches its own edits; calling it would retry against a port nothing binds"
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
