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
