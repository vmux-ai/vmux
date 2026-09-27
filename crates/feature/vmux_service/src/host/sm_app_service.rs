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

pub fn register_main_app() -> Result<(), SmError> {
    let service = unsafe { SMAppService::mainAppService() };
    unsafe { service.registerAndReturnError() }.map_err(|e| SmError::Other(format!("{}", e)))
}

pub fn unregister_main_app() -> Result<(), SmError> {
    let service = unsafe { SMAppService::mainAppService() };
    unsafe { service.unregisterAndReturnError() }.map_err(|e| SmError::Other(format!("{}", e)))
}

pub fn register_agent(plist_name: &str) -> Result<(), SmError> {
    let ns_name = NSString::from_str(plist_name);
    let service = unsafe { SMAppService::agentServiceWithPlistName(&ns_name) };
    unsafe { service.registerAndReturnError() }.map_err(|e| SmError::Other(format!("{e}")))
}

pub fn unregister_agent(plist_name: &str) -> Result<(), SmError> {
    let ns_name = NSString::from_str(plist_name);
    let service = unsafe { SMAppService::agentServiceWithPlistName(&ns_name) };
    unsafe { service.unregisterAndReturnError() }.map_err(|e| SmError::Other(format!("{e}")))
}

#[derive(Debug)]
pub enum Status {
    NotRegistered,
    Enabled,
    RequiresApproval,
    NotFound,
}

pub fn main_app_status() -> Status {
    let service = unsafe { SMAppService::mainAppService() };
    match unsafe { service.status() } {
        SMAppServiceStatus::NotRegistered => Status::NotRegistered,
        SMAppServiceStatus::Enabled => Status::Enabled,
        SMAppServiceStatus::RequiresApproval => Status::RequiresApproval,
        SMAppServiceStatus::NotFound => Status::NotFound,
        _ => Status::NotFound,
    }
}

pub fn agent_status(plist_name: &str) -> Status {
    let ns_name = NSString::from_str(plist_name);
    let service = unsafe { SMAppService::agentServiceWithPlistName(&ns_name) };
    match unsafe { service.status() } {
        SMAppServiceStatus::NotRegistered => Status::NotRegistered,
        SMAppServiceStatus::Enabled => Status::Enabled,
        SMAppServiceStatus::RequiresApproval => Status::RequiresApproval,
        SMAppServiceStatus::NotFound => Status::NotFound,
        _ => Status::NotFound,
    }
}
