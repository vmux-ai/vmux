use bevy::prelude::*;

#[derive(Message, Clone, Debug)]
pub struct ContributedCommandChosen {
    pub id: String,
    pub stack: Option<Entity>,
    pub pane: Option<Entity>,
}

#[derive(Message, Clone, Copy, Debug, Default)]
pub struct LauncherDismissRequest;
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
