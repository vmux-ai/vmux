use crate::event::{
    ActiveSessionState, HeaderState, LayoutGeometry, PaneTreeState, ReloadEffect, RemoteUiState,
    SideSheetState, StackNavigationState, TabBoundaryState, TabListState, UpdateCleared,
    UpdateProgress, UpdateReady,
};
use vmux_api::bookmark::{BookmarkMenuEffect, BookmarkStateEvent};
use vmux_core::event::space::SpacesListEvent;
use vmux_core::event::{ExtensionPopupEvent, ExtensionPopupSizeEvent, ExtensionsEvent};

#[vmux_api::ui_state_patch(Default)]
pub struct LayoutUiStatePatch {
    pub layout: Option<LayoutGeometry>,
    pub stacks: Option<StackNavigationState>,
    pub tabs: Option<TabListState>,
    pub bookmarks: Option<BookmarkStateEvent>,
    pub pane_tree: Option<PaneTreeState>,
    pub side_sheet: Option<SideSheetState>,
    pub spaces: Option<SpacesListEvent>,
    pub projects: Option<TabBoundaryState>,
    pub remote: Option<RemoteUiState>,
    pub extensions: Option<ExtensionsEvent>,
    pub extension_popup: Option<ExtensionPopupEvent>,
    pub extension_popup_size: Option<ExtensionPopupSizeEvent>,
    pub update_progress: Option<UpdateProgress>,
    pub update_ready: Option<UpdateReady>,
    pub update_cleared: Option<UpdateCleared>,
    pub bookmark_menu: Option<BookmarkMenuEffect>,
    pub reload: Option<ReloadEffect>,
    pub active_session: Option<Box<ActiveSessionState>>,
    pub header: Option<HeaderState>,
}

#[vmux_api::ui_state(Default, url = "vmux://layout/")]
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
        assert!(decoded.patches[0].layout.is_some());
        assert!(decoded.patches[1].stacks.is_some());
    }
}
