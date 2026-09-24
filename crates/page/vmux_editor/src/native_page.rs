#[vmux_native::page(
    url = "vmux://lsp/",
    title = "Language Servers",
    component = crate::lsp_page::Page
)]
pub struct LspPage;

#[vmux_native::page(
    url = "file://",
    title = "Files",
    component = crate::ui::Page,
    dom_group = "editor",
    subtree
)]
pub struct FilePage;

#[vmux_native::page(
    url = vmux_api::space::PROJECTS_PAGE_URL,
    title = "Projects",
    component = crate::ui::Page,
    dom_group = "editor",
    subtree
)]
pub struct ProjectsPage;

#[vmux_native::page(
    url = vmux_core::knowledge::KNOWLEDGE_PAGE_URL,
    title = "Knowledge",
    component = crate::ui::Page,
    dom_group = "editor",
    subtree
)]
pub struct KnowledgePage;
