pub use account::{AccountPlugin, Set};
pub use catalog::{
    DirectoryObservation, Error, Loaded, Location, ReadCompleted, ReadRequest, RetryRequest, Root,
    Selection,
};
pub use connection::{
    Authorization, CancelConnectRequest, CloudAvailable, ConnectRequest, Connection,
    ConnectionError, DisconnectRequest, Disconnecting, Phase, Server,
};
pub use mutations::{
    Commit, CreateRequest, LinkRequest, MutationCompleted, RenameRequest, SyncRequest,
    WriteCompleted, WriteFailure, WriteRequest,
};
pub use profile::{Account, Binding, Owner, Profile, SyncEnabled};
pub use profile::{Failure, FailureKind};

#[cfg(feature = "cloud")]
pub use availability::AccountCloudFlag;
#[cfg(feature = "cloud")]
pub use relay::{RelayAssertion, RelayAssertionUnavailable};

#[cfg(feature = "profile-sync")]
pub use sync::{CheckpointChange, ExpectedScope, Origin as SyncOrigin, Pending as ProfileOutbox};
#[cfg(feature = "profile-sync")]
pub use sync_runtime::{
    Conflict, ImportRequest, KeepLocalRequest, RemoteDeleted, RemoteProfile, RemoteRevision,
    SyncProjection, SyncedRevision, UseRemoteRequest,
};

#[cfg(feature = "cloud")]
pub struct Feature;

#[cfg(feature = "cloud")]
impl vmux_feature::manifest::FeatureManifestSource for Feature {
    const SOURCE: &'static str = include_str!("../feature.ron");
}

mod account;
#[cfg(feature = "cloud")]
mod availability;
mod catalog;
mod connection;
mod mutations;
mod profile;
mod wire;

#[cfg(feature = "local")]
pub mod local;
#[cfg(feature = "local")]
mod local_io;

#[cfg(feature = "cloud")]
pub mod cloud;
#[cfg(feature = "cloud")]
pub mod control;
#[cfg(feature = "cloud")]
pub mod relay;

#[cfg(feature = "cloud")]
mod sync;
#[cfg(feature = "profile-sync")]
mod sync_store;

#[cfg(feature = "profile-sync")]
mod sync_runtime;
