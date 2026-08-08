//! How the relay tells one paired desktop from another.

use serde::{Deserialize, Serialize};

/// Identifies one paired desktop to the relay.
///
/// Opaque on purpose: the relay routes on it and reads nothing else about the peer. It is minted
/// by the desktop, not issued by the relay, so it carries no authority — the token in the hello
/// does that.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct DeviceId(pub String);

impl DeviceId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for DeviceId {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<&str> for DeviceId {
    fn from(value: &str) -> Self {
        Self(value.to_string())
    }
}
