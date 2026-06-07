use std::path::{Path, PathBuf};

use crate::bundle;

#[derive(Debug)]
pub enum Backend {
    SmAppService { bundle_root: PathBuf },
    Launchctl,
}

pub fn choose_backend(exe: &Path) -> Backend {
    if let Some(root) = bundle::bundle_root_for(exe) {
        Backend::SmAppService { bundle_root: root }
    } else {
        Backend::Launchctl
    }
}

#[derive(Debug)]
pub enum RegistrationError {
    Io(std::io::Error),
    #[cfg(target_os = "macos")]
    SmAppService(crate::sm_app_service::SmError),
}

impl From<std::io::Error> for RegistrationError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

#[cfg(target_os = "macos")]
impl From<crate::sm_app_service::SmError> for RegistrationError {
    fn from(e: crate::sm_app_service::SmError) -> Self {
        Self::SmAppService(e)
    }
}

pub fn ensure_running(profile: &str, exe: &Path) -> Result<(), RegistrationError> {
    match choose_backend(exe) {
        Backend::SmAppService { .. } => {
            #[cfg(target_os = "macos")]
            {
                match crate::cleanup::cleanup_legacy_registrations() {
                    Ok(0) => {}
                    Ok(n) => tracing::info!(removed = n, "removed legacy launchd plists"),
                    Err(e) => {
                        tracing::warn!(error = %e, "legacy plist cleanup failed (continuing)")
                    }
                }
                if let Err(e) = crate::sm_app_service::unregister_main_app() {
                    tracing::debug!(error = %e, "unregister main app login item (ignored)");
                }
                if let Err(e) =
                    crate::sm_app_service::unregister_agent(bundle::EMBEDDED_AGENT_PLIST)
                {
                    tracing::debug!(error = %e, "unregister embedded agent (ignored)");
                }
                crate::sm_app_service::register_agent(bundle::EMBEDDED_AGENT_PLIST)?;
                crate::launchd::kickstart(bundle::EMBEDDED_AGENT_LABEL)?;
                Ok(())
            }
            #[cfg(not(target_os = "macos"))]
            {
                Ok(())
            }
        }
        Backend::Launchctl => {
            #[cfg(target_os = "macos")]
            {
                crate::launchd::ensure_running(profile, exe)?;
                Ok(())
            }
            #[cfg(not(target_os = "macos"))]
            {
                let _ = (profile, exe);
                Ok(())
            }
        }
    }
}
