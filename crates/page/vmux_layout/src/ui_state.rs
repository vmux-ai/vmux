use crate::event::{
    LayoutStateEvent, PaneTreeEvent, RemoteStateEvent, StacksHostEvent, TabBoundaryEvent,
    TabsHostEvent, UpdateClearedEvent, UpdateProgressEvent, UpdateReadyEvent,
};
use vmux_api::bookmark::BookmarkStateEvent;
use vmux_core::event::space::SpacesListEvent;
use vmux_core::event::team::TeamEvent;
use vmux_core::event::{ExtensionPopupEvent, ExtensionPopupSizeEvent, ExtensionsEvent};

#[vmux_api::payload]
#[derive(vmux_api::UiStatePatch)]
pub enum LayoutUiStatePatch {
    Layout(LayoutStateEvent),
    Stacks(StacksHostEvent),
    Tabs(TabsHostEvent),
    Bookmarks(BookmarkStateEvent),
    PaneTree(PaneTreeEvent),
    Spaces(SpacesListEvent),
    Projects(TabBoundaryEvent),
    Team(TeamEvent),
    Remote(RemoteStateEvent),
    Extensions(ExtensionsEvent),
    ExtensionPopup(ExtensionPopupEvent),
    ExtensionPopupSize(ExtensionPopupSizeEvent),
    UpdateProgress(UpdateProgressEvent),
    UpdateReady(UpdateReadyEvent),
    UpdateCleared(UpdateClearedEvent),
}

#[vmux_api::payload(Default)]
#[vmux_api::host_event(target = "layout")]
#[derive(vmux_api::UiState)]
pub struct LayoutUiStateEvent {
    pub sequence: u64,
    pub patches: Vec<LayoutUiStatePatch>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batches_preserve_patch_order() {
        let event = LayoutUiStateEvent {
            sequence: 4,
            patches: vec![
                LayoutStateEvent::default().into(),
                StacksHostEvent::default().into(),
            ],
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&event).unwrap();
        let decoded = rkyv::from_bytes::<LayoutUiStateEvent, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(decoded.sequence, 4);
        assert!(matches!(decoded.patches[0], LayoutUiStatePatch::Layout(_)));
        assert!(matches!(decoded.patches[1], LayoutUiStatePatch::Stacks(_)));
    }
}
