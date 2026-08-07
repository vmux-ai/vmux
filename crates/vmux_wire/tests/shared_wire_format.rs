//! Guards the one thing rkyv makes dangerous about a remote link.
//!
//! rkyv encodes enum variants *positionally*. On the local socket that is harmless because both
//! sides ship together and the daemon respawns on an identity mismatch. A phone updates on its
//! own schedule, so reordering `SharedMessage` — or inserting a variant anywhere but the end —
//! silently reinterprets messages already in flight rather than failing.
//!
//! These bytes were produced by the v1 layout. Appending a variant leaves them decodable;
//! reordering or re-typing one turns this red.

use vmux_wire::protocol::{ApprovalDecision, ClientMessage, SharedMessage};

/// A missing fixture is a compile error rather than a silently-passing test.
const SHARED_MESSAGE_V1: &[u8] = include_bytes!("fixtures/shared_message_v1.bin");

fn decode_frames(mut bytes: &[u8]) -> Vec<ClientMessage> {
    let mut out = Vec::new();
    while !bytes.is_empty() {
        let (length, rest) = bytes.split_at(4);
        let length = u32::from_le_bytes(length.try_into().expect("length prefix")) as usize;
        let (body, rest) = rest.split_at(length);
        // Copied into a `Vec` before decoding, because rkyv needs 8-byte alignment and a slice of
        // `include_bytes!` at a 4-byte offset has none. The production reader lands each frame in
        // a fresh `Vec` for the same reason.
        let body = body.to_vec();
        out.push(
            rkyv::from_bytes::<ClientMessage, rkyv::rancor::Error>(&body)
                .expect("v1 frame no longer decodes — a Shared variant was reordered or re-typed"),
        );
        bytes = rest;
    }
    out
}

#[test]
fn v1_shared_message_frames_still_decode_to_the_same_variants() {
    let decoded = decode_frames(SHARED_MESSAGE_V1);

    let shapes: Vec<&str> = decoded
        .iter()
        .map(|message| match message {
            ClientMessage::Shared(SharedMessage::AttachPageAgent { .. }) => "AttachPageAgent",
            ClientMessage::Shared(SharedMessage::AgentInput { .. }) => "AgentInput",
            ClientMessage::Shared(SharedMessage::AgentCancel { .. }) => "AgentCancel",
            ClientMessage::Shared(SharedMessage::AgentApprove { .. }) => "AgentApprove",
            ClientMessage::Shared(SharedMessage::AgentInputWithAttachments { .. }) => {
                "AgentInputWithAttachments"
            }
            _ => "not-shared",
        })
        .collect();

    assert_eq!(
        shapes,
        [
            "AttachPageAgent",
            "AgentInput",
            "AgentCancel",
            "AgentApprove",
            "AgentInputWithAttachments",
        ]
    );
}

/// Payloads must survive the round trip too — a field reorder within a variant would decode to
/// the right variant carrying the wrong values.
#[test]
fn v1_payloads_survive_the_round_trip() {
    let decoded = decode_frames(SHARED_MESSAGE_V1);

    let approve = decoded
        .iter()
        .find_map(|message| match message {
            ClientMessage::Shared(SharedMessage::AgentApprove {
                sid,
                call_id,
                decision,
            }) => Some((sid.clone(), call_id.clone(), *decision)),
            _ => None,
        })
        .expect("AgentApprove in fixture");

    assert_eq!(
        approve,
        ("s".to_string(), "c".to_string(), ApprovalDecision::Allow)
    );
}
