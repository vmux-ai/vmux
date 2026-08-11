//! Connection setup for the QUIC link.
//!
//! Everything here is parsed *before* the two peers have agreed on a protocol version, which is
//! why none of it is rkyv: rkyv encodes enum variants positionally, so a peer several releases
//! behind would misread a reordered variant rather than notice the mismatch. The hello is a fixed
//! byte layout that any version can read far enough to decide whether to keep talking.
//!
//! Application frames after the hello are rkyv, length-prefixed by the same codec the local unix
//! socket uses.

/// Endpoint construction and certificate pinning. Absent on wasm, which has no UDP socket.
#[cfg(not(web))]
pub mod endpoint;

/// The relay tunnel a desktop's inner endpoint runs over. Absent on wasm for the same reason.
#[cfg(not(web))]
pub mod tunnel;

use serde::{Deserialize, Serialize};

use crate::DeviceId;

/// ALPN identifier offered during the TLS handshake, and the only compatibility gate there is.
///
/// Bumping it rejects an incompatible peer during the QUIC handshake, before a single application
/// byte is exchanged. There is deliberately no second version field in the hellos: one gate that
/// always runs beats two that can disagree, and rkyv's positional variants mean a mismatched peer
/// has to be refused outright rather than negotiated with.
pub const ALPN: &[u8] = b"vmux/1";

/// ALPN a liveness probe negotiates instead of [`ALPN`].
///
/// A probe is closed as soon as the handshake completes, so reaching that point already proves
/// what a deploy needs to know: the UDP port is open through the firewall, the certificate is
/// present and valid, and the accept loop is running. None of that is visible over TCP, and a
/// probe that spoke [`ALPN`] would have to invent a device id and leave a registration behind.
pub const PROBE_ALPN: &[u8] = b"vmux-probe/1";

/// Leading bytes of every connection, so a mis-dialled port fails loudly rather than as a decode
/// error hundreds of bytes later.
pub const HELLO_MAGIC: [u8; 5] = *b"VMUXQ";

/// Layout version of the hello envelope. Bumping it changes how to *read* a hello, where bumping
/// [`ALPN`] says the conversation after it means something different.
pub const HELLO_VERSION: u8 = 1;

/// First application frame a client sends.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ClientHello {
    pub device_id: DeviceId,
}

/// The desktop's answer to a [`ClientHello`].
///
/// The accept signal, and nothing else: a client that reads a well-formed answer was admitted,
/// where a refusal arrives as a connection close carrying a [`CloseCode`]. Empty on purpose —
/// a capability list and a version were both carried here for a while and neither was ever read.
/// Serde ignores unknown fields, so a field can be added the day something needs one.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct ServerHello {}

/// What a QUIC stream carries, written as its first byte so the peer can dispatch without
/// decoding the payload — and so the relay can route without decoding it at all.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum StreamKind {
    /// Bidirectional request/response. One frame each way, then closed.
    Control = 0,
    /// Server to client, one per subscribed session.
    SessionEvents = 1,
}

impl StreamKind {
    pub fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            0 => Some(Self::Control),
            1 => Some(Self::SessionEvents),
            _ => None,
        }
    }

    pub fn as_byte(self) -> u8 {
        self as u8
    }
}

/// One routed unit as the relay sees it: who it is for, which stream it belongs to, and bytes it
/// does not interpret.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Envelope {
    pub device_id: DeviceId,
    pub stream_kind: StreamKind,
    pub payload: Vec<u8>,
}

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
            _ => None,
        }
    }
}

/// Encode a hello frame: magic, layout version, then a length-prefixed JSON body.
///
/// JSON rather than rkyv precisely because this frame outlives version agreement — an unknown
/// field is skipped instead of shifting every byte after it.
pub fn encode_hello<T: Serialize>(value: &T) -> Result<Vec<u8>, serde_json::Error> {
    let body = serde_json::to_vec(value)?;
    let mut out = Vec::with_capacity(HELLO_MAGIC.len() + 1 + 4 + body.len());
    out.extend_from_slice(&HELLO_MAGIC);
    out.push(HELLO_VERSION);
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(&body);
    Ok(out)
}

