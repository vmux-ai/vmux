use crate::event::{
    ActiveSession, ActiveSessionState, BookmarkUiState, HeaderState, LayoutGeometry, PaneTreeState,
    ReloadEffect, RemoteUiState, SideSheetState, StackNavigationState, TabBoundaryState,
    TabListState, TabStripState, UpdateCleared, UpdateProgress, UpdateReady,
};
use vmux_api::bookmark::{BookmarkEditState, BookmarkMenuEffect, BookmarkStateEvent};
use vmux_api::extension::{ExtensionPopupEvent, ExtensionPopupSizeEvent, ExtensionsUiState};
use vmux_ecs::event::space::{SpaceFormState, SpacesListEvent};

#[vmux_api::ui_state_patch(Default)]
pub struct LayoutUiStatePatch {
    pub layout: Option<LayoutGeometry>,
    pub stacks: Option<StackNavigationState>,
    pub tabs: Option<TabListState>,
    pub tab_strip: Option<TabStripState>,
    pub bookmarks: Option<BookmarkStateEvent>,
    pub bookmark_ui: Option<BookmarkUiState>,
    pub pane_tree: Option<PaneTreeState>,
    pub side_sheet: Option<SideSheetState>,
    pub spaces: Option<SpacesListEvent>,
    pub space_form: Option<SpaceFormState>,
    pub projects: Option<TabBoundaryState>,
    pub remote: Option<RemoteUiState>,
    pub extensions: Option<ExtensionsUiState>,
    pub extension_popup: Option<ExtensionPopupEvent>,
    pub extension_popup_size: Option<ExtensionPopupSizeEvent>,
    pub update_progress: Option<UpdateProgress>,
    pub update_ready: Option<UpdateReady>,
    pub update_cleared: Option<UpdateCleared>,
    pub bookmark_menu: Option<BookmarkMenuEffect>,
    pub bookmark_edit: Option<BookmarkEditState>,
    pub reload: Option<ReloadEffect>,
    pub active_session: Option<Box<ActiveSessionState>>,
    pub header: Option<HeaderState>,
}

#[vmux_api::contract(Eq)]
pub enum UpdateStatus {
    Downloading {
        version: String,
        downloaded: u64,
        total: u64,
    },
    Installing {
        version: String,
    },
    Ready {
        version: String,
    },
}

#[vmux_api::ui_state(Default, patch = LayoutUiStatePatch, version = 2)]
pub struct LayoutUiState {
    pub layout: Option<LayoutGeometry>,
    pub stacks: Option<StackNavigationState>,
    pub tab_strip: Option<TabStripState>,
    pub bookmarks: BookmarkUiState,
    pub side_sheet: Option<SideSheetState>,
    pub projects: TabBoundaryState,
    pub active_session: Option<ActiveSession>,
    pub header: HeaderState,
    pub remote: RemoteUiState,
    pub extensions: ExtensionsUiState,
    pub extension_popup: ExtensionPopupEvent,
    pub extension_popup_size: ExtensionPopupSizeEvent,
    pub update: Option<UpdateStatus>,
    pub bookmark_menu: BookmarkMenuEffect,
    pub bookmark_edit: BookmarkEditState,
    pub space_form: SpaceFormState,
    pub reload_revision: u64,
}

impl vmux_api::UiStateProjection<LayoutUiStatePatch> for LayoutUiState {
    fn apply(&mut self, patch: LayoutUiStatePatch) {
        if let Some(event) = patch.layout {
            self.layout = Some(event);
        }
        if let Some(event) = patch.stacks {
            self.stacks = Some(event);
        }
        if let Some(event) = patch.tab_strip {
            self.tab_strip = Some(event);
        }
        if let Some(event) = patch.bookmark_ui {
            self.bookmarks = event;
        }
        if let Some(event) = patch.side_sheet {
            self.side_sheet = Some(event);
        }
        if let Some(event) = patch.projects {
            self.projects = event;
        }
        if let Some(event) = patch.active_session {
            self.active_session = event.session;
        }
        if let Some(event) = patch.header {
            self.header = event;
        }
        if let Some(event) = patch.remote {
            self.remote = event;
        }
        if let Some(event) = patch.extensions {
            self.extensions = event;
        }
        if let Some(event) = patch.extension_popup {
            self.extension_popup = event;
        }
        if let Some(event) = patch.extension_popup_size {
            self.extension_popup_size = event;
        }
        if let Some(event) = patch.update_progress {
            self.update = Some(if event.installing {
                UpdateStatus::Installing {
                    version: event.version,
                }
            } else {
                UpdateStatus::Downloading {
                    version: event.version,
                    downloaded: event.downloaded,
                    total: event.total,
                }
            });
        }
        if let Some(event) = patch.update_ready {
            self.update = Some(UpdateStatus::Ready {
                version: event.version,
            });
        }
        if patch.update_cleared.is_some() {
            self.update = None;
        }
        if let Some(event) = patch.bookmark_menu {
            self.bookmark_menu = event;
        }
        if let Some(event) = patch.bookmark_edit {
            self.bookmark_edit = event;
        }
        if let Some(event) = patch.space_form {
            self.space_form = event;
        }
        if let Some(effect) = patch.reload {
            self.reload_revision = self.reload_revision.max(effect.revision);
        }
    }
}

impl LayoutUiState {
    pub fn overlay_ready(&self, error: &Option<String>) -> bool {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn state(header_open: bool, side_sheet_open: bool) -> LayoutUiState {
        LayoutUiState {
            layout: Some(LayoutGeometry {
                header_open,
                side_sheet_open,
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[test]
    fn patches_build_a_retained_tree() {
        let mut state = LayoutUiState::default();

        vmux_api::UiStateProjection::apply(
            &mut state,
            BookmarkMenuEffect {
                rename: vmux_api::bookmark::BookmarkRenameEffect {
                    revision: 4,
                    uuid: "bookmark".to_string(),
                },
                ..Default::default()
            }
            .into(),
        );
        vmux_api::UiStateProjection::apply(&mut state, ReloadEffect { revision: 7 }.into());

        assert_eq!(state.bookmark_menu.rename.revision, 4);
        assert_eq!(state.bookmark_menu.rename.uuid, "bookmark");
        assert_eq!(state.reload_revision, 7);
    }

    #[test]
    fn overlay_waits_for_layout_state() {
        assert!(!LayoutUiState::default().overlay_ready(&None));
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
