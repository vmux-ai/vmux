#![cfg(target_os = "macos")]

use std::fmt;

use objc2_foundation::NSString;
use objc2_service_management::{SMAppService, SMAppServiceStatus};

#[derive(Debug)]
pub enum SmError {
    NotEnabled,
    NotRegistered,
    RequiresApproval,
    Other(String),
}

impl fmt::Display for SmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotEnabled => write!(f, "SMAppService not enabled"),
            Self::NotRegistered => write!(f, "SMAppService not registered"),
            Self::RequiresApproval => write!(f, "SMAppService requires user approval"),
            Self::Other(s) => write!(f, "SMAppService: {s}"),
        }
    }
}

impl std::error::Error for SmError {}

pub struct MainAppService;

impl MainAppService {
    pub fn register() -> Result<(), SmError> {
        let service = unsafe { SMAppService::mainAppService() };
        unsafe { service.registerAndReturnError() }
            .map_err(|error| SmError::Other(error.to_string()))
    }

    pub fn unregister() -> Result<(), SmError> {
        let service = unsafe { SMAppService::mainAppService() };
        unsafe { service.unregisterAndReturnError() }
            .map_err(|error| SmError::Other(error.to_string()))
    }

    pub fn status() -> Status {
        let service = unsafe { SMAppService::mainAppService() };
        Status::of(&service)
    }
}

pub struct AgentService(String);

impl AgentService {
    pub fn new(plist_name: impl Into<String>) -> Self {
        Self(plist_name.into())
    }

    pub fn register(&self) -> Result<(), SmError> {
        let name = NSString::from_str(&self.0);
        let service = unsafe { SMAppService::agentServiceWithPlistName(&name) };
        unsafe { service.registerAndReturnError() }
            .map_err(|error| SmError::Other(error.to_string()))
    }

    pub fn unregister(&self) -> Result<(), SmError> {
        let name = NSString::from_str(&self.0);
        let service = unsafe { SMAppService::agentServiceWithPlistName(&name) };
        unsafe { service.unregisterAndReturnError() }
            .map_err(|error| SmError::Other(error.to_string()))
    }

    pub fn status(&self) -> Status {
        let name = NSString::from_str(&self.0);
        let service = unsafe { SMAppService::agentServiceWithPlistName(&name) };
        Status::of(&service)
    }
}

#[derive(Debug)]
pub enum Status {
    NotRegistered,
    Enabled,
    RequiresApproval,
    NotFound,
}

impl Status {
    fn of(service: &SMAppService) -> Self {
        match unsafe { service.status() } {
            SMAppServiceStatus::NotRegistered => Self::NotRegistered,
            SMAppServiceStatus::Enabled => Self::Enabled,
            SMAppServiceStatus::RequiresApproval => Self::RequiresApproval,
            SMAppServiceStatus::NotFound => Self::NotFound,
            _ => Self::NotFound,
        }
    }
}
