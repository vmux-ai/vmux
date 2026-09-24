use crate::event::{SettingsListEvent, SettingsSchemaEvent, UpdateCheckStatusEvent};

#[vmux_api::ui_state_patch]
pub enum SettingsUiStatePatch {
    Settings(SettingsListEvent),
    Schema(SettingsSchemaEvent),
    UpdateStatus(UpdateCheckStatusEvent),
}

#[vmux_api::ui_state(Default, target = "settings")]
pub struct SettingsUiState {
    pub sequence: u64,
    pub patches: Vec<SettingsUiStatePatch>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::UpdateCheckStatus;

    #[test]
    fn batches_preserve_patch_order() {
        let event = SettingsUiState {
            sequence: 2,
            patches: vec![
                SettingsListEvent::default().into(),
                UpdateCheckStatusEvent {
                    status: UpdateCheckStatus::Checking,
                }
                .into(),
            ],
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&event).unwrap();
        let decoded = rkyv::from_bytes::<SettingsUiState, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(decoded.sequence, 2);
        assert!(matches!(
            decoded.patches.as_slice(),
            [
                SettingsUiStatePatch::Settings(_),
                SettingsUiStatePatch::UpdateStatus(_)
            ]
        ));
    }
}
