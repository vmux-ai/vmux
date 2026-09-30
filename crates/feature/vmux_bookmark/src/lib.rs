#![allow(clippy::too_many_arguments, clippy::type_complexity)]

pub(crate) const FEATURE_MANIFEST: &str = include_str!("feature.ron");
pub(crate) type Feature = BookmarkToolPlugin;

mod menu;
mod persistence;
mod tool;

pub use menu::BookmarkPlugin;
pub use tool::BookmarkToolPlugin;
