use vmux_api::protocol::{AgentAttachment, ApprovalDecision, ClientMessage, SharedMessage};

const FROZEN_PREFIX: usize = 48;

#[rustfmt::skip]
const FROZEN: [(&str, &str); 12] = [
    ("AgentAttach",      "1f0000000000000073ffffffffffffff0000000000000000000000000000000000000000000000000000000000000000"),
    ("AgentInput",       "1f0000000100000073ffffffffffffff74ffffffffffffff000000000000000000000000dcffffff0000000000000000"),
    ("AgentCancel",      "1f0000000200000073ffffffffffffff0000000000000000000000000000000000000000000000000000000000000000"),
    ("AgentApprove",     "1f0000000300000073ffffffffffffff63ffffffffffffff000000000000000000000000000000000000000000000000"),
    ("AgentListMedia",   "1f0000000400000073ffffffffffffff71ffffffffffffff000000000000000000000000000000000000000000000000"),
    ("ListSessions",     "1f0000000500000000000000000000000000000000000000000000000000000000000000000000000000000000000000"),
    ("AgentNewChat",     "1f000000060000006fffffffffffffff70ffffffffffffff000000000000000000000000000000000000000000000000"),
    ("AgentListAgents",  "1f0000000700000000000000000000000000000000000000000000000000000000000000000000000000000000000000"),
    ("AgentListTeam",    "1f0000000800000000000000000000000000000000000000000000000000000000000000000000000000000000000000"),
    ("AgentListModels",  "1f0000000900000073ffffffffffffff0000000000000000000000000000000000000000000000000000000000000000"),
    ("AgentSelectModel", "1f0000000a00000073ffffffffffffff6dffffffffffffff000000000000000000000000000000000000000000000000"),
    ("AgentSetEffort",   "1f0000000b00000073ffffffffffffff6cffffffffffffff000000000000000000000000000000000000000000000000"),
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
        SharedMessage::AgentNewChat {
            client_op_id: vmux_api::room::ClientOpId::new("o"),
            prompt: "p".into(),
            agent_url: None,
        },
        SharedMessage::AgentListAgents,
        SharedMessage::AgentListTeam,
        SharedMessage::AgentListModels { sid: "s".into() },
        SharedMessage::AgentSelectModel {
            sid: "s".into(),
            model_id: "m".into(),
        },
        SharedMessage::AgentSetEffort {
            sid: "s".into(),
            level: "l".into(),
        },
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
        SharedMessage::AgentNewChat { .. } => "AgentNewChat",
        SharedMessage::AgentListAgents => "AgentListAgents",
        SharedMessage::AgentListTeam => "AgentListTeam",
        SharedMessage::AgentListModels { .. } => "AgentListModels",
        SharedMessage::AgentSelectModel { .. } => "AgentSelectModel",
        SharedMessage::AgentSetEffort { .. } => "AgentSetEffort",
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
