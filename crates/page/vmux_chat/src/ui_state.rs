use crate::event::{ChatSnapshot, ComposerContext, ModeState, ModelState, SlashCommands};

#[vmux_api::payload]
#[derive(vmux_api::UiStatePatch)]
pub enum ChatUiStatePatch {
    Snapshot(Box<ChatSnapshot>),
    Composer(ComposerContext),
    Mode(ModeState),
    Model(ModelState),
    SlashCommands(SlashCommands),
}

impl From<ChatSnapshot> for ChatUiStatePatch {
    fn from(snapshot: ChatSnapshot) -> Self {
        Self::Snapshot(Box::new(snapshot))
    }
}

#[vmux_api::payload(Default)]
#[vmux_api::host_event(targets = ["sessions", "agent", "start"])]
#[derive(vmux_api::UiState)]
pub struct ChatUiStateEvent {
    pub sequence: u64,
    pub patches: Vec<ChatUiStatePatch>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batches_preserve_patch_order() {
        let event = ChatUiStateEvent {
            sequence: 3,
            patches: vec![ChatSnapshot::default().into(), ModelState::default().into()],
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&event).unwrap();
        let decoded = rkyv::from_bytes::<ChatUiStateEvent, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(decoded.sequence, 3);
        assert!(matches!(decoded.patches[0], ChatUiStatePatch::Snapshot(_)));
        assert!(matches!(decoded.patches[1], ChatUiStatePatch::Model(_)));
    }
}
