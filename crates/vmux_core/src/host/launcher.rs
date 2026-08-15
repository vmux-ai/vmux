//! What a launcher surface tells the workspace.
//!
//! A launcher — the command bar, the start page — opens *onto* the workspace without owning it. It
//! knows what the user picked; it does not know what a stack, a tab or a pane is. These are the
//! things it has to say, so that the crate that does own the workspace can answer them.
//!
//! They live here rather than with either side because neither is the author: the launcher cannot
//! depend on the workspace without inverting the layering, and the workspace must not be taught
//! which launcher is asking.

use bevy::prelude::*;

/// A launcher row contributed through `CommandBarContributions` was chosen.
///
/// The launcher does not know what the row means — whoever published the id answers this. `stack`
/// is the empty stack the launcher was opened on; when there is none, `pane` is the focused pane to
/// spawn into. Exactly one of the two is set.
#[derive(Message, Clone, Debug)]
pub struct ContributedCommandChosen {
    pub id: String,
    pub stack: Option<Entity>,
    pub pane: Option<Entity>,
}

/// The launcher was dismissed without ever using the empty stack it was opened on.
///
/// Disposing of it is the workspace's business, and more than a despawn: opening the launcher with
/// Cmd+T creates a whole tab to hold that one stack, and abandoning it should take the tab too.
/// Which of the two happened also decides where the keyboard goes back to, so the workspace
/// restores it rather than reporting back.
#[derive(Message, Clone, Debug)]
pub struct PendingStackAbandoned {
    pub stack: Entity,
    /// Where focus was before the launcher opened.
    pub previous_stack: Option<Entity>,
}
