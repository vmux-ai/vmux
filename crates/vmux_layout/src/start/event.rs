/// Page→host request: the `vmux://start/` page emits this on mount to ask the host
/// for its launcher entries. The host answers with a `CommandBarOpenEvent`.
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

#[derive(
    Clone,
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
pub struct StartSelectWorkspace {
    pub current_dir: String,
}

/// Host→page signal: focus the start launcher's input. Sent when a command-bar
/// shortcut fires while the start page is active (instead of opening the modal).
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
