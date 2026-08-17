//! Everything that distinguishes one natively-hosted page from another.

/// A page this process can run, described in full.
///
/// A `const` per page, because a page names itself: the alternative is a registry the pages have
/// to be looked up in, which is one more thing to keep in agreement with them.
pub struct NativePage {
    pub url: &'static str,
    pub component: crate::PageComponent,
    /// The element the interpreter renders into, and its classes.
    pub root_id: &'static str,
    pub root_class: &'static str,
    /// Everything inside `<head>` — stylesheets, `<base>`, inline rules.
    pub head: &'static str,
    pub html_attributes: &'static str,
    pub body_class: &'static str,
    /// A page drawn over other content wants to see through itself; one filling a pane does not.
    pub transparent: bool,
}

impl NativePage {
    /// A page that fills what it is opened into: opaque, and carrying the app's own stylesheets.
    ///
    /// Almost every page is this. The ones that are not say so by writing the struct out — the
    /// chrome is the layout's alone, because it is the only page drawn over the others and so the
    /// only one that has to see through itself.
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
        }
    }
}

#[cfg(target_os = "macos")]
impl NativePage {
    /// The document this page loads: the interpreter, and nothing else.
    ///
    /// The chrome a page carries is not decoration: without its stylesheet nothing has a Tailwind
    /// rule, and without the height and flex rules on `html`, `body` and the root, a flex child
    /// has no box to fill — which renders as one icon at its intrinsic size filling the window.
    pub(crate) fn shell(&self) -> wry::http::Response<Vec<u8>> {
        let html = crate::InterpreterShell::new(self.root_id, self.url)
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
