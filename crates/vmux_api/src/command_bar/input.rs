use super::OpenId;

#[vmux_api::ui_event(Copy, Default, Eq, targets = ["command-bar", "start", "layout"])]
pub struct CommandBarReadyEvent;

#[vmux_api::host_event(Copy, Eq, targets = ["command-bar", "start", "layout"])]
pub enum CommandBarKey {
    Next,
    Previous,
    Complete,
    Dismiss,
}

#[vmux_api::ui_event(Copy, Default, Eq, targets = ["command-bar", "start", "layout"])]
pub struct CommandBarRenderedEvent {
    pub open_id: OpenId,
}

#[vmux_api::ui_event(Copy, Default, Eq, targets = ["command-bar", "start", "layout"])]
pub struct CommandBarSizeEvent {
    pub width: u32,
    pub height: u32,
    pub shell_left: i32,
    pub shell_top: i32,
    pub shell_width: u32,
    pub shell_height: u32,
}
