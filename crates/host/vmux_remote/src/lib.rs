//! The wire contract between a client, the relay and a desktop.
//!
//! Deliberately free of domain types. The relay links this crate to route bytes, and keeping
//! `Message`, `RoomEvent` and the session views out of it means a relay *cannot* decode a payload
//! even by accident — the property the transport design depends on. Those live in
//! [`vmux_wire::room`].

pub mod device;
/// Length-prefixed frames, for a stream carrying many messages.
pub mod framing;
pub mod quic;

pub use device::DeviceId;
pub use quic::{Accepted, ClientSetup, CloseCode, MessageType, PeerRole, Protocol, RelaySetup};
