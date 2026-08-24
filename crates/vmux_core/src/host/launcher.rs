use bevy::prelude::*;

#[derive(Message, Clone, Debug)]
pub struct ContributedCommandChosen {
    pub id: String,
    pub stack: Option<Entity>,
    pub pane: Option<Entity>,
}

#[derive(Resource, Default, Debug)]
pub struct PendingLaunch {
    pub stack: Option<Entity>,
    pub previous_stack: Option<Entity>,
    pub needs_open: bool,
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
pub struct FocusLauncherInput {
    pub webview: Entity,
}

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

#[derive(Message, Clone, Debug)]
pub struct PendingStackAbandoned {
    pub stack: Entity,
    pub previous_stack: Option<Entity>,
}
