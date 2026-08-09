use super::*;

#[test]
fn a_base_url_takes_the_relay_host_and_the_allocated_port() {
    assert_eq!(
        Relay::new("https://relay.vmux.ai")
            .base_url_on(41003)
            .unwrap(),
        "https://relay.vmux.ai:41003"
    );
    // The relay's own control port is replaced, not appended to.
    assert_eq!(
        Relay::new("https://localhost:8787")
            .base_url_on(41003)
            .unwrap(),
        "https://localhost:41003"
    );
}

/// The phone can only pin the desktop's certificate if the fingerprint survives into both
/// pairing shapes — the QR-encoded URL and the deep link. Dropping it from either would
/// downgrade that phone to an unpinned connection with nothing to show for it.
#[test]
fn a_fingerprint_reaches_both_pairing_shapes() {
    let pairing = PairingInfo::new("https://localhost:41003", "secret", "abc123").unwrap();

    assert_eq!(
        pairing.url,
        "https://localhost:41003/#token=secret&fp=abc123"
    );
    assert_eq!(
        pairing.deep_link,
        "vmuxremote://pair?base=https%3A%2F%2Flocalhost%3A41003&token=secret&fp=abc123"
    );
}

#[test]
fn an_absent_fingerprint_leaves_both_shapes_well_formed() {
    let pairing = PairingInfo::new("https://localhost:41003", "secret", "").unwrap();

    assert_eq!(pairing.url, "https://localhost:41003/#token=secret");
    assert_eq!(
        pairing.deep_link,
        "vmuxremote://pair?base=https%3A%2F%2Flocalhost%3A41003&token=secret"
    );
}
