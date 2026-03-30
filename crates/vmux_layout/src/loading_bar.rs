//! Indeterminate loading bar along the **bottom edge of the main pane content** (flush above the
//! status strip, not overlapping it) while loading or on the OSR placeholder.

use bevy::asset::{load_internal_asset, uuid_handle};
use bevy::platform::collections::HashMap;
use bevy::pbr::{Material, MaterialPlugin};
use bevy::prelude::*;
use bevy::render::render_resource::AsBindGroup;
use bevy::shader::ShaderRef;
use bevy_cef::prelude::WebviewExtendStandardMaterial;
use vmux_ui::design::color;

const LOADING_BAR_SHADER: Handle<Shader> = uuid_handle!("b2c3d4e5-f678-01ab-cdef-234567890abc");

/// Height of the bar in **layout pixels** (within the main pane area, above the status strip).
pub const LOADING_BAR_HEIGHT_PX: f32 = 3.0;

/// Multiplier on [`Time::elapsed_secs`] for shader phase so the sweep reads clearly in motion.
pub const LOADING_BAR_ANIM_TIME_SCALE: f32 = 1.45;

/// Added on top of each pane’s base depth bias so the bar draws above the main webview but **below**
/// [`super::PaneChromeStrip`] (~1_000_000 + i).
pub const LOADING_BAR_DEPTH_BIAS_ABOVE_PANE: f32 = 150.0;

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
    /// Linear RGBA for the static track (see [`vmux_ui::design::color::loading_bar::TRACK`]).
    #[uniform(1)]
    pub track_rgba: Vec4,
    /// Linear RGBA for the moving sweep (see [`vmux_ui::design::color::loading_bar::SWEEP`]).
    #[uniform(2)]
    pub sweep_rgba: Vec4,
    pub alpha_mode: AlphaMode,
    pub depth_bias: f32,
}

impl Default for LoadingBarMaterial {
    fn default() -> Self {
        Self {
            anim: Vec4::ZERO,
            track_rgba: color::loading_bar::track_vec4(),
            sweep_rgba: color::loading_bar::sweep_vec4(),
            alpha_mode: AlphaMode::Blend,
            depth_bias: LOADING_BAR_DEPTH_BIAS_ABOVE_PANE,
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
