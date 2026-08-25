use vmux_wire::protocol::{
    AgentAction, AgentAttachment, ApprovalDecision, ClientMessage, SharedAgentCommand,
    SharedMessage,
};

const FROZEN_PREFIX: usize = 48;

#[rustfmt::skip]
const FROZEN: [(&str, &str); 7] = [
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
