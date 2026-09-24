#[vmux_native::page(
    url = crate::GIT_PAGE_URL,
    title = "Git",
    component = crate::ui::Page,
    document_url = crate::GIT_DOCUMENT_URL,
    subtree
)]
pub struct GitPage;

#[vmux_native::page(
    url = crate::GIT_DOCUMENT_URL,
    title = "Git",
    component = crate::ui::Page
)]
pub struct LegacyGitPage;
