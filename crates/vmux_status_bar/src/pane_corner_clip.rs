//! Corner clip mode for the **status bar** chrome mesh ([`PaneChromeStrip`](vmux_layout::PaneChromeStrip)).
//!
//! The strip is the bottom overlay on each pane; it should match the pane’s outer bottom edge, so
//! only **bottom** corners are rounded (toward gaps). Main pane webviews use
//! [`PANE_CORNER_CLIP_FULL`](vmux_core::pane_corner_clip::PANE_CORNER_CLIP_FULL) instead.

pub use vmux_core::pane_corner_clip::PANE_CORNER_CLIP_STATUS_BAR_BOTTOM;
