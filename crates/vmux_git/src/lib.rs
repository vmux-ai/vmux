//! Host-side git engine: runs git operations on background threads and bridges results
//! to the `files://` editor page.

pub mod event;
pub mod view;

#[cfg(web)]
pub mod ui;

#[cfg(not(web))]
pub mod highlight;
#[cfg(not(web))]
pub mod job;
#[cfg(not(web))]
pub mod parse;
#[cfg(not(web))]
pub mod runner;
#[cfg(not(web))]
pub mod worktree;

#[cfg(not(web))]
use bevy::prelude::*;

#[cfg(not(web))]
#[derive(Component, Clone, Debug, Default)]
pub struct GitDiffSource {
    pub content: String,
    pub dirty: bool,
}

pub const FILES_HOST: &str = "files";

#[cfg(not(web))]
include!("plugin.rs");
