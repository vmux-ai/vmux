pub mod endpoint;

pub mod tunnel;

use serde::{Deserialize, Serialize};

use crate::DeviceId;

pub const ALPN: &[u8] = b"vmux/2";

pub const PROBE_ALPN: &[u8] = b"vmux-probe/1";

pub const FRAME_VERSION: u8 = 2;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Protocol {
    Transport,
    Session,
    Relay,
    Account,
    Unknown(u8),
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct MessageType(pub u16);

impl MessageType {
    pub const CLIENT_SETUP: Self = Self(0x0100);
    pub const SESSION_ACCEPTED: Self = Self(0x0101);
    pub const CONTROL_REQUEST: Self = Self(0x0102);
    pub const CONTROL_RESPONSE: Self = Self(0x0103);
    pub const SESSION_EVENTS: Self = Self(0x0104);
    pub const SESSION_EVENT: Self = Self(0x0105);

    pub const RELAY_SETUP: Self = Self(0x0200);
    pub const RELAY_ACCEPTED: Self = Self(0x0201);

    pub const fn protocol(self) -> Protocol {
        match (self.0 >> 8) as u8 {
            0x00 => Protocol::Transport,
            0x01 => Protocol::Session,
            0x02 => Protocol::Relay,
            0x03 => Protocol::Account,
            other => Protocol::Unknown(other),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ClientSetup {
    pub device_id: DeviceId,
    pub token: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct Accepted {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum CloseCode {
    Normal = 0,
    Unauthorized = 2,
    RemoteDisabled = 3,
    ProtocolError = 4,
    NoSuchDesktop = 5,
    AtCapacity = 6,
}

impl CloseCode {
    pub fn as_u32(self) -> u32 {
        self as u32
    }

    pub fn from_u32(code: u32) -> Option<Self> {
        match code {
            0 => Some(Self::Normal),
            2 => Some(Self::Unauthorized),
            3 => Some(Self::RemoteDisabled),
            4 => Some(Self::ProtocolError),
            5 => Some(Self::NoSuchDesktop),
            6 => Some(Self::AtCapacity),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PeerRole {
    Desktop,
    Client,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RelaySetup {
    pub device_id: DeviceId,
    pub role: PeerRole,
    pub token: String,
}
