//! The wire contract between a client, the relay and a desktop.
//!
//! Deliberately free of domain types. The relay links this crate to route bytes, and keeping
//! `Message`, `RoomEvent` and the session views out of it means a relay *cannot* decode a payload
//! even by accident — the property the transport design depends on. Those live in
//! [`vmux_wire::room`].

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub mod quic;
pub use quic::{
    Capability, ClientHello, CloseCode, Envelope, PeerRole, ProtocolVersion, RelayHello, StreamKind,
};

/// Identifies one paired desktop to the relay. Opaque: the relay routes on it and reads nothing
/// else about the peer.
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

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct DesktopCommand {
    pub id: String,
    pub kind: DesktopCommandKind,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DesktopCommandKind {
    ListSessions,
    /// `/r/{device}/api/agents` — the installed-agent list.
    ListAgents,
    /// `/r/{device}/api/team` — the active space's roster.
    ListTeam,
    CreateChat {
        body: Value,
    },
    SendPrompt {
        sid: String,
        body: Value,
    },
    Cancel {
        sid: String,
    },
    Approve {
        sid: String,
        body: Value,
    },
    ListMedia {
        sid: String,
        query: String,
    },
    SubscribeSession {
        sid: String,
        stream_id: String,
    },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct DesktopResponse {
    pub status: u16,
    #[serde(default)]
    pub body: Value,
}
