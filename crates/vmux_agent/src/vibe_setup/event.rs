/// Emitted by the vibe-setup page (button click) → handled on the Bevy side to spawn a terminal and
/// run the Vibe install script.
#[derive(
    Clone,
    Debug,
    Default,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct VibeInstallRunRequest;
