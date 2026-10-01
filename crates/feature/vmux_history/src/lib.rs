#[cfg(host)]
pub(crate) struct Feature;

#[cfg(host)]
impl vmux_core::host::manifest::FeatureManifestSource for Feature {
    const SOURCE: &'static str = include_str!("feature.ron");
}

pub use vmux_api::history as event;
pub mod ranking;
pub mod state;
#[cfg(ui)]
pub mod ui;

#[cfg(host)]
pub use vmux_core::{CreatedAt, LastActivatedAt, Visit, now_millis};

#[cfg(host)]
mod host;
#[cfg(host)]
pub use host::{HistoryOpenIntent, HistoryPlugin};
