use super::OpenId;

#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
    vmux_api::UiEvent,
)]
#[event(targets = ["command-bar", "start", "layout"])]
pub struct CommandBarReadyEvent;

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[vmux_api::host_event(targets = ["command-bar", "start", "layout"])]
pub enum CommandBarKey {
    Next,
    Previous,
    Complete,
    Dismiss,
}

#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
    vmux_api::UiEvent,
)]
#[event(targets = ["command-bar", "start", "layout"])]
pub struct CommandBarRenderedEvent {
    pub open_id: OpenId,
}

#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
    vmux_api::UiEvent,
)]
#[event(targets = ["command-bar", "start", "layout"])]
pub struct CommandBarSizeEvent {
    pub width: u32,
    pub height: u32,
    pub shell_left: i32,
    pub shell_top: i32,
    pub shell_width: u32,
    pub shell_height: u32,
}
