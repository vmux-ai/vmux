//! `WebviewMaterial::pane_corner_clip` / shader `webview_corner.w` (see `bevy_cef` patch).

/// All corners use `pane_border_radius_px` (main pane webviews).
pub const PANE_CORNER_CLIP_FULL: f32 = 0.0;
/// Only bottom corners rounded (status strip: outer edge toward pane gaps).
pub const PANE_CORNER_CLIP_STATUS_BAR_BOTTOM: f32 = 1.0;
