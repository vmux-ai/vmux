pub struct NativePage {
    pub(crate) url: &'static str,
    pub(crate) document_url: Option<&'static str>,
    #[cfg_attr(not(ui), allow(dead_code))]
    pub(crate) component: crate::PageComponent,
    pub(crate) root_id: &'static str,
    pub(crate) root_class: &'static str,
    pub(crate) head: &'static str,
    pub(crate) html_attributes: &'static str,
    pub(crate) body_class: &'static str,
    pub(crate) transparent: bool,
    pub(crate) background: Option<(u8, u8, u8, u8)>,
    pub(crate) owns_subtree: bool,
}

impl PartialEq for NativePage {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self, other)
    }
}

impl NativePage {
    pub fn url(&self) -> &'static str {
        self.url
    }

    pub fn background_or(&self, fallback: (u8, u8, u8, u8)) -> (u8, u8, u8, u8) {
        match self.background {
            Some(colour) => colour,
            None => fallback,
        }
    }

    pub fn prefers_dark(&self) -> Option<bool> {
        let (red, green, blue, _) = self.background?;
        let luminance = 0.299 * f64::from(red) + 0.587 * f64::from(green) + 0.114 * f64::from(blue);
        Some(luminance < 128.0)
    }

    pub fn answers_for(&self, url: &str) -> bool {
        url == self.url || (self.owns_subtree && url.starts_with(self.url))
    }

    pub fn document_url(&self) -> &'static str {
        match self.document_url {
            Some(url) => url,
            None => self.url,
        }
    }
    pub const fn heading(mut self, head: &'static str) -> Self {
        self.head = head;
        self
    }

    pub const fn rooted(mut self, id: &'static str, class: &'static str) -> Self {
        self.root_id = id;
        self.root_class = class;
        self
    }

    pub const fn dressed(mut self, html: &'static str, body: &'static str) -> Self {
        self.html_attributes = html;
        self.body_class = body;
        self
    }

    pub const fn see_through(mut self) -> Self {
        self.transparent = true;
        self
    }

    pub const fn served_from(mut self, url: &'static str) -> Self {
        self.document_url = Some(url);
        self
    }
    pub const fn background(mut self, colour: (u8, u8, u8, u8)) -> Self {
        self.background = Some(colour);
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
<meta name="viewport" content="width=device-width, initial-scale=1, maximum-scale=1, user-scalable=no, viewport-fit=cover"/>
<style>
html, body { height: 100%; margin: 0; min-height: 0; }
body { display: flex; flex-direction: column; min-height: 0; overflow: hidden; }
</style>
<link rel="stylesheet" href="./assets/index.css"/>
<link rel="stylesheet" href="./assets/theme.css"/>"#,
            html_attributes: r#"lang="en" class="h-full" style="color-scheme: light dark""#,
            body_class: "m-0 flex h-full min-h-0 flex-col overflow-hidden p-0 text-foreground antialiased",
            transparent: false,
            background: None,
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
