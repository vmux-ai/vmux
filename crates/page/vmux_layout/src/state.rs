use crate::event::{
    ActiveSessionState, HeaderPageState, LayoutGeometry, PaneTreeState, ReloadEffect,
    RemoteUiState, StackNavigationState, TabBoundaryState, TabListState, UpdateCleared,
    UpdateProgress, UpdateReady,
};
use vmux_api::bookmark::{BookmarkMenuEffect, BookmarkStateEvent};
use vmux_core::event::space::SpacesListEvent;
use vmux_core::event::team::TeamEvent;
use vmux_core::event::{ExtensionPopupEvent, ExtensionPopupSizeEvent, ExtensionsEvent};

#[vmux_api::ui_state_patch]
pub enum LayoutUiStatePatch {
    Layout(LayoutGeometry),
    Stacks(StackNavigationState),
    Tabs(TabListState),
    Bookmarks(BookmarkStateEvent),
    PaneTree(PaneTreeState),
    Spaces(SpacesListEvent),
    Projects(TabBoundaryState),
    Team(TeamEvent),
    Remote(RemoteUiState),
    Extensions(ExtensionsEvent),
    ExtensionPopup(ExtensionPopupEvent),
    ExtensionPopupSize(ExtensionPopupSizeEvent),
    UpdateProgress(UpdateProgress),
    UpdateReady(UpdateReady),
    UpdateCleared(UpdateCleared),
    BookmarkMenu(BookmarkMenuEffect),
    Reload(ReloadEffect),
    ActiveSession(Box<ActiveSessionState>),
    HeaderPage(HeaderPageState),
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
                LayoutGeometry::default().into(),
                StackNavigationState::default().into(),
            ],
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&event).unwrap();
        let decoded = rkyv::from_bytes::<LayoutUiState, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(decoded.sequence, 4);
        assert!(matches!(decoded.patches[0], LayoutUiStatePatch::Layout(_)));
        assert!(matches!(decoded.patches[1], LayoutUiStatePatch::Stacks(_)));
    }
}
