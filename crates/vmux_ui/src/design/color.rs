//! Linear RGBA tokens for shaders and [`bevy::prelude::Color`].
//!
//! Values are **linear** sRGB for correct blending with GPU materials; use
//! [`LinearRgba`] for documentation and [`linear_rgba_to_vec4`] for WGSL uniforms.

use bevy::prelude::*;

/// Pack a [`LinearRgba`] token into a `vec4` uniform (`rgb` + `a`).
#[inline]
pub fn linear_rgba_to_vec4(c: LinearRgba) -> Vec4 {
    c.to_vec4()
}

/// Primary brand accent — focus rings, active pane chrome, key highlights (linear sRGB).
pub const PRIMARY: LinearRgba = LinearRgba::new(0.35, 0.55, 1.0, 1.0);

#[inline]
pub fn primary_vec4() -> Vec4 {
    linear_rgba_to_vec4(PRIMARY)
}

/// Active pane focus ring in the CEF webview shader — same linear accent as [`PRIMARY`].
#[inline]
pub fn active_pane_border_vec4() -> Vec4 {
    linear_rgba_to_vec4(PRIMARY)
}

pub mod loading_bar {
    //! Pane loading indicator (track + sweep), drawn just above the status strip.

    use super::linear_rgba_to_vec4;
    use bevy::prelude::*;

    /// Dim band behind the moving highlight.
    pub const TRACK: LinearRgba = LinearRgba::new(0.06, 0.14, 0.26, 0.62);

    /// Moving segment — same linear accent as [`super::PRIMARY`] (focus ring, highlights).
    pub const SWEEP: LinearRgba = super::PRIMARY;

    #[inline]
    pub fn track_vec4() -> Vec4 {
        linear_rgba_to_vec4(TRACK)
    }

    #[inline]
    pub fn sweep_vec4() -> Vec4 {
        linear_rgba_to_vec4(SWEEP)
    }
}
