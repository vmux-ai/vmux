pub const SETTINGS_PAGE_URL: &str = "vmux://settings/";
pub const SETTINGS_LIST_EVENT: &str = "settings_list";
pub const SETTINGS_SCHEMA_EVENT: &str = "settings_schema";
pub const UPDATE_CHECK_STATUS_EVENT: &str = "update_check_status";

/// Requests an immediate update check.
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
pub struct CheckForUpdatesEvent;

/// Current updater activity shown in Settings.
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
pub enum UpdateCheckStatus {
    #[default]
    Idle,
    Checking,
    UpToDate,
    Downloading {
        version: String,
    },
    Installing {
        version: String,
    },
    Ready {
        version: String,
    },
    Failed,
    Unavailable,
}

/// Carries updater activity to the Settings page.
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
pub struct UpdateCheckStatusEvent {
    pub status: UpdateCheckStatus,
}

#[cfg(not(web))]
/// Native request consumed by the desktop updater.
#[derive(bevy::prelude::Message, Clone, Copy, Debug, Default)]
pub struct CheckForUpdatesRequest;

/// Updater activity shared by the desktop updater and Settings host.
#[cfg(not(web))]
#[derive(bevy::prelude::Resource, Clone, Debug, Default, PartialEq, Eq)]
pub struct CurrentUpdateCheckStatus(pub UpdateCheckStatus);

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
pub struct SettingsListEvent {
    pub json: String,
}

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
pub struct SettingsCommandEvent {
    pub path: String,
    pub value: String,
}

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
pub struct SettingsSchemaEvent {
    pub json: String,
}

#[cfg(test)]
#[path = "event.test.rs"]
mod tests;
