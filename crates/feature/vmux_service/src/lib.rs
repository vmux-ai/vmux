#[cfg(host)]
pub use host::{
    AuthorizationOutcome, AuthorizedDevice, DaemonBinary, DaemonIdentity, DeviceId, LaunchAgent,
    RelayToken, RemoteAuthorizationStore, RemotePaths, ServiceCliPlugin, ServicePaths, bundle,
    cleanup, cli, pairing, plugin, registry, runner, server, supervisor,
};
#[cfg(all(host, target_os = "macos"))]
pub use host::{launchd, sm_app_service};
pub use vmux_api::service as event;

#[cfg(host)]
pub(crate) struct Feature;

#[cfg(host)]
impl vmux_ecs::host::manifest::FeatureManifestSource for Feature {
    const SOURCE: &'static str = include_str!("feature.ron");
}

pub mod remote;

#[cfg(host)]
mod host;
