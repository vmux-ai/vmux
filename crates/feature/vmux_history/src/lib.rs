#[cfg(host)]
pub use host::{HistoryOpenIntent, HistoryPlugin};
pub use vmux_api::history as event;
#[cfg(host)]
pub use vmux_ecs::{CreatedAt, LastActivatedAt, Visit, now_millis};

#[cfg(host)]
pub(crate) struct Feature;

#[cfg(host)]
impl vmux_ecs::host::manifest::FeatureManifestSource for Feature {
    const SOURCE: &'static str = include_str!("feature.ron");
}

pub mod ranking;
pub mod state;
#[cfg(ui)]
pub mod ui;

#[cfg(host)]
mod host;
