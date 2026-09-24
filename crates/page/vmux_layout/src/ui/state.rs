use dioxus::prelude::*;
use vmux_api::bookmark::{BookmarkMenuActionEvent, BookmarkStateEvent};
use vmux_core::event::space::SpacesListEvent;
use vmux_core::event::team::TeamEvent;
use vmux_core::event::{
    ExtListRequest, ExtensionPopupEvent, ExtensionPopupSizeEvent, ExtensionsEvent,
};
use vmux_ui::hooks::{send, use_ui_state_root};

use super::update::UpdatePhase;
use crate::event::{
    LayoutStateEvent, PaneTreeEvent, RemoteStateEvent, StacksHostEvent, TabBoundaryEvent,
    TabsHostEvent,
};
use crate::state::{LayoutUiState, LayoutUiStatePatch};

#[derive(Clone, Default)]
pub(crate) struct LayoutPageState {
    pub layout: Option<LayoutStateEvent>,
    pub stacks: Option<StacksHostEvent>,
    pub tabs: Option<TabsHostEvent>,
    pub bookmarks: BookmarkStateEvent,
    pub pane_tree: Option<PaneTreeEvent>,
    pub spaces: Option<SpacesListEvent>,
    pub projects: TabBoundaryEvent,
    pub team: TeamEvent,
    pub remote: RemoteStateEvent,
    pub extensions: ExtensionsEvent,
    pub extension_popup: ExtensionPopupEvent,
    pub extension_popup_size: ExtensionPopupSizeEvent,
    pub update: Option<UpdatePhase>,
    pub bookmark_menu_action: BookmarkMenuActionEvent,
    pub reload_revision: u32,
}

impl LayoutPageState {
    pub(crate) fn use_state() -> LayoutPageStateSubscription {
        let root = use_ui_state_root::<LayoutUiState>();
        let mut state = use_signal(Self::default);
        let mut handled_sequence = use_signal(|| 0);
        use_effect(move || {
            let event = root.state.read();
            if event.sequence == 0 || event.sequence == *handled_sequence.peek() {
                return;
            }
            handled_sequence.set(event.sequence);
            state.with_mut(|state| {
                for patch in &event.patches {
                    state.apply(patch);
                }
            });
        });
        use_effect(move || {
            let _ = send(&ExtListRequest);
        });
        LayoutPageStateSubscription {
            state,
            error: root.error,
        }
    }

    fn apply(&mut self, patch: &LayoutUiStatePatch) {
        match patch {
            LayoutUiStatePatch::Layout(event) => self.layout = Some(*event),
            LayoutUiStatePatch::Stacks(event) => self.stacks = Some(event.clone()),
            LayoutUiStatePatch::Tabs(event) => self.tabs = Some(event.clone()),
            LayoutUiStatePatch::Bookmarks(event) => self.bookmarks = event.clone(),
            LayoutUiStatePatch::PaneTree(event) => self.pane_tree = Some(event.clone()),
            LayoutUiStatePatch::Spaces(event) => self.spaces = Some(event.clone()),
            LayoutUiStatePatch::Projects(event) => self.projects = event.clone(),
            LayoutUiStatePatch::Team(event) => self.team = event.clone(),
            LayoutUiStatePatch::Remote(event) => self.remote = event.clone(),
            LayoutUiStatePatch::Extensions(event) => self.extensions = event.clone(),
            LayoutUiStatePatch::ExtensionPopup(event) => self.extension_popup = event.clone(),
            LayoutUiStatePatch::ExtensionPopupSize(event) => {
                self.extension_popup_size = event.clone()
            }
            LayoutUiStatePatch::UpdateProgress(event) => {
                self.update = Some(UpdatePhase::from(event))
            }
            LayoutUiStatePatch::UpdateReady(event) => self.update = Some(UpdatePhase::from(event)),
            LayoutUiStatePatch::UpdateCleared(_) => self.update = None,
            LayoutUiStatePatch::BookmarkMenuAction(event) => {
                self.bookmark_menu_action = event.clone()
            }
            LayoutUiStatePatch::Reload(_) => {
                self.reload_revision = self.reload_revision.wrapping_add(1)
            }
        }
    }
}

pub(crate) struct LayoutPageStateSubscription {
    state: Signal<LayoutPageState>,
    error: Signal<Option<String>>,
}

impl Clone for LayoutPageStateSubscription {
    fn clone(&self) -> Self {
        *self
    }
}

impl Copy for LayoutPageStateSubscription {}

impl LayoutPageStateSubscription {
    pub(crate) fn value(self) -> LayoutPageState {
        (self.state)()
    }

    pub(crate) fn error(self) -> Option<String> {
        (self.error)()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::ReloadEvent;

    #[test]
    fn transient_layout_effects_are_applied_from_ui_state() {
        let mut state = LayoutPageState::default();

        state.apply(&LayoutUiStatePatch::BookmarkMenuAction(
            BookmarkMenuActionEvent {
                sequence: 4,
                action: "rename".to_string(),
                uuid: Some("bookmark".to_string()),
            },
        ));
        state.apply(&LayoutUiStatePatch::Reload(ReloadEvent));

        assert_eq!(state.bookmark_menu_action.sequence, 4);
        assert_eq!(state.bookmark_menu_action.action, "rename");
        assert_eq!(state.reload_revision, 1);
    }
}
