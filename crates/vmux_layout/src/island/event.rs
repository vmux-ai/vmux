use bevy::prelude::*;
use vmux_command::island::{IslandActivity, IslandActivityKind, IslandNotice};

/// Marker on the island OSR webview entity.
#[derive(Component)]
pub struct Island;

/// High-level island intents produced by feeds, the hotkey, and `Cmd+K`.
#[derive(Message, Clone)]
pub enum IslandEvent {
    ExpandSearch,
    Collapse,
    Activity(IslandActivity),
    ActivityEnded(IslandActivityKind),
    Notify(IslandNotice),
}

/// Request to expand the island into the command bar (from the global hotkey / `Cmd+K`).
#[derive(Message, Clone)]
pub struct SummonCommandBar;

/// ECS → native: show the panel (key it, enable mouse).
#[derive(Message, Clone)]
pub struct IslandPanelShow;

/// ECS → native: hide the panel (order out).
#[derive(Message, Clone)]
pub struct IslandPanelHide;

/// ECS → native: animate the panel frame to the page-reported content size.
#[derive(Message, Clone, Copy)]
pub struct IslandPanelResize {
    pub width: f32,
    pub height: f32,
}

/// Native → ECS: the panel resigned key (blur). Treated as a collapse request.
#[derive(Message, Clone)]
pub struct IslandPanelDismissed;
