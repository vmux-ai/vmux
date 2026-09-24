#[vmux_native::page(
    url = crate::url::PAGE_URL,
    component = crate::ui::Page,
    subtree,
    takes = vmux_core::PageMetadata
)]
pub struct SimulatorPage;
