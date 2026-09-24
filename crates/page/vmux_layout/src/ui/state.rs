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
    ActiveSession, HeaderPageState, LayoutGeometry, PaneTreeState, RemoteUiState,
    StackNavigationState, TabBoundaryState, TabListState,
};
use crate::state::{LayoutUiState, LayoutUiStatePatch};

#[derive(Clone, Default)]
pub(crate) struct LayoutPageState {
    pub layout: Option<LayoutGeometry>,
    pub stacks: Option<StackNavigationState>,
    pub tabs: Option<TabListState>,
    pub bookmarks: BookmarkStateEvent,
    pub pane_tree: Option<PaneTreeState>,
    pub spaces: Option<SpacesListEvent>,
    pub projects: TabBoundaryState,
    pub active_session: Option<ActiveSession>,
    pub header_page: HeaderPageState,
    pub team: TeamEvent,
    pub remote: RemoteUiState,
    pub extensions: ExtensionsEvent,
    pub extension_popup: ExtensionPopupEvent,
    pub extension_popup_size: ExtensionPopupSizeEvent,
    pub update: Option<UpdatePhase>,
    pub bookmark_menu_action: BookmarkMenuActionEvent,
    pub reload_revision: u32,
}

impl LayoutPageState {
    fn use_state() -> LayoutUi {
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
        LayoutUi {
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
            LayoutUiStatePatch::ActiveSession(event) => self.active_session = event.session.clone(),
            LayoutUiStatePatch::HeaderPage(event) => self.header_page = event.clone(),
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

    pub(crate) fn overlay_ready(&self, error: &Option<String>) -> bool {
        let received = |ready| ready || error.is_some();
        let layout_ready = received(self.layout.is_some());
        let stacks_ready = received(self.stacks.is_some());
        let tabs_ready = received(self.tabs.is_some());
        let pane_tree_ready = received(self.pane_tree.is_some());
        let spaces_ready = received(self.spaces.is_some());
        let layout = self.layout.unwrap_or_default();

        layout_ready
            && (!layout.header_visible() || (stacks_ready && tabs_ready))
            && (!layout.side_sheet_open || (pane_tree_ready && spaces_ready))
    }
}

pub(crate) struct LayoutUi {
    state: Signal<LayoutPageState>,
    error: Signal<Option<String>>,
}

impl Clone for LayoutUi {
    fn clone(&self) -> Self {
        *self
    }
}

impl Copy for LayoutUi {}

impl LayoutUi {
    pub(crate) fn use_state() -> Self {
        LayoutPageState::use_state()
    }

    pub(crate) fn provide(self) {
        use_context_provider(|| self);
    }

    pub(crate) fn current() -> Self {
        use_context::<Self>()
    }

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
    use crate::event::{LayoutGeometry, ReloadEffect};

    fn state(header_open: bool, side_sheet_open: bool) -> LayoutPageState {
        LayoutPageState {
            layout: Some(LayoutGeometry {
                header_open,
                side_sheet_open,
                ..Default::default()
            }),
            ..Default::default()
        }
    }

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
        state.apply(&LayoutUiStatePatch::Reload(ReloadEffect));

        assert_eq!(state.bookmark_menu_action.sequence, 4);
        assert_eq!(state.bookmark_menu_action.action, "rename");
        assert_eq!(state.reload_revision, 1);
    }

    #[test]
    fn overlay_waits_for_layout_state() {
        assert!(!LayoutPageState::default().overlay_ready(&None));
    }

    #[test]
    fn overlay_waits_for_visible_header_state() {
        let mut state = state(true, false);

        assert!(!state.overlay_ready(&None));
        state.stacks = Some(Default::default());
        assert!(!state.overlay_ready(&None));
        state.tabs = Some(Default::default());
        assert!(state.overlay_ready(&None));
    }

    #[test]
    fn overlay_waits_for_visible_side_sheet_state() {
        let mut state = state(false, true);

        assert!(!state.overlay_ready(&None));
        state.pane_tree = Some(Default::default());
        assert!(!state.overlay_ready(&None));
        state.spaces = Some(Default::default());
        assert!(state.overlay_ready(&None));
    }

    #[test]
    fn closed_overlay_needs_only_layout_state() {
        assert!(state(false, false).overlay_ready(&None));
    }
}
