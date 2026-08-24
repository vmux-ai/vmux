pub struct NativePage {
    pub url: &'static str,
    /// The url the view is told to load, when that cannot be [`Self::url`].
    ///
    /// The two are the same for every page served from the `vmux` scheme, which is why one field
    /// did for both jobs until the editor arrived. The editor answers for `file://`, and handing
    /// that to the view sends it to the operating system's file loader rather than the custom
    /// protocol: a blank document, with the page's component mounted into nothing.
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
        // The document url rather than `url`: the interpreter appends `__events` and `__edits`
        // to this base and fetches them, so a base the view is not actually on makes every one
        // of those cross-origin — which surfaces as a bare "Script error." from a page that
        // loaded its stylesheet and then never rendered anything.
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

    /// The interpreter fetches `__events` and `__edits` against the base the shell hands it, so a
    /// base that is not the document's own origin makes every one of those cross-origin. That
    /// shows up as a bare `Script error.` and a page that paints its background and stops.
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
