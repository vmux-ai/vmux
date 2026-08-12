//! The editor types other crates are allowed to depend on.
//!
//! [`EditorPlugin`](crate::EditorPlugin) owns a filesystem watcher, an LSP client and the file
//! webview bridge. A crate that only wants to ask the editor to open a search needs none of that,
//! so the registrations live here on their own.

use bevy::prelude::*;

use crate::{FileViewModeRequest, GlobalSearchRequest};

/// Registers every editor message that crates outside `vmux_editor` send or read.
///
/// [`EditorPlugin`](crate::EditorPlugin) adds this, so a full app is unaffected. Add it directly
/// from a plugin that drives the editor without hosting it — `AgentSessionPlugin` does — instead
/// of restating the registrations locally.
///
/// Adding it more than once is deliberate and safe: `add_message` skips a type that is already
/// present, and [`Plugin::is_unique`] is `false` so repeated composition does not trip Bevy's
/// duplicate-plugin check.
pub struct EditorContractPlugin;

impl Plugin for EditorContractPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<FileViewModeRequest>()
            .add_message::<GlobalSearchRequest>();
    }

    fn is_unique(&self) -> bool {
        false
    }
}
