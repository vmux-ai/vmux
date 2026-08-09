use super::*;

#[test]
fn a_fingerprint_is_stable_and_key_specific() {
    let first = SelfSignedIdentity::generate(vec!["localhost".into()]).unwrap();
    let second = SelfSignedIdentity::generate(vec!["localhost".into()]).unwrap();

    assert_eq!(first.fingerprint.len(), 64);
    assert!(
        first
            .fingerprint
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
    );
    assert_ne!(
        first.fingerprint, second.fingerprint,
        "two independently generated identities must not share a fingerprint"
    );
}

/// Reloading from disk has to produce the same fingerprint the pairing link recorded, or every
/// paired phone silently stops trusting the desktop after a restart.
#[test]
fn an_identity_reloaded_from_pem_keeps_its_fingerprint() {
    let minted = SelfSignedIdentity::generate(vec!["localhost".into()]).unwrap();

    let reloaded = SelfSignedIdentity::from_pem(
        minted.certificate_pem.clone(),
        minted.private_key_pem.clone(),
    )
    .unwrap();

    assert_eq!(reloaded.fingerprint, minted.fingerprint);
    assert_eq!(
        SelfSignedIdentity::fingerprint_of_pem(&minted.certificate_pem).unwrap(),
        minted.fingerprint
    );
}

#[test]
fn configs_build_for_both_ends() {
    let identity =
        SelfSignedIdentity::generate(vec!["localhost".into(), "127.0.0.1".into()]).unwrap();

    identity.server_config().unwrap();
    Trust::Desktop {
        fingerprint: identity.fingerprint.clone(),
    }
    .client_config()
    .unwrap();
    Trust::Relay {
        host: "relay.vmux.ai".into(),
    }
    .client_config()
    .unwrap();
    Trust::Relay {
        host: "localhost".into(),
    }
    .client_config()
    .unwrap();
}

/// A public relay must not fall into the verify-nothing branch meant for a dev stack.
#[test]
fn only_private_relay_hosts_skip_verification() {
    assert!(Trust::is_local_development_host("localhost"));
    assert!(Trust::is_local_development_host("127.0.0.1"));
    assert!(Trust::is_local_development_host("192.168.1.4"));
    assert!(!Trust::is_local_development_host("relay.vmux.ai"));
    assert!(!Trust::is_local_development_host("8.8.8.8"));
}

#[test]
fn comparison_rejects_a_different_or_truncated_fingerprint() {
    let pin = PinnedCertificate {
        expected: "abcd".into(),
    };

    assert!(pin.matches("abcd"));
    assert!(!pin.matches("abce"));
    assert!(!pin.matches("abc"));
}
