use super::{CommandBarKey, CommandBarOpenEvent, PathCompleteResponse};
use crate::history::HistorySuggestionsResponse;

#[vmux_api::ui_state_patch]
pub enum CommandBarUiStatePatch {
    Snapshot(Box<CommandBarOpenEvent>),
    Key(CommandBarKey),
    PathCompletion(PathCompleteResponse),
    HistorySuggestions(HistorySuggestionsResponse),
}

impl From<CommandBarOpenEvent> for CommandBarUiStatePatch {
    fn from(snapshot: CommandBarOpenEvent) -> Self {
        Self::Snapshot(Box::new(snapshot))
    }
}

#[vmux_api::ui_state(Default, targets = ["command-bar", "start", "layout"])]
pub struct CommandBarUiState {
    pub sequence: u64,
    pub patches: Vec<CommandBarUiStatePatch>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batches_preserve_patch_order() {
        let state = CommandBarUiState {
            sequence: 4,
            patches: vec![
                CommandBarOpenEvent::default().into(),
                CommandBarKey::Next.into(),
                PathCompleteResponse::default().into(),
            ],
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&state).unwrap();
        let decoded = rkyv::from_bytes::<CommandBarUiState, rkyv::rancor::Error>(&bytes).unwrap();

        assert_eq!(decoded.sequence, 4);
        assert!(matches!(
            decoded.patches.as_slice(),
            [
                CommandBarUiStatePatch::Snapshot(_),
                CommandBarUiStatePatch::Key(CommandBarKey::Next),
                CommandBarUiStatePatch::PathCompletion(_),
            ]
        ));
    }
}
