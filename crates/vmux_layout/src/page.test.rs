use super::*;

#[test]
fn download_pct_clamps_and_handles_zero_total() {
    assert_eq!(download_pct(0, 0), 0);
    assert_eq!(download_pct(50, 100), 50);
    assert_eq!(download_pct(250, 100), 100);
}

fn state(header_open: bool, side_sheet_open: bool) -> LayoutStateEvent {
    LayoutStateEvent {
        header_open,
        side_sheet_open,
        ..Default::default()
    }
}

#[test]
fn overlay_waits_for_layout_state() {
    assert!(!layout_overlay_ready(
        &state(false, false),
        false,
        true,
        true,
        true,
        true
    ));
}

#[test]
fn overlay_waits_for_header_state_when_header_visible() {
    let visible = state(true, false);

    assert!(!layout_overlay_ready(
        &visible, true, false, true, true, true
    ));
    assert!(!layout_overlay_ready(
        &visible, true, true, false, true, true
    ));
    assert!(layout_overlay_ready(&visible, true, true, true, true, true));
}

#[test]
fn overlay_waits_for_side_sheet_state_when_side_sheet_visible() {
    let visible = state(false, true);

    assert!(!layout_overlay_ready(
        &visible, true, true, true, false, true
    ));
    assert!(!layout_overlay_ready(
        &visible, true, true, true, true, false
    ));
    assert!(layout_overlay_ready(&visible, true, true, true, true, true));
}

#[test]
fn overlay_can_be_ready_when_overlay_is_closed() {
    assert!(layout_overlay_ready(
        &state(false, false),
        true,
        false,
        false,
        false,
        false
    ));
}
