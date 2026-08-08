//! The command-bar wire vocabulary.
//!
//! The types themselves live in [`vmux_wire::command_bar`] so hosts that cannot link Bevy — the
//! native mobile client — can still render a launcher. This module re-exports them and adds the
//! Bevy-side resource wrapper.

pub use vmux_wire::command_bar::*;

/// The user's configured search provider, as an ECS resource.
///
/// [`SearchEngine`] itself is a portable wire type, so the `Resource` marker lives on this
/// wrapper rather than on the enum.
#[cfg(not(web))]
#[derive(bevy::prelude::Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SearchEngineSetting(pub SearchEngine);
