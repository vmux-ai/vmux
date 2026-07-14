//! Host-side git engine: runs git operations on background threads and bridges results
//! to the `files://` editor page.

pub mod event;
pub mod view;

#[cfg(target_arch = "wasm32")]
pub mod ui;

#[cfg(not(target_arch = "wasm32"))]
pub mod highlight;
#[cfg(not(target_arch = "wasm32"))]
pub mod job;
#[cfg(not(target_arch = "wasm32"))]
pub mod parse;
#[cfg(not(target_arch = "wasm32"))]
pub mod runner;
#[cfg(not(target_arch = "wasm32"))]
pub mod worktree;

#[cfg(not(target_arch = "wasm32"))]
use bevy::prelude::*;

#[cfg(not(target_arch = "wasm32"))]
#[derive(Component, Clone, Debug, Default)]
pub struct GitDiffSource {
    pub content: String,
    pub dirty: bool,
}

pub const FILES_HOST: &str = "files";

#[cfg(not(target_arch = "wasm32"))]
include!("plugin.rs");
