//! The command-bar wire vocabulary.
//!
//! The types themselves live in [`vmux_wire::command_bar`] so hosts that cannot link Bevy — the
//! native mobile client — can still render a launcher. This module re-exports them and adds the
//! Bevy-side resource wrapper.

pub use vmux_wire::command_bar::*;

use vmux_core::PageMetadata;

/// Page→host: act on a bookmark.
///
/// Lives beside the command vocabulary rather than with the layout page because the launcher
/// emits one too, and the launcher is this crate's.
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

/// The user's configured search provider, as an ECS resource.
///
/// [`SearchEngine`] itself is a portable wire type, so the `Resource` marker lives on this
/// wrapper rather than on the enum.
#[cfg(host)]
#[derive(bevy::prelude::Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SearchEngineSetting(pub SearchEngine);

/// Channel the host pushes a payload on to open the panel inside another page.
///
/// Distinct from the command bar page's own open event so the layout page and the start page can
/// be addressed separately.
pub const LAYOUT_COMMAND_BAR_OPEN_EVENT: &str = "layout-command-bar-open";

/// Channel the host closes the panel on.
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

/// Where the user has dragged and sized the floating command bar, in CSS pixels.
///
/// Absent until the first drag or resize, so an untouched bar keeps its centred default and
/// content-driven height.
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

/// Keep the bar inside the window and above a usable minimum.
///
/// Without the clamp a drag can park the bar past the edge, where it is unreachable and there is
/// no chrome to drag it back by.
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

/// Layout page -> host: the command bar panel took or released the keyboard.
///
/// Mirrors `BookmarkTextInputEvent`: while the panel holds a focused DOM field the layout shell
/// must own `KeyboardOwner`, or keystrokes go to the focused pane instead.
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
}
