//! Indeterminate loading bar along the **bottom** of the pane: just above the status strip when the
//! strip is shown (active pane + focused window), otherwise along the **bottom of the full tile**
//! while loading or on the OSR placeholder.

use bevy::asset::{load_internal_asset, uuid_handle};
use bevy::pbr::{Material, MaterialPlugin};
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::render::render_resource::AsBindGroup;
use bevy::shader::ShaderRef;
use bevy_cef::prelude::WebviewExtendStandardMaterial;

const LOADING_BAR_SHADER: Handle<Shader> = uuid_handle!("b2c3d4e5-f678-01ab-cdef-234567890abc");

/// Linear RGBA tokens for the pane webview loading indicator (track + sweep shader uniforms).
pub mod color {
    use bevy::prelude::*;
    use vmux_ui::utils::color::{linear_rgba_to_vec4, PRIMARY};

    /// Dim band behind the moving highlight.
    pub const TRACK: LinearRgba = LinearRgba::new(0.06, 0.14, 0.26, 0.62);

    /// Moving segment — same linear accent as [`PRIMARY`](vmux_ui::utils::color::PRIMARY) (focus ring, highlights).
    pub const SWEEP: LinearRgba = PRIMARY;

    #[inline]
    pub fn track_vec4() -> Vec4 {
        linear_rgba_to_vec4(TRACK)
    }

    #[inline]
    pub fn sweep_vec4() -> Vec4 {
        linear_rgba_to_vec4(SWEEP)
    }
}

/// Height of the bar in **layout pixels**.
pub const LOADING_BAR_HEIGHT_PX: f32 = 3.0;

/// Multiplier on [`Time::elapsed_secs`] for shader phase so the sweep reads clearly in motion.
pub const LOADING_BAR_ANIM_TIME_SCALE: f32 = 1.45;

/// Added on top of [`PaneChromeStrip`]'s depth bias (`1_000_000 + pane_index`) so the bar draws
/// **on top** of the status strip.
pub const LOADING_BAR_DEPTH_BIAS_ABOVE_CHROME: f32 = 25.0;

/// Marker for the loading bar mesh attached to a pane ([`super::PaneChromeOwner`]).
#[derive(Component, Default, Debug, Clone, Copy, Reflect)]
#[reflect(Component, Default)]
pub struct PaneChromeLoadingBar;

/// Tracks pane entities (`VmuxWebview`) with a pending navigation so the loading bar can show after
/// refresh without relying on the 1×1 placeholder (CEF usually keeps the last frame until repaint).
#[derive(Resource, Default, Debug)]
pub struct PendingNavigationLoads(pub HashMap<Entity, f32>);

/// Unlit material: animated sweep across the bar width.
#[derive(Asset, TypePath, AsBindGroup, Clone)]
pub struct LoadingBarMaterial {
    /// `x` = elapsed time (s), `y` = width (layout px), `z` = height (layout px), `w` unused.
    #[uniform(0)]
    pub anim: Vec4,
    /// Linear RGBA for the static track (see [`TRACK`](crate::loading_bar::color::TRACK)).
    #[uniform(1)]
    pub track_rgba: Vec4,
    /// Linear RGBA for the moving sweep (see [`SWEEP`](crate::loading_bar::color::SWEEP)).
    #[uniform(2)]
    pub sweep_rgba: Vec4,
    pub alpha_mode: AlphaMode,
    pub depth_bias: f32,
}

impl Default for LoadingBarMaterial {
    fn default() -> Self {
        Self {
            anim: Vec4::ZERO,
            track_rgba: color::track_vec4(),
            sweep_rgba: color::sweep_vec4(),
            alpha_mode: AlphaMode::Blend,
            depth_bias: 1_000_000.0 + LOADING_BAR_DEPTH_BIAS_ABOVE_CHROME,
        }
    }
}

impl Material for LoadingBarMaterial {
    fn fragment_shader() -> ShaderRef {
        LOADING_BAR_SHADER.into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        self.alpha_mode
    }

    fn depth_bias(&self) -> f32 {
        self.depth_bias
    }

    fn enable_prepass() -> bool {
        false
    }
}

pub struct LoadingBarPlugin;

impl Plugin for LoadingBarPlugin {
    fn build(&self, app: &mut App) {
        load_internal_asset!(
            app,
            LOADING_BAR_SHADER,
            "./loading_bar.wgsl",
            Shader::from_wgsl
        );
        app.add_plugins(MaterialPlugin::<LoadingBarMaterial>::default());
    }
}

pub(crate) fn webview_surface_is_placeholder(
    images: &Assets<Image>,
    mat: &WebviewExtendStandardMaterial,
) -> bool {
    let Some(h) = mat.extension.surface.as_ref() else {
        return true;
    };
    images
        .get(h.id())
        .map(|img| img.width() <= 1 && img.height() <= 1)
        .unwrap_or(true)
}
