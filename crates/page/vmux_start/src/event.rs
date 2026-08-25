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
)]
pub struct StartDataRequest;

pub use vmux_wire::command_bar::StartSelectWorkspace;

pub const START_COMMAND_BAR_OPEN_EVENT: &str = "start-command-bar-open";

pub const START_FOCUS_INPUT_EVENT: &str = "start-focus-input";

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
)]
pub struct StartFocusInput;
