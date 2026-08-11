//! The layout types other crates are allowed to depend on.
//!
//! A crate that sends `NewTabRequest` or reads `ActivePanes` needs those registered before its
//! own systems run, but pulling in the plugin that owns them — `TabPlugin`, `PanePlugin` — drags
//! along thousands of lines of unrelated behaviour. [`LayoutContractPlugin`] is the declaration
//! on its own: no systems, no observers, nothing but the `add_message` and `init_resource` calls
//! that make the contract exist.

use bevy::prelude::*;

use crate::active_panes::{ActivatePane, ActivePanes};
use crate::apply::{
    LayoutApplyRequest, LayoutApplyResponse, LayoutSnapshotRequest, LayoutSnapshotResponse,
};
use crate::bookmark::{BookmarkOp, ShowBookmarkMenuRequest};
use crate::pane::{OpenBesideRequest, SpawnCounter};
use crate::settings::{EffectiveStartupDir, EffectiveStartupUrl};
use crate::space::ActiveSpaceId;
use crate::stack::{CloseStackRequest, FocusedStack};
use crate::worktree::TabDirectoryObserved;
use crate::{
    BrowserGoBackRequest, BrowserGoForwardRequest, BrowserNavigateRequest,
    ContributedCommandChosen, ExtensionInstallRequest, NewStackContext, NewTabRequest,
    OpenInNewStackRequest,
};

/// Registers every layout message and resource that crates outside `vmux_layout` send, read or
/// spawn into.
///
/// [`LayoutPlugin`](crate::plugin::LayoutPlugin) adds this, so a full app is unaffected. Add it
/// directly from a plugin that talks to the layout without hosting it — `AgentSessionPlugin` and
/// the settings and spaces domains all do — instead of restating the registrations locally, which
/// leaves the owning crate unable to rename or retire a type.
///
/// Adding it more than once is deliberate and safe: `add_message` and `init_resource` both skip a
/// type that is already present, and [`Plugin::is_unique`] is `false` so repeated composition does
/// not trip Bevy's duplicate-plugin check.
pub struct LayoutContractPlugin;

impl Plugin for LayoutContractPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ActivePanes>()
            .init_resource::<ActiveSpaceId>()
            .init_resource::<EffectiveStartupDir>()
            .init_resource::<EffectiveStartupUrl>()
            .init_resource::<FocusedStack>()
            .init_resource::<NewStackContext>()
            .init_resource::<SpawnCounter>()
            .add_message::<ActivatePane>()
            .add_message::<BookmarkOp>()
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
            .add_message::<ShowBookmarkMenuRequest>()
            .add_message::<TabDirectoryObserved>();
    }

    fn is_unique(&self) -> bool {
        false
    }
}
