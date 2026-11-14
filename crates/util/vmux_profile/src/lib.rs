use std::path::{Path, PathBuf};

#[cfg(target_os = "macos")]
pub use credentials::CredentialStore;
pub use lease::DataLease;
pub use product::Product;
pub use profile_driver::{ProfileMigration, ProfileRecord, ProfileStore};
pub use service::{RemotePaths, ServicePaths};

#[cfg(target_os = "macos")]
pub mod account_credentials;
#[cfg(target_os = "macos")]
mod credentials;
