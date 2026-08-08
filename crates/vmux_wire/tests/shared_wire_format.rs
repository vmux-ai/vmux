//! Guards the one thing rkyv makes dangerous about a remote link.
//!
//! rkyv encodes enum variants *positionally*. On the local socket that is harmless because both
//! sides ship together and the daemon respawns on an identity mismatch. A phone updates on its
//! own schedule, so reordering `SharedMessage` — or inserting a variant anywhere but the end —
//! silently reinterprets messages already in flight rather than failing.
//!
//! The frozen bytes below are the encoding of each variant under the v1 layout, truncated past
//! the point where every field offset has been written. Appending a variant leaves them all
//! intact; reordering the enum shifts the discriminant at bytes 4..8, and reordering fields
//! within a variant moves the offsets after it. Either turns this red.
//!
//! Regenerating these to make the test pass is a wire-format break. The diff is meant to make
//! that obvious enough that nobody does it by accident — bump `ProtocolVersion` instead.

use vmux_wire::protocol::{
    AgentAttachment, ApprovalDecision, ClientMessage, SharedAgentCommand, SharedMessage,
};

/// Bytes compared per variant. Long enough to cover the last field offset any variant writes.
const FROZEN_PREFIX: usize = 48;

/// Every variant in declaration order, with the v1 encoding of a minimal instance.
#[rustfmt::skip]
const FROZEN_V1: [(&str, &str); 8] = [
    ("AttachPageAgent",           "200000000000000073ffffffffffffff0000000000000000000000000000000000000000000000000000000000000000"),
    ("AgentInput",                "200000000100000073ffffffffffffff74ffffffffffffff000000000000000000000000000000000000000000000000"),
    ("AgentCancel",               "200000000200000073ffffffffffffff0000000000000000000000000000000000000000000000000000000000000000"),
    ("AgentApprove",              "200000000300000073ffffffffffffff63ffffffffffffff000000000000000000000000000000000000000000000000"),
    ("AgentInputWithAttachments", "200000000400000073ffffffffffffff74ffffffffffffff000000000000000000000000dcffffff0000000000000000"),
    ("ListSessions",              "200000000500000000000000000000000000000000000000000000000000000000000000000000000000000000000000"),
    ("ListMedia",                 "200000000600000073ffffffffffffff71ffffffffffffff000000000000000000000000000000000000000000000000"),
    ("AgentCommand",              "200000000700000001000000000000000000000000000000000000000000000000000000000000000000000000000000"),
];

fn samples() -> Vec<SharedMessage> {
    vec![
        SharedMessage::AttachPageAgent { sid: "s".into() },
        SharedMessage::AgentInput {
            sid: "s".into(),
            text: "t".into(),
            context: None,
        },
        SharedMessage::AgentCancel { sid: "s".into() },
        SharedMessage::AgentApprove {
            sid: "s".into(),
            call_id: "c".into(),
            decision: ApprovalDecision::Allow,
        },
        SharedMessage::AgentInputWithAttachments {
            sid: "s".into(),
            text: "t".into(),
            context: None,
            attachments: Vec::<AgentAttachment>::new(),
        },
        SharedMessage::ListSessions,
        SharedMessage::ListMedia {
            sid: "s".into(),
            query: "q".into(),
        },
        SharedMessage::AgentCommand(SharedAgentCommand::ListAgents),
    ]
}

/// Matches without a wildcard, so a new variant fails to compile until someone appends it here
/// and to `FROZEN_V1` — which is the moment to notice the wire format is changing.
fn name_of(message: &SharedMessage) -> &'static str {
    match message {
        SharedMessage::AttachPageAgent { .. } => "AttachPageAgent",
        SharedMessage::AgentInput { .. } => "AgentInput",
        SharedMessage::AgentCancel { .. } => "AgentCancel",
        SharedMessage::AgentApprove { .. } => "AgentApprove",
        SharedMessage::AgentInputWithAttachments { .. } => "AgentInputWithAttachments",
        SharedMessage::ListSessions => "ListSessions",
        SharedMessage::ListMedia { .. } => "ListMedia",
        SharedMessage::AgentCommand(_) => "AgentCommand",
    }
}

fn encode(message: SharedMessage) -> Vec<u8> {
    rkyv::to_bytes::<rkyv::rancor::Error>(&ClientMessage::Shared(message))
        .expect("encode")
        .to_vec()
}

#[test]
fn every_shared_variant_still_encodes_to_its_v1_bytes() {
    let mut encoded = Vec::new();
    for message in samples() {
        let name = name_of(&message);
        let bytes = encode(message);
        let mut hex = String::new();
        for byte in bytes.iter().take(FROZEN_PREFIX) {
            hex.push_str(&format!("{byte:02x}"));
        }
        encoded.push((name, hex));
    }

    let mut frozen = Vec::new();
    for (name, hex) in FROZEN_V1 {
        frozen.push((name, hex.to_string()));
    }

    assert_eq!(
        encoded, frozen,
        "the v1 wire format changed — bump ProtocolVersion rather than refreezing these bytes"
    );
}

/// The frozen bytes are a prefix, so this covers what they cannot: a full frame still decodes to
/// the same variant carrying the same payload.
#[test]
fn a_v1_frame_round_trips_with_its_payload_intact() {
    let bytes = encode(SharedMessage::AgentApprove {
        sid: "s".into(),
        call_id: "c".into(),
        decision: ApprovalDecision::Allow,
    });

    let decoded = rkyv::from_bytes::<ClientMessage, rkyv::rancor::Error>(&bytes).expect("decode");

    let ClientMessage::Shared(SharedMessage::AgentApprove {
        sid,
        call_id,
        decision,
    }) = decoded
    else {
        panic!("decoded to the wrong variant");
    };
    assert_eq!(
        (sid.as_str(), call_id.as_str(), decision),
        ("s", "c", ApprovalDecision::Allow)
    );
}
