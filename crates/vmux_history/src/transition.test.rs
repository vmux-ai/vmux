use super::*;

fn no_qual() -> CefTransitionQualifiers {
    CefTransitionQualifiers::default()
}

#[test]
fn forward_back_wins_over_core() {
    let qual = CefTransitionQualifiers {
        forward_back: true,
        ..no_qual()
    };
    assert_eq!(
        map(CefTransitionCore::Explicit, qual),
        TransitionType::BackForward
    );
}

#[test]
fn server_redirect_wins_over_core() {
    let qual = CefTransitionQualifiers {
        server_redirect: true,
        ..no_qual()
    };
    assert_eq!(map(CefTransitionCore::Link, qual), TransitionType::Redirect);
}

#[test]
fn typed_from_explicit() {
    assert_eq!(
        map(CefTransitionCore::Explicit, no_qual()),
        TransitionType::Typed
    );
}

#[test]
fn typed_from_generated() {
    assert_eq!(
        map(CefTransitionCore::Generated, no_qual()),
        TransitionType::Typed
    );
}

#[test]
fn link_from_link_and_form_submit() {
    assert_eq!(
        map(CefTransitionCore::Link, no_qual()),
        TransitionType::Link
    );
    assert_eq!(
        map(CefTransitionCore::FormSubmit, no_qual()),
        TransitionType::Link
    );
}

#[test]
fn reload_maps_directly() {
    assert_eq!(
        map(CefTransitionCore::Reload, no_qual()),
        TransitionType::Reload
    );
}

#[test]
fn subframe_falls_to_other() {
    assert_eq!(
        map(CefTransitionCore::AutoSubframe, no_qual()),
        TransitionType::Other
    );
}
