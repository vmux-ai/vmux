use super::*;

/// The fingerprint is the whole basis for trusting the desktop's certificate. If it were
/// dropped while parsing, the phone would silently fall back to an unpinned connection —
/// a downgrade with no visible symptom, so both pairing shapes are covered.
#[test]
fn a_pairing_link_carries_the_certificate_fingerprint() {
    let expected = "c620a502885ddf230420184cc3a1b190792c14c1049ab76a6a63596054a1025e";

    let pasted = parse_pairing_url(&format!(
        "https://mac.example.ts.net/#token=secret&fp={expected}"
    ))
    .unwrap();
    let deep_link = parse_pairing_url(&format!(
        "vmuxremote://pair?base=https%3A%2F%2Fmac.example.ts.net&token=secret&fp={expected}"
    ))
    .unwrap();

    assert_eq!(pasted.fingerprint, expected);
    assert_eq!(deep_link.fingerprint, expected);
    assert_eq!(
        pasted.token, "secret",
        "the token must survive alongside it"
    );
}

/// A link with no fingerprint parses but cannot be used: there is no unpinned transport left
/// to fall back to. It has to fail here, at the point of use, rather than at parse time —
/// that is what lets the phone say "scan again" instead of "malformed link".
#[test]
fn a_link_without_a_fingerprint_parses_but_cannot_be_dialled() {
    let credentials = parse_pairing_url("https://mac.example.ts.net/#token=secret").unwrap();

    assert!(credentials.fingerprint.is_empty());
    assert_eq!(credentials.token, "secret");
    assert!(
        Api::new(credentials).is_err(),
        "an unpinned pairing must be refused, not silently downgraded"
    );
}

#[test]
fn parses_pairing_url() {
    assert_eq!(
        parse_pairing_url("paste into Vmux: https://mac.example.ts.net/#token=secret").unwrap(),
        Credentials {
            base_url: "https://mac.example.ts.net".to_string(),
            token: "secret".to_string(),
            fingerprint: String::new(),
        }
    );
}

#[test]
fn parses_pairing_deep_link() {
    assert_eq!(
        parse_pairing_url(
            "vmuxremote://pair?base=https%3A%2F%2Fmac.example.ts.net%3A54821&token=secret"
        )
        .unwrap(),
        Credentials {
            base_url: "https://mac.example.ts.net:54821".to_string(),
            token: "secret".to_string(),
            fingerprint: String::new(),
        }
    );
}

#[test]
fn pairing_url_preserves_relay_path() {
    assert_eq!(
        parse_pairing_url("http://localhost:8787/r/device-1/#token=secret").unwrap(),
        Credentials {
            base_url: "http://localhost:8787/r/device-1".to_string(),
            token: "secret".to_string(),
            fingerprint: String::new(),
        }
    );
}

fn sample_events() -> Vec<RoomEvent> {
    vmux_wire::room::RoomEvent::from_messages(
        "s",
        0,
        &[
            Message::user("hello"),
            Message::Assistant {
                blocks: vec![AssistantBlock::Thinking("working".to_string())],
            },
            Message::ToolResult {
                call_id: "tool-1".to_string(),
                content: "done".to_string(),
                is_error: false,
            },
            Message::Assistant {
                blocks: vec![AssistantBlock::Text("answer".to_string())],
            },
        ],
    )
}

#[test]
fn groups_agent_activity_into_one_turn() {
    let items = group_messages(sample_events(), "", false);

    assert_eq!(items.len(), 2);
    assert!(matches!(items[0], ChatItem::User { .. }));
    assert!(matches!(
        &items[1],
        ChatItem::Turn(turn) if turn.blocks.len() == 3 && !turn.running
    ));
}

#[test]
fn streaming_delta_extends_the_live_turn() {
    let items = group_messages(sample_events(), "partial", true);

    let ChatItem::Turn(turn) = &items[1] else {
        panic!("expected a turn");
    };
    assert!(turn.running);
    assert_eq!(
        turn.blocks.last(),
        Some(&ChatBlock::Text("partial".to_string()))
    );
}
