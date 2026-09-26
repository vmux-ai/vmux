use vmux_api::protocol::{
    AgentAttachment, ApprovalDecision, ClientMessage, SharedAgentCommand, SharedMessage,
};

const FROZEN_PREFIX: usize = 48;

#[rustfmt::skip]
const FROZEN: [(&str, &str); 7] = [
    ("AgentAttach",    "320000000000000073ffffffffffffff0000000000000000000000000000000000000000000000000000000000000000"),
    ("AgentInput",     "320000000100000073ffffffffffffff74ffffffffffffff000000000000000000000000dcffffff0000000000000000"),
    ("AgentCancel",    "320000000200000073ffffffffffffff0000000000000000000000000000000000000000000000000000000000000000"),
    ("AgentApprove",   "320000000300000073ffffffffffffff63ffffffffffffff000000000000000000000000000000000000000000000000"),
    ("AgentListMedia", "320000000400000073ffffffffffffff71ffffffffffffff000000000000000000000000000000000000000000000000"),
    ("ListSessions",   "320000000500000000000000000000000000000000000000000000000000000000000000000000000000000000000000"),
    ("AgentCommand",   "320000000600000001000000000000000000000000000000000000000000000000000000000000000000000000000000"),
];

fn samples() -> Vec<SharedMessage> {
    vec![
        SharedMessage::AgentAttach { sid: "s".into() },
        SharedMessage::AgentInput {
            sid: "s".into(),
            text: "t".into(),
            context: None,
            attachments: Vec::<AgentAttachment>::new(),
            preferred_mode: None,
        },
        SharedMessage::AgentCancel { sid: "s".into() },
        SharedMessage::AgentApprove {
            sid: "s".into(),
            call_id: "c".into(),
            decision: ApprovalDecision::Allow,
        },
        SharedMessage::AgentListMedia {
            sid: "s".into(),
            query: "q".into(),
        },
        SharedMessage::ListSessions,
        SharedMessage::AgentCommand(SharedAgentCommand::ListAgents),
    ]
}

fn name_of(message: &SharedMessage) -> &'static str {
    match message {
        SharedMessage::AgentAttach { .. } => "AgentAttach",
        SharedMessage::AgentInput { .. } => "AgentInput",
        SharedMessage::AgentCancel { .. } => "AgentCancel",
        SharedMessage::AgentApprove { .. } => "AgentApprove",
        SharedMessage::AgentListMedia { .. } => "AgentListMedia",
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
fn every_shared_variant_still_encodes_to_its_frozen_bytes() {
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
    for (name, hex) in FROZEN {
        frozen.push((name, hex.to_string()));
    }

    assert_eq!(
        encoded, frozen,
        "the wire format changed — bump the ALPN rather than quietly refreezing these bytes"
    );
}

#[test]
fn a_frame_round_trips_with_its_payload_intact() {
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
