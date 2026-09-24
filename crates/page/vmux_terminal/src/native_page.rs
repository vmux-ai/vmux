#[vmux_native::page(
    url = crate::event::TERMINAL_PAGE_URL,
    title = "Terminal",
    component = crate::ui::Page,
    claims = crate::Terminal
)]
pub struct TerminalPage;
