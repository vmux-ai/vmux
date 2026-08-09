use bevy::prelude::*;
use moonshine_save::prelude::*;

use crate::terminal::TerminalLaunch;

#[derive(Component, Clone, Debug, Reflect, Default)]
#[reflect(Component, Default)]
#[require(Save)]
#[type_path = "vmux_core::archive"]
pub struct ArchivedPage {
    pub url: String,
    pub title: String,
    pub space_id: String,
    pub closed_at: i64,
    pub launch: Option<TerminalLaunch>,
    pub tab_index: Option<usize>,
}

#[derive(Component, Clone, Debug, Reflect, Default)]
#[reflect(Component, Default)]
#[require(Save)]
#[type_path = "vmux_core::archive"]
pub struct ArchivedPagePosition {
    pub leaf_pane_id: String,
    pub stack_index: usize,
    pub pane_path: Vec<PaneStep>,
}

/// Membership and tab metadata for a page archived by a whole-tab close.
#[derive(Component, Clone, Debug, Reflect, Default)]
#[reflect(Component, Default)]
#[require(Save)]
#[type_path = "vmux_core::archive"]
pub struct ArchivedTabPage {
    pub group_id: String,
    pub tab_name: String,
    pub tab_startup_dir: Option<String>,
    pub active: bool,
}

#[derive(Clone, Debug, Reflect, Default, PartialEq)]
#[type_path = "vmux_core::archive"]
pub struct PaneStep {
    pub split_id: String,
    pub axis: SplitAxis,
    pub child_index: usize,
    pub flex_weights: Vec<f32>,
}

#[derive(Clone, Copy, Debug, Reflect, Default, PartialEq, Eq)]
#[type_path = "vmux_core::archive"]
pub enum SplitAxis {
    #[default]
    Row,
    Column,
}

#[derive(Message, Clone, Debug)]
pub struct PageArchiveRequest {
    pub url: String,
    pub title: String,
    pub space_id: String,
    pub launch: Option<TerminalLaunch>,
    pub tab_index: Option<usize>,
    pub leaf_pane_id: String,
    pub stack_index: usize,
    pub pane_path: Vec<PaneStep>,
}

#[cfg(test)]
#[path = "archive.test.rs"]
mod tests;
