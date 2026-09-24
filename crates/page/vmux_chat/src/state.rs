use crate::event::{ChatKey, ChatSnapshot, ComposerContext, ModeState, ModelState, SlashCommands};

#[vmux_api::ui_state_patch]
pub enum ChatUiStatePatch {
    Snapshot(Box<ChatSnapshot>),
    Composer(ComposerContext),
    Mode(ModeState),
    Model(ModelState),
    SlashCommands(SlashCommands),
    Key(ChatKey),
}

impl From<ChatSnapshot> for ChatUiStatePatch {
    fn from(snapshot: ChatSnapshot) -> Self {
        Self::Snapshot(Box::new(snapshot))
    }
}

#[vmux_api::ui_state(Default, targets = ["sessions", "agent", "start"])]
pub struct ChatUiState {
    pub sequence: u64,
    pub patches: Vec<ChatUiStatePatch>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batches_preserve_patch_order() {
        let event = ChatUiState {
            sequence: 3,
            patches: vec![ChatSnapshot::default().into(), ModelState::default().into()],
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&event).unwrap();
        let decoded = rkyv::from_bytes::<ChatUiState, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(decoded.sequence, 3);
        assert!(matches!(decoded.patches[0], ChatUiStatePatch::Snapshot(_)));
        assert!(matches!(decoded.patches[1], ChatUiStatePatch::Model(_)));
    }
}
