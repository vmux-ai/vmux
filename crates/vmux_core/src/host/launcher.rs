use bevy::prelude::*;

#[derive(Message, Clone, Debug)]
pub struct ContributedCommandChosen {
    pub id: String,
    pub stack: Option<Entity>,
    pub pane: Option<Entity>,
}

/// Set when something other than the launcher has taken over the surface it was aimed at.
///
/// The launcher is a modal over whichever stack is focused. Opening a tab, pane or stack moves
/// that focus out from under it, so it has to go.
#[derive(Resource, Default, Debug)]
pub struct PendingLaunch {
    pub dismiss_modal: bool,
}

impl PendingLaunch {
    pub fn opened_elsewhere(&mut self) {
        self.dismiss_modal = true;
    }
}
#[derive(Component, Clone, Copy, Debug)]
pub struct HostsLauncher;

#[derive(Message, Clone, Copy, Debug)]
pub struct InlineTransitionRequested {
    pub stack: Entity,
    pub webview: Entity,
}

#[derive(Component, Clone, Copy, Debug)]
pub struct RendersLauncherPanel;

#[derive(Message, Clone, Copy, Debug)]
pub struct RestoreKeyboardToStack {
    pub stack: Entity,
}

#[derive(Message, Clone, Copy, Debug)]
pub struct StackInPaneChosen {
    pub pane_bits: u64,
    pub index: usize,
}
