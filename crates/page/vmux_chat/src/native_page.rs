#[vmux_native::page(
    url = "vmux://sessions/",
    title = "Sessions",
    component = crate::ui::Page,
    subtree,
    preserve_title,
    no_favicon,
    takes = vmux_core::PageMetadata
)]
pub struct ChatPage;
