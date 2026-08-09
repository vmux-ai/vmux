use super::*;

/// A bar dragged past the edge has no chrome left to grab, so the clamp is the only thing
/// keeping it recoverable.
#[test]
fn dragging_the_panel_off_screen_keeps_it_reachable() {
    let dragged_past_the_corner = PanelPlacement {
        left: 5000.0,
        top: 5000.0,
        width: 576.0,
        height: 400.0,
    };

    let clamped = clamp_panel_placement(dragged_past_the_corner, 1440.0, 900.0);

    assert_eq!(clamped.left, 1440.0 - 576.0);
    assert_eq!(clamped.top, 900.0 - 400.0);
    assert_eq!(clamped.width, 576.0);
    assert_eq!(clamped.height, 400.0);

    let dragged_past_the_origin = PanelPlacement {
        left: -300.0,
        top: -80.0,
        ..dragged_past_the_corner
    };

    let clamped = clamp_panel_placement(dragged_past_the_origin, 1440.0, 900.0);

    assert_eq!(clamped.left, 0.0);
    assert_eq!(clamped.top, 0.0);
}

/// Resizing below the minimum collapses the bar to a sliver with no visible resize handle.
#[test]
fn resizing_the_panel_stops_at_the_minimum() {
    let collapsed = PanelPlacement {
        left: 10.0,
        top: 10.0,
        width: 10.0,
        height: 10.0,
    };

    let clamped = clamp_panel_placement(collapsed, 1440.0, 900.0);

    assert_eq!(clamped.width, PANEL_MIN_WIDTH);
    assert_eq!(clamped.height, PANEL_MIN_HEIGHT);
}

/// A window narrower than the minimum must still produce a placement inside it rather than a
/// negative offset.
#[test]
fn panel_survives_a_window_smaller_than_its_minimum() {
    let placement = PanelPlacement {
        left: 40.0,
        top: 40.0,
        width: 576.0,
        height: 400.0,
    };

    let clamped = clamp_panel_placement(placement, 200.0, 100.0);

    assert_eq!(clamped.left, 0.0);
    assert_eq!(clamped.top, 0.0);
    assert_eq!(clamped.width, PANEL_MIN_WIDTH);
    assert_eq!(clamped.height, PANEL_MIN_HEIGHT);
}

#[test]
fn main_cef_left_includes_side_sheet_gap_when_open() {
    let open = LayoutStateEvent {
        side_sheet_open: true,
        side_sheet_width: 280.0,
        pane_gap: 8.0,
        ..Default::default()
    };
    let closed = LayoutStateEvent {
        side_sheet_open: false,
        side_sheet_width: 280.0,
        pane_gap: 8.0,
        ..Default::default()
    };

    assert_eq!(open.main_cef_left(), 288.0);
    assert_eq!(closed.main_cef_left(), 0.0);
}

#[test]
fn main_cef_left_includes_effective_window_left_padding() {
    let closed = LayoutStateEvent {
        side_sheet_open: false,
        window_pad_left: 16.0,
        ..Default::default()
    };
    let open = LayoutStateEvent {
        side_sheet_open: true,
        side_sheet_width: 280.0,
        pane_gap: 8.0,
        window_pad_left: 16.0,
        ..Default::default()
    };

    assert_eq!(closed.main_cef_left(), 16.0);
    assert_eq!(open.main_cef_left(), 304.0);
}

#[test]
fn header_offsets_can_override_derived_window_padding() {
    let state = LayoutStateEvent {
        side_sheet_open: true,
        side_sheet_width: 220.0,
        pane_gap: 4.0,
        window_pad_left: 8.0,
        window_pad_top: 2.0,
        window_pad_right: 8.0,
        header_left: Some(230.0),
        header_top: Some(1.0),
        header_right: Some(9.0),
        ..Default::default()
    };

    assert_eq!(state.main_cef_left(), 232.0);
    assert_eq!(state.header_left(), 230.0);
    assert_eq!(state.header_top(), 1.0);
    assert_eq!(state.header_right(), 9.0);
}

#[test]
fn tab_row_pad_left_clears_traffic_lights_when_side_sheet_closed() {
    let closed = LayoutStateEvent {
        side_sheet_open: false,
        ..Default::default()
    };
    let open = LayoutStateEvent {
        side_sheet_open: true,
        ..Default::default()
    };

    assert_eq!(closed.tab_row_pad_left(), TRAFFIC_LIGHTS_PAD_PX);
    assert!(open.tab_row_pad_left() < TRAFFIC_LIGHTS_PAD_PX);
}

#[test]
fn header_visibility_tracks_header_open() {
    let open = LayoutStateEvent {
        header_open: true,
        side_sheet_open: false,
        ..Default::default()
    };
    let closed = LayoutStateEvent {
        header_open: false,
        side_sheet_open: true,
        ..Default::default()
    };

    assert!(open.header_visible());
    assert!(!closed.header_visible());
}

#[test]
fn header_command_event_rkyv_roundtrip() {
    let original = HeaderCommandEvent {
        header_command: "back".into(),
    };
    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&original).expect("ser");
    let recovered =
        rkyv::from_bytes::<HeaderCommandEvent, rkyv::rancor::Error>(&bytes).expect("de");
    assert_eq!(recovered.header_command, "back");
}

#[test]
fn tabs_command_event_rkyv_roundtrip() {
    let original = TabsCommandEvent {
        command: "switch-tab".into(),
        tab_id: Some("work".into()),
    };
    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&original).expect("ser");
    let recovered = rkyv::from_bytes::<TabsCommandEvent, rkyv::rancor::Error>(&bytes).expect("de");
    assert_eq!(recovered.command, "switch-tab");
    assert_eq!(recovered.tab_id.as_deref(), Some("work"));
}
