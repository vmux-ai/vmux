#[vmux_native::page(
    url = crate::event::LAYOUT_PAGE_URL,
    title = "vmux",
    component = crate::ui::Page,
    placement = layout,
    document_url = crate::event::LAYOUT_PAGE_URL,
    transparent,
    head = r#"<base href="/"/>
<style>
html, body { height: 100%; margin: 0; min-height: 0; }
body { display: flex; flex-direction: column; min-height: 0; overflow: hidden; background: transparent; }
</style>
<link rel="stylesheet" href="./assets/index.css"/>
<link rel="stylesheet" href="./assets/theme.css"/>"#,
    body_class = "m-0 flex h-full min-h-0 flex-col overflow-hidden bg-transparent p-0 text-foreground antialiased"
)]
pub struct LayoutPage;

#[vmux_native::page(
    url = "vmux://tools/",
    title = "Tools",
    component = crate::tool_page::Page,
    subtree,
    takes = vmux_core::PageMetadata
)]
pub struct ToolsPage;

#[vmux_native::page(
    url = "vmux://vault/",
    title = "Vault",
    component = crate::vault_page::Page,
    subtree,
    takes = vmux_core::PageMetadata
)]
pub struct VaultPage;

#[vmux_native::page(
    url = vmux_core::event::EXTENSIONS_PAGE_URL,
    title = "Extensions",
    component = crate::extensions_page::Page
)]
pub struct ExtensionsPage;

#[vmux_native::page(
    url = vmux_api::error::ERROR_PAGE_URL,
    title = "Error",
    component = crate::error_page::Page,
    takes = vmux_api::error::ErrorPageData
)]
pub struct ErrorPage;
