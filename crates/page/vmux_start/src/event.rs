#[vmux_api::ui_event(Copy, Default, Eq, url = "vmux://start/")]
pub struct StartDataRequest;

pub use vmux_api::command_bar::StartSelectWorkspace;
