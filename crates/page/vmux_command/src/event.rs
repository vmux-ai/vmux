//! The command-bar wire vocabulary.
//!
//! The types themselves live in [`vmux_wire::command_bar`] so hosts that cannot link Bevy — the
//! native mobile client — can still render a launcher. This module re-exports them and adds the
//! Bevy-side resource wrapper.

pub use vmux_wire::command_bar::*;

use vmux_core::PageMetadata;

/// Page→host: act on a bookmark.
///
/// Lives beside the command vocabulary rather than with the layout page because the launcher
/// emits one too, and the launcher is this crate's.
#[derive(
    Clone,
    Debug,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct BookmarksCommandEvent {
    pub command: String,
    #[serde(default)]
    pub uuid: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub metadata: Option<PageMetadata>,
    #[serde(default)]
    pub folder: Option<String>,
}

/// The user's configured search provider, as an ECS resource.
///
/// [`SearchEngine`] itself is a portable wire type, so the `Resource` marker lives on this
/// wrapper rather than on the enum.
#[cfg(host)]
#[derive(bevy::prelude::Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SearchEngineSetting(pub SearchEngine);
