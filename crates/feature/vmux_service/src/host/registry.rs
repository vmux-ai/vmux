use crate::bundle;
use crate::DaemonBinary;

#[derive(Debug)]
pub enum Backend {
    SmAppService,
    Launchctl,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegistrationStep {
    CleanupLegacy,
    UnregisterMainApp,
    UnregisterEmbeddedAgent,
    RegisterEmbeddedAgent,
    KickstartEmbeddedAgent,
    EnsureLaunchAgent,
}

impl Backend {
    pub fn for_binary(binary: &DaemonBinary) -> Self {
        if bundle::bundle_root_for(binary.path()).is_some() {
            Self::SmAppService
        } else {
            Self::Launchctl
        }
    }

    pub fn registration_steps(&self) -> &'static [RegistrationStep] {
        match self {
            Self::SmAppService => &[
                RegistrationStep::CleanupLegacy,
                RegistrationStep::UnregisterMainApp,
                RegistrationStep::UnregisterEmbeddedAgent,
                RegistrationStep::RegisterEmbeddedAgent,
                RegistrationStep::KickstartEmbeddedAgent,
            ],
            Self::Launchctl => &[RegistrationStep::EnsureLaunchAgent],
        }
    }

    pub fn ensure_running(
        &self,
        profile: &str,
        binary: &DaemonBinary,
    ) -> Result<(), RegistrationError> {
        #[cfg(target_os = "macos")]
        for step in self.registration_steps() {
            match step {
                RegistrationStep::CleanupLegacy => {
                    match crate::cleanup::cleanup_legacy_registrations() {
                        Ok(0) => {}
                        Ok(n) => tracing::info!(removed = n, "removed legacy launchd plists"),
                        Err(e) => {
                            tracing::warn!(error = %e, "legacy plist cleanup failed (continuing)")
                        }
                    }
                }
                RegistrationStep::UnregisterMainApp => {
                    if let Err(e) = crate::sm_app_service::unregister_main_app() {
                        tracing::debug!(error = %e, "unregister main app login item (ignored)");
                    }
                }
                RegistrationStep::UnregisterEmbeddedAgent => {
                    if let Err(e) =
                        crate::sm_app_service::unregister_agent(bundle::EMBEDDED_AGENT_PLIST)
                    {
                        tracing::debug!(error = %e, "unregister embedded agent (ignored)");
                    }
                }
                RegistrationStep::RegisterEmbeddedAgent => {
                    crate::sm_app_service::register_agent(bundle::EMBEDDED_AGENT_PLIST)?;
                }
                RegistrationStep::KickstartEmbeddedAgent => {
                    crate::launchd::kickstart(bundle::EMBEDDED_AGENT_LABEL)?;
                }
                RegistrationStep::EnsureLaunchAgent => {
                    crate::LaunchAgent::for_profile(profile).ensure_running(binary.path())?;
                }
            }
        }
        #[cfg(not(target_os = "macos"))]
        let _ = (profile, binary);
        Ok(())
    }
}

impl DaemonBinary {
    pub fn requires_registration(&self, profile: &str) -> bool {
        profile == "release" && bundle::bundle_root_for(self.path()).is_some()
    }

    pub fn prepare_detached_spawn(&self) {
        #[cfg(not(target_os = "macos"))]
        let _ = self;

        #[cfg(target_os = "macos")]
        if bundle::bundle_root_for(self.path()).is_some()
            && let Err(error) =
                crate::sm_app_service::unregister_agent(bundle::EMBEDDED_AGENT_PLIST)
        {
            tracing::debug!(%error, "unregister embedded agent before detached spawn");
        }
    }
}

#[derive(Debug)]
pub enum RegistrationError {
    Io(std::io::Error),
    #[cfg(target_os = "macos")]
    SmAppService(crate::sm_app_service::SmError),
}

impl From<std::io::Error> for RegistrationError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

#[cfg(target_os = "macos")]
impl From<crate::sm_app_service::SmError> for RegistrationError {
    fn from(error: crate::sm_app_service::SmError) -> Self {
        Self::SmAppService(error)
    }
}
