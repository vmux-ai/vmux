use crate::event::{
    ActiveSessionEvent, HeaderPageEvent, LayoutStateEvent, PaneTreeEvent, ReloadEvent,
    RemoteUiState, StacksHostEvent, TabBoundaryEvent, TabsHostEvent, UpdateClearedEvent,
    UpdateProgressEvent, UpdateReadyEvent,
};
use vmux_api::bookmark::{BookmarkMenuActionEvent, BookmarkStateEvent};
use vmux_core::event::space::SpacesListEvent;
use vmux_core::event::team::TeamEvent;
use vmux_core::event::{ExtensionPopupEvent, ExtensionPopupSizeEvent, ExtensionsEvent};

#[vmux_api::ui_state_patch]
pub enum LayoutUiStatePatch {
    Layout(LayoutStateEvent),
    Stacks(StacksHostEvent),
    Tabs(TabsHostEvent),
    Bookmarks(BookmarkStateEvent),
    PaneTree(PaneTreeEvent),
    Spaces(SpacesListEvent),
    Projects(TabBoundaryEvent),
    Team(TeamEvent),
    Remote(RemoteUiState),
    Extensions(ExtensionsEvent),
    ExtensionPopup(ExtensionPopupEvent),
    ExtensionPopupSize(ExtensionPopupSizeEvent),
    UpdateProgress(UpdateProgressEvent),
    UpdateReady(UpdateReadyEvent),
    UpdateCleared(UpdateClearedEvent),
    BookmarkMenuAction(BookmarkMenuActionEvent),
    Reload(ReloadEvent),
    ActiveSession(Box<ActiveSessionEvent>),
    HeaderPage(HeaderPageEvent),
}

#[vmux_api::ui_state(Default, target = "layout")]
pub struct LayoutUiState {
    pub sequence: u64,
    pub patches: Vec<LayoutUiStatePatch>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batches_preserve_patch_order() {
        let event = LayoutUiState {
            sequence: 4,
            patches: vec![
                LayoutStateEvent::default().into(),
                StacksHostEvent::default().into(),
            ],
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&event).unwrap();
        let decoded = rkyv::from_bytes::<LayoutUiState, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(decoded.sequence, 4);
        assert!(matches!(decoded.patches[0], LayoutUiStatePatch::Layout(_)));
        assert!(matches!(decoded.patches[1], LayoutUiStatePatch::Stacks(_)));
    }
}
