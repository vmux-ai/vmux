use crate::DaemonBinary;
use crate::bundle::AppBundle;
#[cfg(target_os = "macos")]
use crate::bundle::EMBEDDED_AGENT_PLIST;

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
        if AppBundle::try_from(binary.path()).is_ok() {
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
}

impl DaemonBinary {
    pub fn requires_registration(&self, profile: &str) -> bool {
        profile == "release" && AppBundle::try_from(self.path()).is_ok()
    }

    pub fn prepare_detached_spawn(&self) {
        #[cfg(not(target_os = "macos"))]
        let _ = self;

        #[cfg(target_os = "macos")]
        if AppBundle::try_from(self.path()).is_ok()
            && let Err(error) =
                crate::sm_app_service::AgentService::new(EMBEDDED_AGENT_PLIST).unregister()
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
