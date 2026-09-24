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

#[cfg(ui)]
pub(crate) struct LayoutUi<T> {
    state: dioxus::prelude::Signal<T>,
    received: dioxus::prelude::Signal<bool>,
}

#[cfg(ui)]
impl<T> Clone for LayoutUi<T> {
    fn clone(&self) -> Self {
        *self
    }
}

#[cfg(ui)]
impl<T> Copy for LayoutUi<T> {}

#[cfg(ui)]
impl<T> LayoutUi<T>
where
    T: Clone + 'static,
{
    pub(crate) fn value(self) -> T {
        (self.state)()
    }

    pub(crate) fn received(self) -> bool {
        (self.received)()
    }

    pub(crate) fn signal(self) -> dioxus::prelude::Signal<T> {
        self.state
    }
}

#[cfg(ui)]
pub(crate) fn use_layout_ui<T>() -> LayoutUi<T>
where
    LayoutUiStatePatch: vmux_api::UiStatePatch<T>,
    T: Clone + Default + 'static,
{
    use dioxus::prelude::*;

    let patches = use_layout_ui_patches::<T>();
    let mut state = use_signal(T::default);
    let mut received = use_signal(|| false);
    use_effect(move || {
        patches.for_each(|event| {
            state.set(event);
            received.set(true);
        });
    });
    LayoutUi { state, received }
}

#[cfg(ui)]
pub(crate) fn use_layout_ui_patches<T>() -> vmux_ui::hooks::UiStatePatchBatch<LayoutUiStateEvent, T>
where
    LayoutUiStatePatch: vmux_api::UiStatePatch<T>,
    T: Clone + 'static,
{
    vmux_ui::hooks::use_ui_state_patch::<LayoutUiStateEvent, T>()
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
