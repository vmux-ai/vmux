#[cfg(host)]
use bevy::prelude::*;

#[cfg_attr(host, derive(Component, Reflect))]
#[cfg_attr(host, reflect(Component, Default))]
#[cfg_attr(host, type_path = "vmux_header::system")]
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
