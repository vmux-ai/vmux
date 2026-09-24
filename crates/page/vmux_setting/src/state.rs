use crate::event::UpdateCheckStatus;
use crate::schema::SettingsSchema;

#[vmux_api::ui_state(Default, version = 2, target = "settings")]
pub struct SettingsUiState {
    pub settings: vmux_api::json::JsonValue,
    pub schema: SettingsSchema,
    pub update_status: UpdateCheckStatus,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_round_trips() {
        let state = SettingsUiState {
            settings: serde_json::json!({"auto_update": true}).into(),
            schema: SettingsSchema::default(),
            update_status: UpdateCheckStatus::Checking,
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&state).unwrap();
        let decoded = rkyv::from_bytes::<SettingsUiState, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(
            serde_json::Value::try_from(&decoded.settings).unwrap(),
            serde_json::json!({"auto_update": true})
        );
        assert_eq!(decoded.update_status, UpdateCheckStatus::Checking);
    }
}
