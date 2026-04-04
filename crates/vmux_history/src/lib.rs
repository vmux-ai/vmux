pub mod event;

#[cfg(not(target_arch = "wasm32"))]
use bevy::prelude::*;
#[cfg(not(target_arch = "wasm32"))]
use chrono::{DateTime, Utc};

#[cfg(not(target_arch = "wasm32"))]
#[derive(Component, Clone, Copy, Debug)]
pub struct CreatedAt(pub DateTime<Utc>);

#[cfg(not(target_arch = "wasm32"))]
#[derive(Component, Clone, Copy, Debug)]
pub struct LastActivatedAt(pub DateTime<Utc>);

#[cfg(not(target_arch = "wasm32"))]
impl Default for CreatedAt {
    fn default() -> Self {
        Self(Utc::now())
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Default for LastActivatedAt {
    fn default() -> Self {
        Self(Utc::now())
    }
}

#[cfg(not(target_arch = "wasm32"))]
include!("plugin.rs");
