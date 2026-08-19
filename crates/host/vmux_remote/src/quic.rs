//! What the messages on a QUIC link are, and which number names each one.
//!
//! How they are framed is [`crate::framing`]. What they mean is here.
//!
//! The setup messages are JSON rather than rkyv because they are parsed *before* the two peers
//! have agreed on anything: rkyv encodes enum variants positionally, so a peer several releases
//! behind would misread a reordered variant rather than notice the mismatch, where serde skips a
//! field it does not know. Session traffic after the setup is rkyv, which is faster and by then
//! safe.

/// Endpoint construction and certificate pinning. Absent on wasm, which has no UDP socket.
pub mod endpoint;

/// The relay tunnel a desktop's inner endpoint runs over. Absent on wasm for the same reason.
pub mod tunnel;

use serde::{Deserialize, Serialize};

use crate::DeviceId;

/// ALPN identifier offered during the TLS handshake, and the gate on what the conversation
/// *means*.
///
/// Bumping it rejects an incompatible peer during the QUIC handshake, before a single application
/// byte is exchanged. Nothing after it negotiates: rkyv encodes enum variants positionally, so a
/// peer several releases behind must be refused outright rather than talked down to some common
/// version. No application message carries a capability list or a version of its own.
///
/// It is not the only gate, and the doc here used to claim it was. [`FRAME_VERSION`] answers the
/// narrower question of how to *read* a frame, which is a different question from what the frame
/// then means — see its own note.
pub const ALPN: &[u8] = b"vmux/2";

/// ALPN a liveness probe negotiates instead of [`ALPN`].
///
/// A probe is closed as soon as the handshake completes, so reaching that point already proves
/// what a deploy needs to know: the UDP port is open through the firewall, the certificate is
/// present and valid, and the accept loop is running. None of that is visible over TCP, and a
/// probe that spoke [`ALPN`] would have to invent a device id and leave a registration behind.
pub const PROBE_ALPN: &[u8] = b"vmux-probe/1";

/// Layout version, sent once at the head of every stream.
///
/// Bumping it changes how to *read* a frame, where bumping [`ALPN`] says the conversation means
/// something different. Two gates rather than one because they answer different questions and a
/// layout can change without the vocabulary doing so — which is exactly what version 2 was: the
/// magic and the untyped envelope went, and a message type arrived.
///
/// Per stream rather than per connection. One byte is cheap, and the alternative is a reader that
/// cannot make sense of a stream without first consulting connection state it may not hold.
pub const FRAME_VERSION: u8 = 2;

/// Which conversation a message belongs to, read off the high byte of a [`MessageType`].
///
/// Ranges rather than one flat list, because this crate is public and the relay's repo consumes it
/// through a pin that only advances to `main`. A flat registry would make every new account-service
/// message need a change here first, just to be allocated a number.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Protocol {
    /// Framing itself, shared by every leg.
    Transport,
    /// A client talking to the desktop that holds its sessions.
    Session,
    /// Either peer talking to the relay.
    Relay,
    /// A device talking to the account service. Codes are defined in that service, not here.
    Account,
    /// A range this build has never heard of.
    Unknown(u8),
}

/// What a frame carries, as a number on the wire.
///
/// The discriminator that makes a frame self-describing. Without one a reader parses whatever it
/// happens to expect, and serde's tolerance for unknown fields means a message from another leg
/// decodes cleanly rather than being refused — a relay setup satisfied a session setup exactly
/// that way, because a shared ALPN meant nothing else separated them either.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct MessageType(pub u16);

impl MessageType {
    /// A client naming itself to the desktop that holds its sessions.
    pub const CLIENT_SETUP: Self = Self(0x0100);
    /// The desktop admitting it.
    pub const SESSION_ACCEPTED: Self = Self(0x0101);
    /// One request on a control stream.
    pub const CONTROL_REQUEST: Self = Self(0x0102);
    /// Its answer.
    pub const CONTROL_RESPONSE: Self = Self(0x0103);
    /// A client asking to be sent a session's events.
    pub const SESSION_EVENTS: Self = Self(0x0104);
    /// One of them. Many ride a single stream, which is why frames carry a length.
    pub const SESSION_EVENT: Self = Self(0x0105);

    /// A peer naming the pair it belongs to, to the relay.
    pub const RELAY_SETUP: Self = Self(0x0200);
    /// The relay admitting it.
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

/// First frame a client sends to a desktop: who it is, and the token that says it may.
///
/// The token rides here rather than in a header because QUIC has none. It used to be a second
/// struct wrapping this one with `#[serde(flatten)]`, declared twice in two crates with nothing
/// keeping them in step.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ClientSetup {
    pub device_id: DeviceId,
    pub token: String,
}

/// An admission, carrying nothing.
///
/// Both legs answer with this, under their own [`MessageType`] — [`MessageType::SESSION_ACCEPTED`]
/// and [`MessageType::RELAY_ACCEPTED`] — so the two stay distinguishable on the wire even though
/// the payload is identical. Reading a well-formed one is the accept signal; a refusal arrives as
/// a close carrying a [`CloseCode`].
///
/// Empty on purpose: a capability list and a version were both carried here for a while and
/// neither was ever read. Serde ignores unknown fields, so a field can be added the day something
/// needs one.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct Accepted {}

/// Application close codes, so a disconnect says why.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum CloseCode {
    Normal = 0,
    /// Bearer token missing or wrong.
    Unauthorized = 2,
    /// Remote was switched off on the desktop while the connection was open.
    RemoteDisabled = 3,
    /// Frame was malformed, oversized, or arrived out of order.
    ProtocolError = 4,
    /// A phone named a desktop the relay is not holding.
    ///
    /// Its own code rather than a normal close, so a phone can tell "not registered yet, retry"
    /// from "the desktop went away deliberately". A desktop reconnects on a backoff, so this is
    /// the ordinary answer during a redeploy.
    NoSuchDesktop = 5,
    /// The relay, or the desktop, is already carrying as many peers as it will.
    ///
    /// Distinct from [`CloseCode::NoSuchDesktop`] because retrying helps with one and not the
    /// other, and both used to arrive as a clean shutdown.
    AtCapacity = 6,
}

impl CloseCode {
    pub fn as_u32(self) -> u32 {
        self as u32
    }

    /// The code a peer closed with, when this build recognises it.
    ///
    /// `None` covers anything else, including codes an older build still sends — a client turns
    /// that into a generic message rather than guessing at what was meant.
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

/// Which end of a relayed pair a peer is.
///
/// The HTTP relay told these apart by URL path (`/desktop/…` against `/r/…`). A QUIC connection
/// has no path, so the role is declared once at connect time.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PeerRole {
    /// Holds the sessions. Registers once, then answers whatever the relay forwards to it.
    Desktop,
    /// Wants to reach a desktop. Dials the same port the desktop did and names the one it wants.
    Client,
}

/// First frame a peer sends to the relay.
///
/// Deliberately the only thing the relay parses. Everything after it is opaque bytes copied
/// between two peers — the relay routes on `device_id` and never learns what it moved.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RelaySetup {
    /// The desktop being named, whichever end is speaking: its own id from a
    /// [`PeerRole::Desktop`], the id of the desktop it wants from a [`PeerRole::Client`].
    ///
    /// Never the phone's own id. This identifies a pair, not a peer.
    pub device_id: DeviceId,
    pub role: PeerRole,
    /// Proves both ends belong to the same pairing. The relay compares, it does not mint.
    pub token: String,
}
