pub const SETTINGS_PAGE_URL: &str = "vmux://settings/";
#[vmux_api::ui_event(Copy, Default, Eq, target = "settings")]
pub struct CheckForUpdatesEvent;

#[vmux_api::contract(Default, Eq)]
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

#[vmux_api::contract(Default, Eq)]
pub struct UpdateCheckStatusEvent {
    pub status: UpdateCheckStatus,
}

#[cfg(host)]
#[derive(bevy::prelude::Message, Clone, Copy, Debug, Default)]
pub struct CheckForUpdatesRequest;

#[cfg(host)]
#[derive(bevy::prelude::Resource, Clone, Debug, Default, PartialEq, Eq)]
pub struct CurrentUpdateCheckStatus(pub UpdateCheckStatus);

#[vmux_api::contract(Default, Eq)]
pub struct SettingsListEvent {
    pub value: vmux_api::json::JsonValue,
}

#[vmux_api::ui_event(Default, Eq, version = 2, target = "settings")]
pub struct SettingsRequest {
    pub path: String,
    pub value: vmux_api::json::JsonValue,
}

#[vmux_api::contract(Default)]
pub struct SettingsSchemaEvent {
    pub schema: crate::schema::SettingsSchema,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_list_event_rkyv_roundtrip() {
        let original = SettingsListEvent {
            value: serde_json::json!({"auto_update": true}).into(),
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&original).expect("ser");
        let decoded =
            rkyv::from_bytes::<SettingsListEvent, rkyv::rancor::Error>(&bytes).expect("de");
        assert_eq!(decoded, original);
    }

    #[test]
    fn settings_request_rkyv_roundtrip() {
        let original = SettingsRequest {
            path: "layout.pane.gap".to_string(),
            value: serde_json::json!(12.0).into(),
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&original).expect("ser");
        let decoded = rkyv::from_bytes::<SettingsRequest, rkyv::rancor::Error>(&bytes).expect("de");
        assert_eq!(decoded, original);
    }

    #[test]
    fn settings_schema_event_rkyv_roundtrip() {
        let original = SettingsSchemaEvent {
            schema: crate::schema::SettingsSchema::default(),
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
