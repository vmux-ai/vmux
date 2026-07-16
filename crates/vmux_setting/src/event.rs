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

#[cfg(not(target_arch = "wasm32"))]
/// Native request consumed by the desktop updater.
#[derive(bevy::prelude::Message, Clone, Copy, Debug, Default)]
pub struct CheckForUpdatesRequest;

/// Updater activity shared by the desktop updater and Settings host.
#[cfg(not(target_arch = "wasm32"))]
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
mod tests {
    use super::*;

    #[test]
    fn settings_list_event_rkyv_roundtrip() {
        let original = SettingsListEvent {
            json: r#"{"auto_update":true}"#.to_string(),
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&original).expect("ser");
        let decoded =
            rkyv::from_bytes::<SettingsListEvent, rkyv::rancor::Error>(&bytes).expect("de");
        assert_eq!(decoded, original);
    }

    #[test]
    fn settings_command_event_rkyv_roundtrip() {
        let original = SettingsCommandEvent {
            path: "layout.pane.gap".to_string(),
            value: "12.0".to_string(),
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&original).expect("ser");
        let decoded =
            rkyv::from_bytes::<SettingsCommandEvent, rkyv::rancor::Error>(&bytes).expect("de");
        assert_eq!(decoded, original);
    }

    #[test]
    fn settings_schema_event_rkyv_roundtrip() {
        let original = SettingsSchemaEvent {
            json: r#"{"sections":[]}"#.to_string(),
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&original).expect("ser");
        let decoded =
            rkyv::from_bytes::<SettingsSchemaEvent, rkyv::rancor::Error>(&bytes).expect("de");
        assert_eq!(decoded, original);
    }

    #[test]
    fn check_for_updates_event_rkyv_roundtrip() {
        let original = CheckForUpdatesEvent;
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&original).expect("ser");
        let decoded =
            rkyv::from_bytes::<CheckForUpdatesEvent, rkyv::rancor::Error>(&bytes).expect("de");
        assert_eq!(decoded, original);
    }

    #[test]
    fn update_check_status_event_rkyv_roundtrip() {
        let original = UpdateCheckStatusEvent {
            status: UpdateCheckStatus::Downloading {
                version: "1.2.3".to_string(),
            },
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&original).expect("ser");
        let decoded =
            rkyv::from_bytes::<UpdateCheckStatusEvent, rkyv::rancor::Error>(&bytes).expect("de");
        assert_eq!(decoded, original);
    }
}
