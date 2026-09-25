use bevy::prelude::*;

use crate::active_panes::{ActivatePane, ActivePanes};
use crate::apply::{
    LayoutApplyRequest, LayoutApplyResponse, LayoutSnapshotRequest, LayoutSnapshotResponse,
};
use crate::bookmark::{
    AddRequest, CreateFolderRequest, MoveFolderRequest, MovePinRequest, MoveRequest, PinRequest,
    PinUrlRequest, RemoveFolderRequest, RemoveRequest, RenameFolderRequest, RenameRequest,
    ReorderPinRequest, ShowBookmarkMenuRequest, ToggleFolderRequest, ToggleForUrlRequest,
    UnpinRequest,
};
use crate::pane::{OpenBesideRequest, SpawnCounter};
use crate::settings::{EffectiveStartupDir, EffectiveStartupUrl};
use crate::stack::{CloseStackRequest, FocusedStack};
use crate::worktree::TabDirectoryObserved;
use crate::{
    BrowserGoBackRequest, BrowserGoForwardRequest, BrowserNavigateRequest,
    ContributedCommandChosen, ExtensionInstallRequest, NewTabRequest, OpenInNewStackRequest,
    PendingLaunch,
};

pub struct LayoutContractPlugin;

impl Plugin for LayoutContractPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ActivePanes>()
            .init_resource::<EffectiveStartupDir>()
            .init_resource::<EffectiveStartupUrl>()
            .init_resource::<FocusedStack>()
            .init_resource::<PendingLaunch>()
            .init_resource::<SpawnCounter>()
            .add_message::<ActivatePane>()
            .add_message::<AddRequest>()
            .add_message::<BrowserGoBackRequest>()
            .add_message::<BrowserGoForwardRequest>()
            .add_message::<BrowserNavigateRequest>()
            .add_message::<CloseStackRequest>()
            .add_message::<ContributedCommandChosen>()
            .add_message::<ExtensionInstallRequest>()
            .add_message::<LayoutApplyRequest>()
            .add_message::<LayoutApplyResponse>()
            .add_message::<LayoutSnapshotRequest>()
            .add_message::<LayoutSnapshotResponse>()
            .add_message::<NewTabRequest>()
            .add_message::<OpenBesideRequest>()
            .add_message::<OpenInNewStackRequest>()
            .add_message::<CreateFolderRequest>()
            .add_message::<MoveFolderRequest>()
            .add_message::<MovePinRequest>()
            .add_message::<MoveRequest>()
            .add_message::<PinRequest>()
            .add_message::<PinUrlRequest>()
            .add_message::<RemoveFolderRequest>()
            .add_message::<RemoveRequest>()
            .add_message::<RenameFolderRequest>()
            .add_message::<RenameRequest>()
            .add_message::<ReorderPinRequest>()
            .add_message::<ShowBookmarkMenuRequest>()
            .add_message::<ToggleFolderRequest>()
            .add_message::<ToggleForUrlRequest>()
            .add_message::<TabDirectoryObserved>()
            .add_message::<UnpinRequest>();
    }

    fn is_unique(&self) -> bool {
        false
    }
}
