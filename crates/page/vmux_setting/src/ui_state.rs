use crate::event::{SettingsListEvent, SettingsSchemaEvent, UpdateCheckStatusEvent};

#[vmux_api::payload]
#[derive(vmux_api::UiStatePatch)]
pub enum SettingsUiStatePatch {
    Settings(SettingsListEvent),
    Schema(SettingsSchemaEvent),
    UpdateStatus(UpdateCheckStatusEvent),
}

#[vmux_api::payload(Default)]
#[vmux_api::host_event(target = "settings")]
#[derive(vmux_api::UiState)]
pub struct SettingsUiStateEvent {
    pub sequence: u64,
    pub patches: Vec<SettingsUiStatePatch>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::UpdateCheckStatus;

    #[test]
    fn batches_preserve_patch_order() {
        let event = SettingsUiStateEvent {
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
        let decoded =
            rkyv::from_bytes::<SettingsUiStateEvent, rkyv::rancor::Error>(&bytes).unwrap();
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
