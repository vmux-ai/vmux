use dioxus::prelude::*;
use vmux_api::bookmark::BookmarkMenuEffect;
use vmux_api::extension::{ExtensionPopupEvent, ExtensionPopupSizeEvent, ExtensionsEvent};
use vmux_ui::hooks::use_ui_state;

use super::update::UpdatePhase;
use crate::event::{
    ActiveSession, BookmarkUiState, HeaderState, LayoutGeometry, RemoteUiState, SideSheetState,
    StackNavigationState, TabBoundaryState, TabStripState,
};
use crate::state::{LayoutUiState, LayoutUiStatePatch};

#[derive(Clone, Default)]
pub(super) struct LayoutPageState {
    pub layout: Option<LayoutGeometry>,
    pub stacks: Option<StackNavigationState>,
    pub tab_strip: Option<TabStripState>,
    pub bookmarks: BookmarkUiState,
    pub side_sheet: Option<SideSheetState>,
    pub projects: TabBoundaryState,
    pub active_session: Option<ActiveSession>,
    pub header: HeaderState,
    pub remote: RemoteUiState,
    pub extensions: ExtensionsEvent,
    pub extension_popup: ExtensionPopupEvent,
    pub extension_popup_size: ExtensionPopupSizeEvent,
    pub update: Option<UpdatePhase>,
    pub bookmark_menu: BookmarkMenuEffect,
    pub reload_revision: u64,
}

impl LayoutPageState {
    fn use_state() -> LayoutUi {
        let root = use_ui_state::<LayoutUiState>().use_projection(Self::apply);
        LayoutUi {
            state: root.state,
            error: root.error,
        }
    }

    fn apply(&mut self, patch: &LayoutUiStatePatch) {
        if let Some(event) = patch.layout {
            self.layout = Some(event);
        }
        if let Some(event) = &patch.stacks {
            self.stacks = Some(event.clone());
        }
        if let Some(event) = &patch.tab_strip {
            self.tab_strip = Some(event.clone());
        }
        if let Some(event) = &patch.bookmark_ui {
            self.bookmarks = event.clone();
        }
        if let Some(event) = &patch.side_sheet {
            self.side_sheet = Some(event.clone());
        }
        if let Some(event) = &patch.projects {
            self.projects = event.clone();
        }
        if let Some(event) = &patch.active_session {
            self.active_session = event.session.clone();
        }
        if let Some(event) = &patch.header {
            self.header = event.clone();
        }
        if let Some(event) = &patch.remote {
            self.remote = event.clone();
        }
        if let Some(event) = &patch.extensions {
            self.extensions = event.clone();
        }
        if let Some(event) = &patch.extension_popup {
            self.extension_popup = event.clone();
        }
        if let Some(event) = &patch.extension_popup_size {
            self.extension_popup_size = event.clone();
        }
        if let Some(event) = &patch.update_progress {
            self.update = Some(UpdatePhase::from(event));
        }
        if let Some(event) = &patch.update_ready {
            self.update = Some(UpdatePhase::from(event));
        }
        if patch.update_cleared.is_some() {
            self.update = None;
        }
        if let Some(event) = &patch.bookmark_menu {
            self.bookmark_menu = event.clone();
        }
        if let Some(effect) = &patch.reload {
            self.reload_revision = self.reload_revision.max(effect.revision);
        }
    }

    pub(super) fn overlay_ready(&self, error: &Option<String>) -> bool {
        let received = |ready| ready || error.is_some();
        let layout_ready = received(self.layout.is_some());
        let stacks_ready = received(self.stacks.is_some());
        let tabs_ready = received(self.tab_strip.is_some());
        let side_sheet_ready = received(self.side_sheet.is_some());
        let layout = self.layout.unwrap_or_default();

        layout_ready
            && (!layout.header_visible() || (stacks_ready && tabs_ready))
            && (!layout.side_sheet_open || side_sheet_ready)
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

    pub(super) fn value(self) -> LayoutPageState {
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

        state.apply(
            &BookmarkMenuEffect {
                rename: vmux_api::bookmark::BookmarkRenameEffect {
                    revision: 4,
                    uuid: "bookmark".to_string(),
                },
                ..Default::default()
            }
            .into(),
        );
        state.apply(&ReloadEffect { revision: 7 }.into());

        assert_eq!(state.bookmark_menu.rename.revision, 4);
        assert_eq!(state.bookmark_menu.rename.uuid, "bookmark");
        assert_eq!(state.reload_revision, 7);
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
        state.tab_strip = Some(Default::default());
        assert!(state.overlay_ready(&None));
    }

    #[test]
    fn overlay_waits_for_visible_side_sheet_state() {
        let mut state = state(false, true);

        assert!(!state.overlay_ready(&None));
        state.side_sheet = Some(Default::default());
        assert!(state.overlay_ready(&None));
    }

    #[test]
    fn closed_overlay_needs_only_layout_state() {
        assert!(state(false, false).overlay_ready(&None));
    }
}
