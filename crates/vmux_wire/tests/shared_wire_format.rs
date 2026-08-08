//! Guards the one thing rkyv makes dangerous about a remote link.
//!
//! rkyv encodes enum variants *positionally*. On the local socket that is harmless because both
//! sides ship together and the daemon respawns on an identity mismatch. A phone updates on its
//! own schedule, so reordering `SharedMessage` or `AgentAction` — or inserting a variant anywhere
//! but the end — silently reinterprets messages already in flight rather than failing.
//!
//! The frozen bytes below are the encoding of each variant under the v2 layout, truncated past
//! the point where every field offset has been written. Appending a variant leaves them all
//! intact; reordering an enum shifts a discriminant, and reordering fields within a variant moves
//! the offsets after it. Either turns this red.
//!
//! Regenerating these to make the test pass is a wire-format break. The diff is meant to make
//! that obvious enough that nobody does it by accident — bump `ProtocolVersion` instead. v2 is
//! itself such a break: v1 spelled these as seven flat variants carrying their own `sid`.

use vmux_wire::protocol::{
    AgentAction, AgentAttachment, ApprovalDecision, ClientMessage, SharedAgentCommand,
    SharedMessage,
};

/// Bytes compared per variant. Long enough to cover the last field offset any variant writes.
const FROZEN_PREFIX: usize = 48;

/// Every variant in declaration order, with the v2 encoding of a minimal instance.
#[rustfmt::skip]
const FROZEN_V2: [(&str, &str); 7] = [
    ("Agent/Attach",    "200000000000000073ffffffffffffff0000000000000000000000000000000000000000000000000000000000000000"),
    ("Agent/Input",     "200000000000000073ffffffffffffff0100000074ffffffffffffff000000000000000000000000d8ffffff00000000"),
    ("Agent/Cancel",    "200000000000000073ffffffffffffff0200000000000000000000000000000000000000000000000000000000000000"),
    ("Agent/Approve",   "200000000000000073ffffffffffffff0300000063ffffffffffffff0000000000000000000000000000000000000000"),
    ("Agent/ListMedia", "200000000000000073ffffffffffffff0400000071ffffffffffffff0000000000000000000000000000000000000000"),
    ("ListSessions",    "200000000100000000000000000000000000000000000000000000000000000000000000000000000000000000000000"),
    ("AgentCommand",    "200000000200000001000000000000000000000000000000000000000000000000000000000000000000000000000000"),
];

fn samples() -> Vec<SharedMessage> {
    vec![
        SharedMessage::agent("s", AgentAction::Attach),
        SharedMessage::agent(
            "s",
            AgentAction::Input {
                text: "t".into(),
                context: None,
                attachments: Vec::<AgentAttachment>::new(),
            },
        ),
        SharedMessage::agent("s", AgentAction::Cancel),
        SharedMessage::agent(
            "s",
            AgentAction::Approve {
                call_id: "c".into(),
                decision: ApprovalDecision::Allow,
            },
        ),
        SharedMessage::agent("s", AgentAction::ListMedia { query: "q".into() }),
        SharedMessage::ListSessions,
        SharedMessage::AgentCommand(SharedAgentCommand::ListAgents),
    ]
}

/// Matches without a wildcard, so a new variant fails to compile until someone appends it here
/// and to `FROZEN_V2` — which is the moment to notice the wire format is changing.
fn name_of(message: &SharedMessage) -> &'static str {
    match message {
        SharedMessage::Agent { action, .. } => match action {
            AgentAction::Attach => "Agent/Attach",
            AgentAction::Input { .. } => "Agent/Input",
            AgentAction::Cancel => "Agent/Cancel",
            AgentAction::Approve { .. } => "Agent/Approve",
            AgentAction::ListMedia { .. } => "Agent/ListMedia",
        },
        SharedMessage::ListSessions => "ListSessions",
        SharedMessage::AgentCommand(_) => "AgentCommand",
    }
}

fn encode(message: SharedMessage) -> Vec<u8> {
    rkyv::to_bytes::<rkyv::rancor::Error>(&ClientMessage::Shared(message))
        .expect("encode")
        .to_vec()
}

#[test]
fn every_shared_variant_still_encodes_to_its_v2_bytes() {
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
    for (name, hex) in FROZEN_V2 {
        frozen.push((name, hex.to_string()));
    }

    assert_eq!(
        encoded, frozen,
        "the v2 wire format changed — bump ProtocolVersion rather than refreezing these bytes"
    );
}

/// The frozen bytes are a prefix, so this covers what they cannot: a full frame still decodes to
/// the same variant carrying the same payload.
#[test]
fn a_v2_frame_round_trips_with_its_payload_intact() {
    let bytes = encode(SharedMessage::agent(
        "s",
        AgentAction::Approve {
            call_id: "c".into(),
            decision: ApprovalDecision::Allow,
        },
    ));

    let decoded = rkyv::from_bytes::<ClientMessage, rkyv::rancor::Error>(&bytes).expect("decode");

    let ClientMessage::Shared(SharedMessage::Agent {
        sid,
        action: AgentAction::Approve { call_id, decision },
    }) = decoded
    else {
        panic!("decoded to the wrong variant");
    };
    assert_eq!(
        (sid.as_str(), call_id.as_str(), decision),
        ("s", "c", ApprovalDecision::Allow)
    );
}
