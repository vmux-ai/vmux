pub struct NativePage {
    pub url: &'static str,
    pub document_url: Option<&'static str>,
    pub component: crate::PageComponent,
    pub root_id: &'static str,
    pub root_class: &'static str,
    pub head: &'static str,
    pub html_attributes: &'static str,
    pub body_class: &'static str,
    pub transparent: bool,
    pub owns_subtree: bool,
}

impl PartialEq for NativePage {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self, other)
    }
}

impl NativePage {
    pub fn answers_for(&self, url: &str) -> bool {
        url == self.url || (self.owns_subtree && url.starts_with(self.url))
    }

    pub fn document_url(&self) -> &'static str {
        match self.document_url {
            Some(url) => url,
            None => self.url,
        }
    }
    pub const fn served_from(mut self, url: &'static str) -> Self {
        self.document_url = Some(url);
        self
    }
    pub const fn owning_subtree(mut self) -> Self {
        self.owns_subtree = true;
        self
    }

    pub const fn pane(url: &'static str, component: crate::PageComponent) -> Self {
        Self {
            url,
            component,
            root_id: "main",
            root_class: "flex min-h-0 min-w-0 flex-1 flex-col",
            head: r#"<base href="/"/>
<style>
html, body { height: 100%; margin: 0; min-height: 0; }
body { display: flex; flex-direction: column; min-height: 0; overflow: hidden; }
</style>
<link rel="stylesheet" href="./assets/index.css"/>
<link rel="stylesheet" href="./assets/theme.css"/>"#,
            html_attributes: r#"lang="en" class="h-full" style="color-scheme: light dark""#,
            body_class: "m-0 flex h-full min-h-0 flex-col overflow-hidden p-0 text-foreground antialiased",
            transparent: false,
            owns_subtree: false,
            document_url: None,
        }
    }
}

#[cfg(ui)]
impl NativePage {
    pub(crate) fn shell(&self) -> wry::http::Response<Vec<u8>> {
        let html = crate::InterpreterShell::new(self.root_id, self.document_url())
            .with_head(self.head)
            .with_html_attributes(self.html_attributes)
            .with_body_class(self.body_class)
            .with_root_class(self.root_class)
            .html();

        wry::http::Response::builder()
            .header(wry::http::header::CONTENT_TYPE, "text/html")
            .body(html.into_bytes())
            .unwrap_or_else(|_| wry::http::Response::new(Vec::new()))
    }
}

#[cfg(all(test, ui))]
mod shell_tests {
    use super::*;

    fn page() -> NativePage {
        NativePage::pane("file://", || unreachable!()).served_from("vmux://files/")
    }

    #[test]
    fn the_interpreter_talks_to_the_origin_the_document_came_from() {
        let html = String::from_utf8(page().shell().into_body()).unwrap();

        assert!(
            html.contains(r#"new NativeInterpreter("vmux://files", false)"#),
            "the shell pointed the interpreter somewhere other than the document url"
        );
        assert!(
            !html.contains(r#"NativeInterpreter("file:"#),
            "no protocol handler answers `file://`, so nothing would reply to a fetch there"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_subtree_page_answers_below_its_url_without_reaching_a_sibling() {
        fn nowhere() -> dioxus_core::Element {
            dioxus_core::VNode::empty()
        }
        let list = NativePage::pane("vmux://agents/", nowhere);
        let chat = NativePage::pane("vmux://agent/", nowhere).owning_subtree();

        assert!(chat.answers_for("vmux://agent/"));
        assert!(chat.answers_for("vmux://agent/claude/sess-7"));
        assert!(!chat.answers_for("vmux://agents/"));
        assert!(list.answers_for("vmux://agents/"));
        assert!(!list.answers_for("vmux://agents/anything"));
    }
}
