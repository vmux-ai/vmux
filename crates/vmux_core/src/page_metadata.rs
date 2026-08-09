//! What a page shows in a tab strip or a bookmark: its title, URL, icon and background colour.
//!
//! Portable, so the web bundle can render it; the `Component` and `Reflect` derives are the only
//! part that needs a host.

#[cfg(not(web))]
use bevy::prelude::*;

#[cfg_attr(not(web), derive(Component, Reflect))]
#[cfg_attr(not(web), reflect(Component, Default))]
#[cfg_attr(not(web), type_path = "vmux_header::system")]
#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct PageMetadata {
    pub title: String,
    pub url: String,
    pub icon: crate::icon::PageIcon,
    pub bg_color: Option<String>,
}
