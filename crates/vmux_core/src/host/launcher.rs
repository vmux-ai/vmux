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

/// An empty stack waiting for a launcher to fill it, and what to undo if it never does.
///
/// The workspace stages this when a launcher is about to open onto fresh space - Cmd+T, or Cmd+K
/// on an empty pane - and the launcher consumes it. Both sides need it, so it belongs to neither:
/// the workspace cannot depend on the launcher, and the launcher must not depend on the workspace.
#[derive(Resource, Default, Debug)]
pub struct PendingLaunch {
    /// The empty stack staged for whatever the user picks.
    pub stack: Option<Entity>,
    /// Where focus was beforehand, to go back to on dismiss.
    pub previous_stack: Option<Entity>,
    /// The launcher has been asked to open and has not managed it yet.
    pub needs_open: bool,
    pub dismiss_modal: bool,
}

/// A page that hosts a launcher of its own.
///
/// The start page carries this. Two things follow from it, and both used to be decided by
/// comparing the page's URL against `vmux://start/` from outside: the launcher shortcut focuses
/// this page's input rather than opening a second launcher over it, and picking something the page
/// can become morphs it in place rather than opening a page beside it.
///
/// Asking for the capability means the command bar never learns which page this is, and a second
/// page that wants the same behaviour needs no change to the bar.
#[derive(Component, Clone, Copy, Debug)]
pub struct HostsLauncher;

/// Put the caret in the launcher this page hosts, rather than opening one over it.
#[derive(Message, Clone, Copy, Debug)]
pub struct FocusLauncherInput {
    pub webview: Entity,
}

/// The page in `stack` can become what was just picked, so it should morph rather than be replaced.
///
/// What "morph" means belongs to the page — the launcher only reports that the opportunity is
/// there, having checked the target is something the page can turn into.
#[derive(Message, Clone, Copy, Debug)]
pub struct InlineTransitionRequested {
    pub stack: Entity,
    pub webview: Entity,
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
