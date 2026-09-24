#[vmux_api::ui_event(Copy, Default, Eq, target = "start")]
pub struct StartDataRequest;

pub use vmux_api::command_bar::StartSelectWorkspace;

#[vmux_api::host_event(Copy, Default, Eq, target = "start")]
pub struct StartFocusInput;
