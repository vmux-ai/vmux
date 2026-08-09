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
