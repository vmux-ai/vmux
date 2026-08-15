//! The command bar's host half: opening it, revealing it, and acting on what was picked.
//!
//! It lived in `vmux_layout`, which made the window shell the implementer of a surface it only
//! hosts, and then in `vmux_browser`, which composes pages and could reach both sides while the
//! bar still read the workspace directly. Neither was its home.
//!
//! What made this crate impossible before was the direction of the dependency: answering "where
//! does this land" and "what tabs exist" meant naming `Stack`, `Pane`, `FocusedStack` and the rest,
//! and `vmux_layout` already depends on this crate. Those questions are asked through
//! `vmux_core::launcher` messages and a workspace snapshot now, so the bar states what it wants and
//! the workspace answers. Nothing here names the layout.
// Bevy queries in a host half this size are wide by nature; naming each one would add indirection
// without adding meaning.
#![allow(clippy::too_many_arguments, clippy::type_complexity)]

use bevy::prelude::*;

pub mod handler;
pub mod key;
pub mod panel;
pub mod state;
pub mod wake;
pub mod work_snapshot;

/// The bar's host half.
///
/// Separate from [`CommandPlugin`](crate::CommandPlugin) because these systems need a CEF host to
/// talk to, while the command vocabulary is used by crates that have none - several of them add
/// `CommandPlugin` in tests. Whoever owns the browser adds this.
pub struct CommandBarPlugin;

impl Plugin for CommandBarPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            handler::CommandBarInputPlugin,
            key::CommandBarKeyPlugin,
            panel::CommandBarPanelPlugin,
            wake::CommandBarWakePlugin,
        ));
    }
}
