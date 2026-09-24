#[vmux_native::page(
    url = crate::START_PAGE_URL,
    title = "Start",
    component = crate::ui::StartPage,
    document_url = crate::START_PAGE_URL
)]
pub struct StartPage;
