//! Editor viewport math now lives in `vmux_core::scroll` (shared with the
//! terminal). Re-exported here so existing `crate::viewport::*` call sites are
//! unchanged.
pub use vmux_core::scroll::{clamp_top_line, rows_from_viewport, visible_slice, window_range};
