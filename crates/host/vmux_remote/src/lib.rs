//! The wire contract between a client, the relay and a desktop.
//!
//! Deliberately free of domain types. The relay links this crate to route bytes, and keeping
//! `Message`, `RoomEvent` and the session views out of it means a relay *cannot* decode a payload
//! even by accident — the property the transport design depends on. Those live in
//! [`vmux_wire::room`].

pub mod device;
/// Length-prefixed frames, for a stream carrying many messages. Absent on wasm, which has no
/// socket to carry them over.
#[cfg(not(target_arch = "wasm32"))]
pub mod framing;
pub mod quic;

pub use device::DeviceId;
pub use quic::{ClientHello, CloseCode, Envelope, PeerRole, RelayHello, StreamKind};