/// Why a hello frame could not be read.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HelloError {
    /// Not a vmux endpoint, or not speaking this transport.
    BadMagic,
    /// Hello envelope itself is a layout this build cannot read.
    UnsupportedHelloVersion(u8),
    /// Fewer bytes than the declared length.
    Truncated,
    /// Envelope read, body did not parse.
    Malformed,
}

/// Decode a hello frame, returning the value and how many bytes it consumed.
pub fn decode_hello<T: for<'de> Deserialize<'de>>(bytes: &[u8]) -> Result<(T, usize), HelloError> {
    let header = HELLO_MAGIC.len() + 1 + 4;
    if bytes.len() < header {
        return Err(HelloError::Truncated);
    }
    if bytes[..HELLO_MAGIC.len()] != HELLO_MAGIC {
        return Err(HelloError::BadMagic);
    }
    let hello_version = bytes[HELLO_MAGIC.len()];
    if hello_version != HELLO_VERSION {
        return Err(HelloError::UnsupportedHelloVersion(hello_version));
    }
    let mut length = [0u8; 4];
    length.copy_from_slice(&bytes[HELLO_MAGIC.len() + 1..header]);
    let length = u32::from_le_bytes(length) as usize;
    let end = header + length;
    if bytes.len() < end {
        return Err(HelloError::Truncated);
    }
    let value = serde_json::from_slice(&bytes[header..end]).map_err(|_| HelloError::Malformed)?;
    Ok((value, end))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hello_roundtrips_and_reports_its_length() {
        let hello = ClientHello {
            device_id: DeviceId::new("device-1"),
        };
        let mut bytes = encode_hello(&hello).unwrap();
        bytes.extend_from_slice(b"frames follow");

        let (decoded, consumed) = decode_hello::<ClientHello>(&bytes).unwrap();

        assert_eq!(decoded, hello);
        assert_eq!(&bytes[consumed..], b"frames follow");
    }

    /// The whole reason the hello is JSON: a client several releases ahead can send a field
    /// this build has never heard of and still be understood. That tolerance is what lets the
    /// hello grow later without a version bump, so it is worth a test of its own.
    #[test]
    fn an_unknown_field_degrades_instead_of_failing() {
        let wire = br#"{"device_id":"d","teleportation":true}"#;
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&HELLO_MAGIC);
        bytes.push(HELLO_VERSION);
        bytes.extend_from_slice(&(wire.len() as u32).to_le_bytes());
        bytes.extend_from_slice(wire);

        let (decoded, _) = decode_hello::<ClientHello>(&bytes).unwrap();

        assert_eq!(decoded.device_id, DeviceId::new("d"));
    }

    #[test]
    fn a_non_vmux_endpoint_is_rejected_before_parsing() {
        assert_eq!(
            decode_hello::<ClientHello>(b"HTTP/1.1 200 OK\r\n\r\n").unwrap_err(),
            HelloError::BadMagic
        );
    }

    #[test]
    fn a_partial_frame_is_not_mistaken_for_a_complete_one() {
        let hello = ClientHello {
            device_id: DeviceId::new("d"),
        };
        let bytes = encode_hello(&hello).unwrap();

        assert_eq!(
            decode_hello::<ClientHello>(&bytes[..bytes.len() - 1]).unwrap_err(),
            HelloError::Truncated
        );
    }

    #[test]
    fn stream_kind_byte_roundtrips_and_rejects_unknown() {
        for kind in [StreamKind::Control, StreamKind::SessionEvents] {
            assert_eq!(StreamKind::from_byte(kind.as_byte()), Some(kind));
        }
        assert_eq!(StreamKind::from_byte(200), None);
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
    /// Wants to reach a desktop. Dials the port the relay allocated that desktop.
    Client,
}

/// First frame on a connection to the relay.
///
/// Deliberately the only thing the relay parses. Everything after it is opaque bytes copied
/// between two peers — the relay routes on `device_id` and never learns what it moved.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RelayHello {
    pub device_id: DeviceId,
    pub role: PeerRole,
    /// Proves both ends belong to the same pairing. The relay compares, it does not mint.
    pub token: String,
}

/// The relay's answer to a desktop's hello.
///
/// A pairing link has to name the UDP port the phone should dial, and only the relay knows which
/// one this desktop was given — so the desktop cannot build a link until this arrives.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RelayAllocation {
    pub port: u16,
}
