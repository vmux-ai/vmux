use vmux_api::protocol::{
    AgentAttachment, AgentListModels, AgentNewChat, AgentRequest, AgentSelectModel, AgentSetEffort,
    ApprovalDecision, ClientMessage, SharedMessage,
};

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
    let samples = vec![
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
            request: AgentNewChat {
                client_op_id: vmux_api::session::ClientOpId::new("o"),
                prompt: "p".into(),
                agent_url: None,
            },
        },
        SharedMessage::AgentListAgents,
        SharedMessage::AgentListTeam,
        SharedMessage::AgentListModels {
            request: AgentListModels { sid: "s".into() },
        },
        SharedMessage::AgentSelectModel {
            request: AgentSelectModel {
                sid: "s".into(),
                model_id: "m".into(),
            },
        },
        SharedMessage::AgentSetEffort {
            request: AgentSetEffort {
                sid: "s".into(),
                level: "l".into(),
            },
        },
    ];
    let mut encoded = Vec::new();
    for message in samples {
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

#[test]
fn canonical_payloads_decode_and_reencode_complete_vmux11_frames_from_main_4aeade6b() {
    let fixtures: &[(&[u8], &[u8], &str, &str)] = &[
        (
            include_bytes!("fixtures/remote/legacy-0.direct.bin"),
            include_bytes!("fixtures/remote/legacy-0.client.bin"),
            "agent_new_chat@1",
            r#"{"client_op_id":"legacy-mobile-operation-with-a-long-identity","prompt":"Long prompt: 日本語 and Unicode 🦀 remain intact beyond inline archive storage","agent_url":"https://agents.example.test/providers/a-long-provider-identity"}"#,
        ),
        (
            include_bytes!("fixtures/remote/legacy-1.direct.bin"),
            include_bytes!("fixtures/remote/legacy-1.client.bin"),
            "agent_new_chat@1",
            r#"{"client_op_id":"another-long-mobile-operation-identity","prompt":"A different long prompt with an absent provider URL","agent_url":null}"#,
        ),
        (
            include_bytes!("fixtures/remote/legacy-2.direct.bin"),
            include_bytes!("fixtures/remote/legacy-2.client.bin"),
            "agent_list_models@1",
            r#"{"sid":"session-with-a-long-canonical-identity"}"#,
        ),
        (
            include_bytes!("fixtures/remote/legacy-3.direct.bin"),
            include_bytes!("fixtures/remote/legacy-3.client.bin"),
            "agent_select_model@1",
            r#"{"sid":"session-with-a-long-canonical-identity","model_id":"provider/model-with-long-identity-日本語"}"#,
        ),
        (
            include_bytes!("fixtures/remote/legacy-4.direct.bin"),
            include_bytes!("fixtures/remote/legacy-4.client.bin"),
            "agent_set_effort@1",
            r#"{"sid":"session-with-a-long-canonical-identity","level":"high-effort-with-a-long-provider-value"}"#,
        ),
    ];
    for (direct, wrapped, selector, body) in fixtures {
        let mut direct_aligned = rkyv::util::AlignedVec::<16>::new();
        direct_aligned.extend_from_slice(direct);
        let direct_value =
            rkyv::from_bytes::<SharedMessage, rkyv::rancor::Error>(&direct_aligned).unwrap();
        assert_eq!(
            rkyv::to_bytes::<rkyv::rancor::Error>(&direct_value)
                .unwrap()
                .as_slice(),
            *direct
        );
        let mut wrapped_aligned = rkyv::util::AlignedVec::<16>::new();
        wrapped_aligned.extend_from_slice(wrapped);
        let wrapped_value =
            rkyv::from_bytes::<ClientMessage, rkyv::rancor::Error>(&wrapped_aligned).unwrap();
        assert_eq!(
            rkyv::to_bytes::<rkyv::rancor::Error>(&wrapped_value)
                .unwrap()
                .as_slice(),
            *wrapped
        );
        let ClientMessage::Shared(wrapped_value) = wrapped_value else {
            panic!("wrong historical envelope");
        };
        for value in [direct_value, wrapped_value] {
            let encoded = match value {
                SharedMessage::AgentNewChat { request } => AgentRequest::encode(&request),
                SharedMessage::AgentListModels { request } => AgentRequest::encode(&request),
                SharedMessage::AgentSelectModel { request } => AgentRequest::encode(&request),
                SharedMessage::AgentSetEffort { request } => AgentRequest::encode(&request),
                _ => panic!("wrong historical request"),
            }
            .unwrap();
            assert_eq!(encoded.id, *selector);
            assert_eq!(
                serde_json::from_slice::<serde_json::Value>(&encoded.body).unwrap(),
                serde_json::from_str::<serde_json::Value>(body).unwrap()
            );
        }
    }
}
