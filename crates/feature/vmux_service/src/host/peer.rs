use std::path::Path;

use bevy_ecs::prelude::Component;
use vmux_transport::service_peer::{CodeRequirement, LocalPeer};

#[derive(Component)]
pub struct TrustedDesktop {
    pub(super) requirement: Option<CodeRequirement>,
    pub(super) uid: u32,
}

impl TrustedDesktop {
    pub(super) fn admits(&self, peer: &LocalPeer) -> bool {
        peer.uid() == self.uid && peer.verified_code()
    }

    pub fn for_binary(path: &Path) -> std::io::Result<Self> {
        Ok(Self {
            requirement: Some(CodeRequirement::for_binary(path)?),
            uid: unsafe { libc::geteuid() },
        })
    }

    pub(super) fn current() -> Self {
        let mut policy = Self {
            requirement: None,
            uid: unsafe { libc::geteuid() },
        };
        let Ok(exe) = std::env::current_exe() else {
            return policy;
        };
        let mut application = None;
        for parent in exe.ancestors() {
            if parent
                .extension()
                .is_some_and(|extension| extension == "app")
            {
                application = Some(parent);
            }
        }
        let Some(application) = application else {
            return policy;
        };
        let desktop = application.join("Contents/MacOS/vmux_desktop");
        if let Ok(requirement) = CodeRequirement::for_binary(&desktop) {
            policy.requirement = Some(requirement);
        }
        policy
    }
}

#[derive(Component, Clone)]
#[component(immutable)]
pub(super) struct ClientPeer(pub(super) LocalPeer);

#[derive(Component)]
pub(super) struct DesktopPeer;

#[derive(Component)]
pub(super) struct PendingClient(pub(super) Option<tokio::net::UnixStream>);
