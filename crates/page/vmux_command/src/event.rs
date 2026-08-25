pub use vmux_wire::command_bar::*;

use vmux_core::PageMetadata;

#[derive(
    Clone,
    Debug,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct BookmarksCommandEvent {
    pub command: String,
    #[serde(default)]
    pub uuid: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub metadata: Option<PageMetadata>,
    #[serde(default)]
    pub folder: Option<String>,
}

#[cfg(host)]
#[derive(bevy::prelude::Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SearchEngineSetting(pub SearchEngine);

pub const LAYOUT_COMMAND_BAR_OPEN_EVENT: &str = "layout-command-bar-open";

pub const LAYOUT_COMMAND_BAR_CLOSE_EVENT: &str = "layout-command-bar-close";

#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct CommandBarPanelCloseEvent;

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct PanelPlacement {
    pub left: f64,
    pub top: f64,
    pub width: f64,
    pub height: f64,
}

pub const PANEL_MIN_WIDTH: f64 = 320.0;

pub const PANEL_MIN_HEIGHT: f64 = 120.0;

pub fn clamp_panel_placement(
    placement: PanelPlacement,
    viewport_width: f64,
    viewport_height: f64,
) -> PanelPlacement {
    let width = placement
        .width
        .clamp(PANEL_MIN_WIDTH, viewport_width.max(PANEL_MIN_WIDTH));
    let height = placement
        .height
        .clamp(PANEL_MIN_HEIGHT, viewport_height.max(PANEL_MIN_HEIGHT));
    PanelPlacement {
        left: placement.left.clamp(0.0, (viewport_width - width).max(0.0)),
        top: placement
            .top
            .clamp(0.0, (viewport_height - height).max(0.0)),
        width,
        height,
    }
}

#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct CommandBarPanelActiveEvent {
    pub active: bool,
}

#[cfg(test)]
mod panel_tests {
    use super::*;

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
}
