#![allow(clippy::too_many_arguments, clippy::type_complexity)]

#[cfg(all(host, feature = "app"))]
pub use host::AcpPlugin;
#[cfg(all(host, feature = "service"))]
pub use service::AcpServicePlugin;
#[cfg(all(host, feature = "app"))]
pub use session::AcpSessionId;

#[cfg(all(host, feature = "app"))]
pub(crate) struct Feature;

#[cfg(all(host, feature = "app"))]
impl vmux_ecs::manifest::FeatureManifestSource for Feature {
    const SOURCE: &'static str = include_str!("feature.ron");
}

#[cfg(all(host, feature = "service"))]
mod acp;
#[cfg(all(host, feature = "service"))]
mod broker_driver;
#[cfg(all(host, feature = "app"))]
mod host;
#[cfg(all(host, feature = "app"))]
mod managed_mcp_driver;
#[cfg(all(host, feature = "app"))]
mod mcp_driver;
#[cfg(all(host, feature = "app"))]
mod policy;
#[cfg(all(host, feature = "app"))]
mod policy_driver;
#[cfg(all(host, feature = "service"))]
mod remote_driver;
#[cfg(all(host, feature = "service"))]
mod service;
#[cfg(all(host, feature = "service"))]
mod service_driver;
#[cfg(all(host, feature = "app"))]
mod session;
