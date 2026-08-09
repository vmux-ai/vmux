use super::should_emit_update;
use vmux_layout::UpdateState;

fn downloading(v: &str) -> UpdateState {
    UpdateState::Downloading {
        version: v.into(),
        downloaded: 1,
        total: 2,
    }
}

#[test]
fn emits_on_change() {
    assert!(should_emit_update(
        &UpdateState::Ready {
            version: "v2".into()
        },
        &None,
        false
    ));
    assert!(should_emit_update(
        &UpdateState::Idle,
        &Some(downloading("v2")),
        false
    ));
}

#[test]
fn no_emit_when_unchanged_and_no_page_ready() {
    assert!(!should_emit_update(
        &UpdateState::Idle,
        &Some(UpdateState::Idle),
        false
    ));
    let r = UpdateState::Ready {
        version: "v2".into(),
    };
    assert!(!should_emit_update(&r, &Some(r.clone()), false));
}

#[test]
fn re_emits_non_idle_on_page_ready() {
    let r = UpdateState::Ready {
        version: "v2".into(),
    };
    assert!(should_emit_update(&r, &Some(r.clone()), true));
    assert!(!should_emit_update(
        &UpdateState::Idle,
        &Some(UpdateState::Idle),
        true
    ));
}
