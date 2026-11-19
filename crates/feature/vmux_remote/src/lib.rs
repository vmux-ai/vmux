pub use domain::{RemoteDomainPlugin, RemoteDomainSet};
#[cfg(feature = "host")]
pub use host::RemotePlugin;

mod domain;
#[cfg(feature = "host")]
mod host;

pub struct Feature;

impl vmux_feature::manifest::FeatureManifestSource for Feature {
    const SOURCE: &'static str = include_str!("../feature.ron");
}
